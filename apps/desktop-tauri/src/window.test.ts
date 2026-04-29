import { describe, expect, it } from "vitest";
import { readAppRoute } from "./window";

describe("window routing", () => {
  it("defaults to the note window route", () => {
    window.history.pushState({}, "", "/");

    expect(readAppRoute()).toEqual({ mode: "note", noteId: null });
  });
});
