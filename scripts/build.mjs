import { createHash, randomUUID } from "node:crypto";
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { copyFile, mkdir, readFile, readdir, rename, rm, stat, writeFile } from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { findExecutable, prepareWindowsMsvcEnvironment } from "./tauri-env.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceTauriDir = join(root, "src-tauri");
const target = "x86_64-pc-windows-msvc";
const fast = process.argv.slice(2).includes("--fast");
const startedAt = Date.now();

class BuildError extends Error {}

function stage(message) {
  process.stdout.write(`\n📦 ${message}\n`);
}

function fail(message) {
  throw new BuildError(message);
}

function run(executable, args, { cwd = root, env, capture = false } = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(executable, args, {
      cwd,
      env,
      shell: process.platform === "win32" && executable.toLowerCase().endsWith(".cmd"),
      windowsHide: true,
      stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit",
    });
    let stdout = "";
    let stderr = "";
    if (capture) {
      child.stdout.setEncoding("utf8").on("data", (chunk) => { stdout += chunk; });
      child.stderr.setEncoding("utf8").on("data", (chunk) => { stderr += chunk; });
    }
    child.on("error", rejectRun);
    child.on("exit", (code, signal) => {
      const result = { code: code ?? 1, stdout, stderr, signal };
      if (result.code === 0) resolveRun(result);
      else rejectRun(Object.assign(new Error(`${basename(executable)} terminou com código ${result.code}${signal ? ` (${signal})` : ""}.`), result));
    });
  });
}

async function commandOutput(executable, args, options = {}) {
  try {
    return (await run(executable, args, { ...options, capture: true })).stdout.trim();
  } catch (error) {
    const detail = error.stderr?.trim() || error.stdout?.trim();
    throw new BuildError(`${basename(executable)} ${args.join(" ")} falhou.${detail ? `\n${detail}` : ` ${error.message}`}`);
  }
}

function parseVersion(version) {
  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(version || "");
  if (!match) fail(`Versão inválida: "${version}". Use MAJOR.MINOR.PATCH, por exemplo 0.2.0.`);
  return match.slice(1).map(Number);
}

function versionAtLeast(actual, minimum) {
  const current = actual.split(".").map(Number);
  const required = minimum.split(".").map(Number);
  for (let index = 0; index < 3; index += 1) {
    if ((current[index] || 0) > (required[index] || 0)) return true;
    if ((current[index] || 0) < (required[index] || 0)) return false;
  }
  return true;
}

function cargoPackageVersion(manifest) {
  const match = /^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m.exec(manifest);
  if (!match) fail("Não encontrei a versão do pacote em src-tauri/Cargo.toml.");
  return match[1];
}

function configVersion(config) {
  if (typeof config.version !== "string" || !config.version.trim()) {
    fail("src-tauri/tauri.conf.json precisa definir a versão do aplicativo.");
  }
  if (/\.json$/i.test(config.version)) {
    const versionFile = resolve(sourceTauriDir, config.version);
    const packageJsonPath = resolve(root, "package.json");
    if (versionFile.toLowerCase() !== packageJsonPath.toLowerCase()) {
      fail(`A versão do Tauri deve apontar para ../package.json; configuração atual: ${config.version}.`);
    }
    return JSON.parse(readFileSync(versionFile, "utf8")).version;
  }
  return config.version;
}

async function validateVersions() {
  const [packageText, configText, manifest] = await Promise.all([
    readFile(join(root, "package.json"), "utf8"),
    readFile(join(sourceTauriDir, "tauri.conf.json"), "utf8"),
    readFile(join(sourceTauriDir, "Cargo.toml"), "utf8"),
  ]);
  const packageJson = JSON.parse(packageText);
  const config = JSON.parse(configText);
  const packageVersion = packageJson.version;
  const configuredVersion = configVersion(config);
  const rustVersion = cargoPackageVersion(manifest);
  parseVersion(packageVersion);
  if (configuredVersion !== packageVersion || rustVersion !== packageVersion) {
    fail(`Versões divergentes: package.json=${packageVersion}, tauri.conf.json=${configuredVersion}, Cargo.toml=${rustVersion}. Execute npm run version:set -- <MAJOR.MINOR.PATCH>.`);
  }
  return { packageJson, version: packageVersion };
}

