import { spawn } from "node:child_process";
import { readFile, writeFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { findExecutable, prepareWindowsMsvcEnvironment } from "./tauri-env.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const packagePath = join(root, "package.json");
const manifestPath = join(root, "src-tauri", "Cargo.toml");
const lockPath = join(root, "src-tauri", "Cargo.lock");

function run(executable, args, env) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(executable, args, { cwd: root, env, stdio: "inherit", windowsHide: true });
    child.on("error", rejectRun);
    child.on("exit", (code) => code === 0
      ? resolveRun()
      : rejectRun(new Error(`${executable} terminou com código ${code ?? 1}.`)));
  });
}

async function main() {
  const version = process.argv[2];
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version || "")) {
    throw new Error("Informe a nova versão no formato MAJOR.MINOR.PATCH, por exemplo: npm run version:set -- 0.2.0");
  }

  const env = prepareWindowsMsvcEnvironment();
  const cargo = findExecutable(process.platform === "win32" ? "cargo.exe" : "cargo", env);
  if (!cargo) throw new Error("Não encontrei cargo. Instale Rust com rustup antes de atualizar a versão.");

  const [packageOriginal, manifestOriginal] = await Promise.all([
    readFile(packagePath, "utf8"),
    readFile(manifestPath, "utf8"),
  ]);
  let lockOriginal;
  try {
    lockOriginal = await readFile(lockPath);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }

  const packageJson = JSON.parse(packageOriginal);
  packageJson.version = version;
  const lineEnding = packageOriginal.includes("\r\n") ? "\r\n" : "\n";
  const packageUpdated = `${JSON.stringify(packageJson, null, 2).replace(/\n/g, lineEnding)}${lineEnding}`;
  const manifestUpdated = manifestOriginal.replace(
    /(^\[package\][\s\S]*?^version\s*=\s*")[^"]+(".*$)/m,
    `$1${version}$2`,
  );
  if (manifestUpdated === manifestOriginal) throw new Error("Não encontrei a versão em [package] no Cargo.toml.");

  await Promise.all([
    writeFile(packagePath, packageUpdated, "utf8"),
    writeFile(manifestPath, manifestUpdated, "utf8"),
  ]);

  try {
    await run(cargo, ["check", "--manifest-path", manifestPath], env);
  } catch (error) {
    await Promise.all([
      writeFile(packagePath, packageOriginal, "utf8"),
      writeFile(manifestPath, manifestOriginal, "utf8"),
      lockOriginal ? writeFile(lockPath, lockOriginal) : rm(lockPath, { force: true }),
    ]);
    throw new Error(`cargo check falhou; package.json, Cargo.toml e Cargo.lock foram restaurados. ${error.message}`);
  }

  process.stdout.write(`✅ Versão atualizada para ${version} em package.json e Cargo.toml; cargo check sincronizou Cargo.lock.\n`);
}

main().catch((error) => {
  process.stderr.write(`❌ Não foi possível atualizar a versão: ${error.message}\n`);
  process.exitCode = 1;
});
