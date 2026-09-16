/** Real milliseconds used by the authoritative server, converted to display time. */
export function displayTime(realRemaining: number): number {
  const ms = Math.max(0, realRemaining);
  if (ms <= 4000) return ms / 4;
  if (ms <= 6000) return 1000 + (ms - 4000) / 2;
  return 2000 + ms - 6000;
}
export function timerStage(displayMs: number): 'normal'|'slow'|'critical' {
  return displayMs <= 1000 ? 'critical' : displayMs <= 2000 ? 'slow' : 'normal';
}
