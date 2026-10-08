import type { Update } from "@tauri-apps/plugin-updater";

// Release builds poll GitHub Releases for a newer signed build. The browser
// demo and `tauri dev` skip it: there is nothing to install over.
const canUpdate = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window && !import.meta.env.DEV;

export async function checkForUpdate(): Promise<Update | null> {
  if (!canUpdate) return null;
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    return await check();
  } catch {
    // Offline or no release published yet; try again next launch.
    return null;
  }
}

export async function installUpdate(update: Update): Promise<void> {
  await update.downloadAndInstall();
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}
