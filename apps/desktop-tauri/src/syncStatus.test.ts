import { describe, expect, it } from "vitest";
import { syncStatusLabel } from "./features/sync/syncStatus";
import type { SyncAccountState } from "./types";

describe("sync status label", () => {
  it("shows offline when sync is not connected", () => {
    expect(syncStatusLabel(null)).toBe("offline");
    expect(syncStatusLabel({ is_logged_in: false } as SyncAccountState)).toBe("offline");
  });

  it("shows online when sync is connected", () => {
    expect(syncStatusLabel({ is_logged_in: true } as SyncAccountState)).toBe("online");
  });
});
