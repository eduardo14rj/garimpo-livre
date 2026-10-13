import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { delimiter, dirname, join } from "node:path";
import { homedir } from "node:os";

export function tauriEnvironment(source = process.env) {
  const env = { ...source };
  const pathKey = Object.keys(env).find((key) => key.toLowerCase() === "path") || "PATH";
  const cargoBin = join(env.USERPROFILE || homedir(), ".cargo", "bin");
  const nodeBin = dirname(process.execPath);
  const paths = (env[pathKey] || "").split(delimiter).filter(Boolean);

  for (const directory of [cargoBin, nodeBin]) {
    if (!paths.some((entry) => entry.toLowerCase() === directory.toLowerCase())) {
      paths.unshift(directory);
    }
  }

  env[pathKey] = paths.join(delimiter);
  return env;
}

export function findExecutable(executable, env = process.env) {
  const path = Object.entries(env).find(([key]) => key.toLowerCase() === "path")?.[1] || "";
  for (const directory of path.split(delimiter).filter(Boolean)) {
    const candidate = join(directory, executable);
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

export function prepareWindowsMsvcEnvironment(source = process.env) {
  const env = tauriEnvironment(source);
  if (process.platform !== "win32") return env;
  if (findExecutable("cl.exe", env) && findExecutable("link.exe", env)) return env;

  const vswhere = [
    findExecutable("vswhere.exe", env),
    "C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe",
    "C:\\Program Files\\Microsoft Visual Studio\\Installer\\vswhere.exe",
  ].find((candidate) => candidate && existsSync(candidate));

  if (!vswhere) {
    throw new Error("Não encontrei o vswhere do Visual Studio. Instale o Microsoft C++ Build Tools com o workload Desktop development with C++.");
  }

  const discovery = spawnSync(vswhere, [
    "-latest",
    "-products",
    "*",
    "-requires",
    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
    "-property",
    "installationPath",
  ], { env, encoding: "utf8", windowsHide: true });

  const installationPath = discovery.stdout?.trim().split(/\r?\n/).filter(Boolean)[0];
  if (discovery.status !== 0 || !installationPath) {
    throw new Error("Não encontrei as ferramentas C++ x64 do Visual Studio. Instale o workload Desktop development with C++.");
  }

  const developerCommand = join(installationPath, "Common7", "Tools", "VsDevCmd.bat");
  if (!existsSync(developerCommand)) {
    throw new Error(`Não encontrei o ambiente de desenvolvimento do Visual Studio em ${developerCommand}.`);
  }

  const command = `call "${developerCommand}" -no_logo -arch=x64 -host_arch=x64 && set`;
  const result = spawnSync(env.ComSpec || "cmd.exe", ["/d", "/s", "/c", command], {
    env,
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
    windowsVerbatimArguments: true,
    windowsHide: true,
  });

  if (result.error || result.status !== 0) {
    const detail = (result.stderr || result.stdout || result.error?.message || "").trim().slice(-1200);
    throw new Error(`Não consegui carregar as ferramentas C++ do Visual Studio.${detail ? `\n${detail}` : ""}`);
  }

  const prepared = { ...env };
  for (const line of result.stdout.split(/\r?\n/)) {
    const separator = line.indexOf("=");
    if (separator <= 0) continue;
    const key = line.slice(0, separator);
    const value = line.slice(separator + 1);
    if (key.toLowerCase() === "path") {
      const existingPathKey = Object.keys(prepared).find((name) => name.toLowerCase() === "path") || "PATH";
      prepared[existingPathKey] = value;
    } else {
      prepared[key] = value;
    }
  }

  if (!findExecutable("cl.exe", prepared) || !findExecutable("link.exe", prepared)) {
    throw new Error("O ambiente do Visual Studio foi carregado, mas cl.exe ou link.exe continua indisponível.");
  }

  return prepared;
}
