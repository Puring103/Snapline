import { describe, expect, it } from "vitest";
import { importPromptText, validateSyncConnection } from "./components/SyncSettings";

describe("sync connection validation", () => {
  it("requires all connection fields", () => {
    expect(validateSyncConnection({ serverUrl: "", email: "", password: "" })).toEqual({
      serverUrl: "Server URL is required",
      email: "Email is required",
      password: "Password is required",
    });
  });

  it("validates server URL and email shape", () => {
    expect(validateSyncConnection({ serverUrl: "snapline", email: "bad-email", password: "secret" })).toEqual({
      serverUrl: "Enter a valid http or https URL",
      email: "Enter a valid email address",
    });
  });

  it("accepts valid connection fields", () => {
    expect(validateSyncConnection({
      serverUrl: "https://sync.example.com",
      email: "me@example.com",
      password: "secret",
    })).toEqual({});
  });

  it("describes local draft import after login", () => {
    expect(importPromptText(1)).toBe("Detected 1 local note. Import into this account?");
    expect(importPromptText(3)).toBe("Detected 3 local notes. Import into this account?");
  });
});
