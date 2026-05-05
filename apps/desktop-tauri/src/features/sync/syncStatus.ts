import type { SyncAccountState } from "../../types";

export function syncStatusLabel(account: SyncAccountState | null): "offline" | "online" {
  return account?.is_logged_in ? "online" : "offline";
}
