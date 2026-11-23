import { getContext } from 'svelte';
import type { CoachAvailability } from './types/CoachAvailability';
import type { PromptList } from './types/PromptList';

export interface CoachTarget {
  styleId?: string;
  samples?: boolean;
}

export interface CoachContext {
  availability: () => CoachAvailability | null;
  open: (target: CoachTarget, opener: HTMLElement, onOpen?: () => void) => void;
  firstNoteTip: (id: string) => boolean;
  tipSaving: () => boolean;
  tipSaveFailed: () => boolean;
  dismissTip: () => void;
  styles: () => PromptList | null;
  updateStyles: (styles: PromptList) => void;
}

export const COACH_CONTEXT = Symbol('note style coach');
export const getCoachContext = () => getContext<CoachContext | undefined>(COACH_CONTEXT);
