import type { LockReason } from "../types/LockReason";

/** Must match `START_FRESH_CONFIRMATION` in commands.rs. */
export const START_FRESH_PHRASE = "start fresh";

export interface LockedCopy {
  title: string;
  body: string;
  /** Only when the key is missing or wrong (spec 01 R7). */
  canStartFresh: boolean;
}

export function lockedCopy(reason: LockReason): LockedCopy {
  switch (reason) {
    case "keychainDenied":
      return {
        title: "Scribular couldn't read its key from your Keychain",
        body: "Your sessions are still on this Mac, encrypted. Unlock your Keychain, or allow access when macOS asks, then reopen Scribular.",
        canStartFresh: false,
      };
    case "keyMissing":
      return {
        title: "Scribular's key is missing from your Keychain",
        body: "Without it your sessions can't be opened. If you can restore the Keychain item, do that and reopen Scribular. Otherwise you can start fresh with an empty history.",
        canStartFresh: true,
      };
    case "wrongKey":
      return {
        title: "The key in your Keychain doesn't open your sessions",
        body: "If you can restore the original Keychain item, do that and reopen Scribular. Otherwise you can start fresh with an empty history.",
        canStartFresh: true,
      };
    case "migrationFailed":
      return {
        title: "Scribular couldn't encrypt your earlier sessions",
        body: "The original file has not been changed. Quit and reopen Scribular to try again.",
        canStartFresh: false,
      };
    case "notSqlcipherBuild":
      return {
        title: "This build of Scribular can't encrypt your sessions",
        body: "It won't open them, so nothing is stored unencrypted. Install the latest version of Scribular.",
        canStartFresh: false,
      };
    case "keychainWriteFailed":
      return {
        title: "Scribular couldn't save its key to your Keychain",
        body: "Nothing was created. Unlock your Keychain, then reopen Scribular.",
        canStartFresh: false,
      };
  }
}

export function confirmsStartFresh(typed: string): boolean {
  return typed.trim().toLowerCase() === START_FRESH_PHRASE;
}
