import { invoke, type InvokeArgs } from "@tauri-apps/api/core";

// Utility functions
export async function callBackend<T>(
  cmd: string,
  args?: InvokeArgs,
): Promise<T> {
  try {
    return await invoke(cmd, args);
  } catch (err) {
    alert(err);
    throw err;
  }
}

export async function runWithWaitCursor<T>(func: () => Promise<T>) {
  document.body.style.cursor = "wait";
  try {
    return await func();
  } finally {
    document.body.style.cursor = "auto";
  }
}

export async function callBackendWithWaitCursor<T>(
  cmd: string,
  args?: InvokeArgs,
): Promise<T> {
  return runWithWaitCursor(() => callBackend(cmd, args));
}
