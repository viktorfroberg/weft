/**
 * `2m`, `3h`, `5d`, `8w` — terse relative-time formatter for dense
 * row UIs (sidebar task list, recent-tasks switcher). Optimized for
 * scanability over precision; shows just-now as `now`.
 *
 * Input is unix-millis. `now` defaults to `Date.now()`. Future
 * timestamps render as `now` rather than negative durations — clock
 * skew shouldn't surface as a confusing `-3s`.
 */
export function formatRelativeShort(unixMs: number, now: number = Date.now()): string {
  const elapsed = Math.max(0, now - unixMs);
  const seconds = Math.floor(elapsed / 1000);
  if (seconds < 45) return "now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  const weeks = Math.floor(days / 7);
  if (weeks < 5) return `${weeks}w`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months}mo`;
  const years = Math.floor(days / 365);
  return `${years}y`;
}

/**
 * Longer form for tooltips: "Apr 21, 06:42" for under a year, plus
 * the year for older entries. Uses Intl so the user's locale picks
 * the conventional ordering / month name.
 */
export function formatAbsolute(unixMs: number, now: number = Date.now()): string {
  const d = new Date(unixMs);
  const sameYear = new Date(now).getFullYear() === d.getFullYear();
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    year: sameYear ? undefined : "numeric",
  });
}
