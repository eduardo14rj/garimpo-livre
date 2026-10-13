import { spawn } from "node:child_process";
import { configForDevPort, findAvailablePort, portAvailable } from "./tauri-dev-port.mjs";
import { delimiter, dirname, join } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";

const cargoBin = join(process.env.USERPROFILE || homedir(), ".cargo", "bin");
const env = { ...process.env };
const args = process.argv.slice(2);
const pathKey = Object.keys(env).find((key) => key.toLowerCase() === "path") || "PATH";
const paths = (env[pathKey] || "").split(delimiter).filter(Boolean);
for (const directory of [cargoBin, dirname(process.execPath)]) {
  if (!paths.some((entry) => entry.toLowerCase() === directory.toLowerCase())) {
    paths.unshift(directory);
  }
}
env[pathKey] = paths.join(delimiter);

const hasExplicitConfig = args.some((argument) =>
  argument === "--config" || argument === "-c" || argument.startsWith("--config="),
);
if (args[0] === "dev" && !hasExplicitConfig) {
  const requestedPort = Number(env.GARIMPO_TAURI_DEV_PORT || 1420);
  if (!Number.isInteger(requestedPort) || requestedPort < 1 || requestedPort > 65534) {
    process.stderr.write("GARIMPO_TAURI_DEV_PORT precisa ser uma porta válida entre 1 e 65534.\n");
    process.exit(2);
  }
  const baseFree = await portAvailable(requestedPort);
  const hmrFree = !env.TAURI_DEV_HOST || await portAvailable(requestedPort + 1);
  if (!baseFree || !hmrFree) {
    const port = await findAvailablePort(
      requestedPort + 1,
      env.TAURI_DEV_HOST ? 65534 : 65535,
      env.TAURI_DEV_HOST ? 2 : 1,
    );
    if (port === null) {
      process.stderr.write("Não encontrei uma porta livre para o servidor de desenvolvimento.\n");
      process.exit(2);
    }
    env.GARIMPO_TAURI_DEV_PORT = String(port);
    args.push("--config", JSON.stringify(configForDevPort(port, true)));
    process.stdout.write(`Porta ${requestedPort} ocupada; usando ${port} e um perfil isolado de desenvolvimento.\n`);
  } else if (env.GARIMPO_TAURI_DEV_PORT) {
    args.push("--config", JSON.stringify(configForDevPort(requestedPort)));
  }
}

const tauriCli = fileURLToPath(new URL("../node_modules/@tauri-apps/cli/tauri.js", import.meta.url));
const child = spawn(process.execPath, [tauriCli, ...args], {
  stdio: "inherit",
  env,
});

child.on("error", (error) => {
  process.stderr.write(`Falha ao iniciar o Tauri CLI: ${error.message}\n`);
  process.exitCode = 1;
});
child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
