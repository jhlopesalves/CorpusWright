/** The only application-wide import boundary for raw Tauri APIs. */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { normaliseIpcError } from "./ipc-error.ts";

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(command, args);
  } catch (error) {
    // String IPC is retained consistently for this release. Classification is centralised here so
    // a future envelope migration has one rejection boundary and current visible wording remains.
    throw normaliseIpcError(error).message;
  }
}

export { listen, type UnlistenFn } from "@tauri-apps/api/event";
export { open, save } from "@tauri-apps/plugin-dialog";