async function prechecks(env) {
  stage("Pré-checagens do ambiente e da versão");
  const { packageJson, version } = await validateVersions();
  const nodeVersion = process.versions.node;
  const minimumNode = /^(?:>=)?(\d+\.\d+\.\d+)$/.exec(packageJson.engines?.node || "")?.[1];
  if (!minimumNode) fail("package.json precisa definir engines.node como uma versão mínima, por exemplo >=22.12.0.");
  if (!versionAtLeast(nodeVersion, minimumNode)) {
    fail(`Node.js ${nodeVersion} não atende ao mínimo ${minimumNode} definido em package.json.`);
  }

  const cargo = findExecutable(process.platform === "win32" ? "cargo.exe" : "cargo", env);
  const rustc = findExecutable(process.platform === "win32" ? "rustc.exe" : "rustc", env);
  const rustup = findExecutable(process.platform === "win32" ? "rustup.exe" : "rustup", env);
  if (!cargo || !rustc || !rustup) {
    fail("Não encontrei cargo, rustc e rustup. Instale Rust com rustup e a toolchain MSVC antes de gerar o instalador.");
  }
  const installedTargets = await commandOutput(rustup, ["target", "list", "--installed"], { env });
  if (!installedTargets.split(/\r?\n/).includes(target)) {
    fail(`O target Rust ${target} não está instalado. Execute: rustup target add ${target}`);
  }
  if (process.platform === "win32" && (!findExecutable("cl.exe", env) || !findExecutable("link.exe", env))) {
    fail("Não encontrei cl.exe e link.exe. Instale o workload Desktop development with C++ do Visual Studio.");
  }

  const rustVersionText = await commandOutput(rustc, ["--version"], { env });
  const git = findExecutable(process.platform === "win32" ? "git.exe" : "git", env);
  if (!git) fail("Git é necessário para registrar o commit de origem em BUILD_INFO.txt.");
  const [commit, workingTree] = await Promise.all([
    commandOutput(git, ["rev-parse", "--short=12", "HEAD"], { cwd: root, env }),
    commandOutput(git, ["status", "--porcelain", "--untracked-files=normal"], { cwd: root, env }),
  ]);
  const cargoMetadata = await commandOutput(cargo, ["metadata", "--no-deps", "--format-version=1"], { cwd: sourceTauriDir, env });
  const cargoTargetDirectory = JSON.parse(cargoMetadata).target_directory;
  return {
    cargo,
    commit,
    dirty: workingTree.length > 0,
    packageJson,
    rustVersionText,
    targetDirectory: cargoTargetDirectory,
    version,
  };
}

async function runNpm(args, env) {
  const npmExecPath = env.npm_execpath;
  if (npmExecPath) return run(process.execPath, [npmExecPath, ...args], { env });
  const npmCommand = findExecutable(process.platform === "win32" ? "npm.cmd" : "npm", env);
  if (!npmCommand) fail("Não encontrei npm para executar os testes definidos no package.json.");
  return run(npmCommand, args, { env });
}

async function runTests(info, env) {
  stage("Executando verificações de frontend e Rust");
  const tsc = join(root, "node_modules", "typescript", "bin", "tsc");
  if (!await fileExists(tsc)) fail("Dependências do frontend ausentes. Execute npm install antes do build.");
  await run(process.execPath, [tsc, "--noEmit"], { env });
  await runNpm(["run", "test:extractor"], env);
  await runNpm(["run", "test:ui"], env);
  await run(info.cargo, ["test", "--release", "--manifest-path", join(sourceTauriDir, "Cargo.toml")], { env });
  await run(info.cargo, ["clippy", "--manifest-path", join(sourceTauriDir, "Cargo.toml"), "--", "-D", "warnings"], { env });
}

