import { invoke as tauriInvoke, isTauri } from "@tauri-apps/api/core";
import { listen as tauriListen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";

const desktopRequiredMessage =
  'Esta interface foi aberta no navegador. Para acessar os dados locais, inicie o app desktop com "npm run tauri dev".';
const desktopIpcUnavailableMessage =
  'O app foi aberto, mas a comunicação com o núcleo Tauri não está disponível. Reinicie com "npm run tauri dev".';

function hasDesktopIpc() {
  const internals = Reflect.get(globalThis, "__TAURI_INTERNALS__");
  return (
    isTauri() &&
    internals !== null &&
    typeof internals === "object" &&
    typeof Reflect.get(internals, "invoke") === "function"
  );
}

export function isDesktopApp() {
  return hasDesktopIpc();
}

export function desktopRequiredError() {
  return new Error(isTauri() ? desktopIpcUnavailableMessage : desktopRequiredMessage);
}

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!hasDesktopIpc()) throw desktopRequiredError();
  return tauriInvoke<T>(command, args);
}

export async function listen<T>(
  event: string,
  handler: (event: Event<T>) => void,
): Promise<UnlistenFn> {
  if (!hasDesktopIpc()) return () => undefined;
  return tauriListen<T>(event, handler);
}

export async function openUrl(url: string) {
  if (!isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  if (!hasDesktopIpc()) throw desktopRequiredError();
  await tauriOpenUrl(url);
}
