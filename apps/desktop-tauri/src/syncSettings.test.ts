import { describe, expect, it } from "vitest";
import { validateSyncConnection } from "./SyncSettings";

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
});