async function fileExists(path) {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

function formatMiB(bytes) {
  return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
}

async function makeArtifacts(info, env) {
  stage("Gerando instalador NSIS e executável portable x64");
  const tauriCli = join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
  if (!await fileExists(tauriCli)) fail("Dependência @tauri-apps/cli ausente. Execute npm install antes do build.");
  await run(process.execPath, [tauriCli, "build", "--bundles", "nsis", "--target", target], { env });

  const releaseDir = join(info.targetDirectory, target, "release");
  const nsisDir = join(releaseDir, "bundle", "nsis");
  if (!await fileExists(nsisDir)) fail(`O instalador NSIS não foi gerado em ${nsisDir}.`);
  const installerSourceName = `Garimpo Livre_${info.version}_x64-setup.exe`;
  const installerSource = join(nsisDir, installerSourceName);
  if (!await fileExists(installerSource)) {
    fail(`O instalador NSIS da versão ${info.version} não foi encontrado: ${installerSource}`);
  }
  const portableSource = join(releaseDir, "garimpo-livre.exe");
  if (!await fileExists(portableSource)) fail(`O executável Rust não foi encontrado: ${portableSource}`);

  const installerName = `GarimpoLivre_${info.version}_x64-setup.exe`;
  const portableName = `GarimpoLivre_${info.version}_x64-portable.exe`;
  const releaseRoot = join(root, "release");
  const outputDir = join(releaseRoot, info.version);
  const stagingDir = join(releaseRoot, `.${info.version}.tmp-${process.pid}-${randomUUID()}`);

  await mkdir(stagingDir, { recursive: true });
  try {
    const installerPath = join(stagingDir, installerName);
    const portablePath = join(stagingDir, portableName);
    await copyFile(installerSource, installerPath);
    await copyFile(portableSource, portablePath);
    const [installerHash, portableHash, installerStat, portableStat] = await Promise.all([
      sha256(installerPath),
      sha256(portablePath),
      stat(installerPath),
      stat(portablePath),
    ]);
    await writeFile(join(stagingDir, "SHA256SUMS.txt"), `${installerHash}  ${installerName}\n${portableHash}  ${portableName}\n`, "utf8");
    await writeFile(join(stagingDir, "BUILD_INFO.txt"), [
      `Versão: ${info.version}`,
      `Commit: ${info.commit}`,
      `Árvore de trabalho: ${info.dirty ? "dirty" : "clean"}`,
      `Data do build (UTC): ${new Date().toISOString()}`,
      `Node.js: ${process.version}`,
      `Rust: ${info.rustVersionText}`,
      `Target: ${target}`,
      `Instalador NSIS: ${installerStat.size} bytes (${formatMiB(installerStat.size)})`,
      `Executável portable: ${portableStat.size} bytes (${formatMiB(portableStat.size)})`,
      "",
    ].join("\n"), "utf8");

    await mkdir(releaseRoot, { recursive: true });
    await mkdir(outputDir, { recursive: true });
    for (const fileName of [installerName, portableName, "SHA256SUMS.txt", "BUILD_INFO.txt"]) {
      const staged = join(stagingDir, fileName);
      const temporary = join(outputDir, `.${fileName}.tmp-${process.pid}`);
      await copyFile(staged, temporary);
      await rename(temporary, join(outputDir, fileName));
    }
    return {
      files: [installerName, portableName, "SHA256SUMS.txt", "BUILD_INFO.txt"],
      installerBytes: installerStat.size,
      outputDir,
      portableBytes: portableStat.size,
    };
  } finally {
    await rm(stagingDir, { recursive: true, force: true });
  }
}

async function sha256(path) {
  const content = await readFile(path);
  return createHash("sha256").update(content).digest("hex");
}

async function main() {
  const unknownArgs = process.argv.slice(2).filter((argument) => argument !== "--fast");
  if (unknownArgs.length) fail(`Argumento não reconhecido: ${unknownArgs.join(" ")}. Use somente --fast para pular os testes.`);

  let env;
  try {
    env = prepareWindowsMsvcEnvironment();
  } catch (error) {
    fail(error.message);
  }

  const info = await prechecks(env);
  if (info.dirty) process.stdout.write("⚠️ A árvore Git contém alterações locais; o build seguirá e marcará dirty em BUILD_INFO.txt.\n");
  if (fast) {
    process.stdout.write("⚠️ Modo rápido: testes automáticos foram pulados.\n");
  } else {
    await runTests(info, env);
  }
  const artifacts = await makeArtifacts(info, env);
  const elapsedSeconds = ((Date.now() - startedAt) / 1000).toFixed(1);

  process.stdout.write(`\n✅ Build concluído em ${elapsedSeconds}s\n`);
  for (const fileName of artifacts.files) {
    const path = join(artifacts.outputDir, fileName);
    const bytes = (await stat(path)).size;
    process.stdout.write(`📄 ${path} — ${bytes} bytes${fileName.endsWith(".exe") ? ` (${formatMiB(bytes)})` : ""}\n`);
  }
  if (artifacts.installerBytes > 20 * 1024 * 1024) {
    process.stdout.write(`⚠️ O instalador ultrapassa a referência de 20 MiB: ${formatMiB(artifacts.installerBytes)}.\n`);
  }
}

main().catch((error) => {
  process.stderr.write(`\n❌ Build interrompido: ${error.message}\n`);
  process.exitCode = 1;
});
