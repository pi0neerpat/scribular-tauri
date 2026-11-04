import { describe, expect, it } from "vitest";
import { confirmsStartFresh, lockedCopy } from "./locked";
import type { LockReason } from "../types/LockReason";

const ALL: LockReason[] = [
  "keychainDenied",
  "keyMissing",
  "wrongKey",
  "migrationFailed",
  "notSqlcipherBuild",
  "keychainWriteFailed",
];

describe("locked screen", () => {
  it("offers Start fresh only when the key is missing or wrong", () => {
    const fresh = ALL.filter((r) => lockedCopy(r).canStartFresh);
    expect(fresh).toEqual(["keyMissing", "wrongKey"]);
    for (const r of ALL) expect(lockedCopy(r).title).not.toBe("");
  });

  it("needs the exact phrase", () => {
    expect(confirmsStartFresh(" Start Fresh ")).toBe(true);
    expect(confirmsStartFresh("start")).toBe(false);
    expect(confirmsStartFresh("")).toBe(false);
  });
});
