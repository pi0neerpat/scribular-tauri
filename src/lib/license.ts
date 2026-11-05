// The trial / licence state for the UI (spec 04 R11/R12): fetched once, then
// kept current by `license://changed`, so nothing polls.
import { writable } from 'svelte/store';
import { getAccessState, onLicenseChanged } from './api';
import type { AccessState } from './types/AccessState';
import type { ActivationResult } from './types/ActivationResult';
import type { Rejection } from './types/Rejection';

export const PRICING_URL = 'https://www.scribular.com/#pricing';

/** Null until the first answer, and on the locked-store path where Rust has none. */
export const accessState = writable<AccessState | null>(null);

/** Starts following the state; returns the cleanup. */
export function followAccessState(): () => void {
  // An event is newer than any snapshot, so the snapshot never overwrites one.
  let heard = false;
  const unlisten = onLicenseChanged((s) => {
    heard = true;
    accessState.set(s);
  });
  unlisten
    .then(() => getAccessState())
    .then((s) => heard || accessState.set(s))
    .catch(() => {}); // locked store: no licensing state, nothing to show
  return () => {
    unlisten.then((fn) => fn());
  };
}

/** Mirrors `AccessState::allows` in Rust: warnings never deny (R8). */
export function allows(s: AccessState | null): boolean {
  return !s || !['trialEnded', 'licenseInvalid', 'licenseLapsed'].includes(s.kind);
}

const days = (n: number) => `${n} ${n === 1 ? 'day' : 'days'}`;

export function badgeText(s: AccessState): string {
  switch (s.kind) {
    case 'licensed':
    case 'licensedGrace':
      return 'Licensed';
    case 'trial':
    case 'trialWarning':
      return `Trial (${days(s.daysLeft)} remaining)`;
    case 'trialEnded':
      return 'Trial ended';
    case 'licenseInvalid':
      return 'Licence invalid';
    case 'licenseLapsed':
      return 'Licence not verified';
  }
}

const rejectionText: Record<Rejection, string> = {
  invalidKey: "That licence key wasn't recognised.",
  expired: 'That licence has expired.',
  deviceLimit:
    'That licence is already active on the maximum number of Macs. Contact support to free a slot.',
  badFormat: 'Licence keys look like word-word-word-123.',
};

/** Why new recordings are refused, or null when they aren't. */
export function deniedReason(s: AccessState | null): string | null {
  switch (s?.kind) {
    case 'trialEnded':
      return 'Your free trial has ended.';
    case 'licenseInvalid':
      return rejectionText[s.reason];
    case 'licenseLapsed':
      return "Your licence has expired, or Scribular hasn't been able to verify it for too long. Connect to the internet to re-verify it.";
    default:
      return null;
  }
}

/** A non-blocking notice, with whether it can be dismissed for the day. */
export function warning(s: AccessState | null): { text: string; dismissible: boolean } | null {
  if (s?.kind === 'trialWarning')
    return {
      text: `Your free trial ends in ${days(s.daysLeft)}.`,
      dismissible: true,
    };
  if (s?.kind === 'licensedGrace')
    return {
      text: 'Connect to the internet to re-verify your licence.',
      dismissible: false,
    };
  return null;
}

export function activationText(r: ActivationResult): string {
  switch (r.kind) {
    case 'valid':
      return 'Licence activated.';
    case 'offline':
      return "Couldn't reach scribular.com. Check your connection and try again; the first activation needs one.";
    case 'serverError':
      return 'scribular.com had a problem. Try again later.';
    default:
      return rejectionText[r.kind];
  }
}

const DISMISS_KEY = 'scribular.trialBannerDismissed';
const today = () => new Date().toDateString();

/** Local storage can throw (private mode, blocked); then the banner just shows. */
export function dismissedToday(): boolean {
  try {
    return localStorage.getItem(DISMISS_KEY) === today();
  } catch {
    return false;
  }
}

export function dismissForToday(): void {
  try {
    localStorage.setItem(DISMISS_KEY, today());
  } catch {
    // Not remembered; it hides until the next reload.
  }
}
