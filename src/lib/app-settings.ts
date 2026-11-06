import { writable } from 'svelte/store';
import type { AppSettings } from './types/AppSettings';

export const appSettings = writable<AppSettings>({ notifySummaryComplete: false, showBackgroundAnimation: true });
