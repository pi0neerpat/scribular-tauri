export function shouldAnimate(enabled: boolean, hidden: boolean, reducedMotion: boolean): boolean {
  return enabled && !hidden && !reducedMotion;
}
