import { invoke } from "@tauri-apps/api/core";

const appStartedAt = performance.now();
const consoleStartupLoggingEnabled = localStorage.getItem("snapline.startupLog") === "1";

export function startupLog(event: string, details: Record<string, string | number | boolean | null> = {}) {
  const elapsedMs = Math.round(performance.now() - appStartedAt);
  const detailText = Object.entries(details)
    .map(([key, value]) => `${key}=${String(value)}`)
    .join(" ");
  const message = `snapline.startup source=frontend event=${event} elapsed_ms=${elapsedMs}${detailText ? ` ${detailText}` : ""}`;
  if (consoleStartupLoggingEnabled) {
    console.info(message);
  }
  void invoke("log_startup", { message }).catch(() => undefined);
}
