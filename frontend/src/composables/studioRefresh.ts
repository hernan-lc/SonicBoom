/** Fast background cadence: provider/job-facing status only. */
export const STUDIO_FAST_MS = 5000;
/** Slow background cadence: low-frequency metadata (voice list, snapshots). */
export const STUDIO_SLOW_EVERY = 6;
/** Decides whether the slow metadata refresh is due on a fast tick. */
export function slowRefreshDue(tick: number): boolean {
  return tick % STUDIO_SLOW_EVERY === 0;
}
