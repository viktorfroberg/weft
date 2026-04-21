/**
 * Stable hash → pick an accent color. Used for entities like Workspaces
 * that don't have an explicit color — we derive one deterministically from
 * the name so the same workspace always renders in the same color.
 *
 * We define our own palette here rather than reusing shadcn Nova's
 * `--chart-N` tokens because Nova's defaults are pure grayscale
 * (oklch(X 0 0)) — useless for distinguishing workspaces visually.
 *
 * Picks are muted-vibrant oklch values that read well on both light and
 * dark backgrounds without screaming. Same order means same workspace
 * always gets the same color across sessions.
 */

export const CHART_COLORS: readonly string[] = [
  "oklch(0.75 0.18 330)", // pink / magenta
  "oklch(0.75 0.15 235)", // blue
  "oklch(0.78 0.15 150)", // green
  "oklch(0.78 0.16 60)",  // orange / amber
  "oklch(0.72 0.20 285)", // purple
  "oklch(0.77 0.13 190)", // teal / cyan
  "oklch(0.72 0.18 15)",  // red / coral
];

/** djb2 — cheap, good-enough distribution for our bucket count. */
function hashStr(s: string): number {
  if (!s) return 0;
  let h = 5381;
  for (let i = 0; i < s.length; i++) {
    h = (h * 33) ^ s.charCodeAt(i);
  }
  return Math.abs(h | 0);
}

export function hashedChartColor(key: string): string {
  return CHART_COLORS[hashStr(key) % CHART_COLORS.length];
}

/** Derive a tint (translucent bg) + border color from an arbitrary CSS
 * color string. Works for hex, oklch(), and CSS var() references. We
 * stack layers instead of computing rgba so CSS var refs stay dynamic.
 *
 * Appearance matters in light mode: 15% over a 0.99-L surface washes
 * out to near-white. Bumping to 28% keeps hue identity without going
 * pastel. Dark mode stays at 15% — the contrast is already there. */
export function tintedBackground(
  color: string,
  appearance: "dark" | "light" = "dark",
): string {
  const pct = appearance === "light" ? 28 : 15;
  return `color-mix(in oklch, ${color} ${pct}%, transparent)`;
}

export function tintedBorder(
  color: string,
  appearance: "dark" | "light" = "dark",
): string {
  const pct = appearance === "light" ? 55 : 40;
  return `color-mix(in oklch, ${color} ${pct}%, transparent)`;
}

/**
 * Return a readable text variant of an accent color. Our chart palette
 * sits at L=0.72–0.78 (mid-light) which reads fine on dark backgrounds
 * but fails WCAG on near-white. In light mode we pull it down toward
 * black so the letter keeps its hue + chroma identity while hitting a
 * usable contrast ratio — but not so far that chroma collapses to grey.
 *
 * Uses `color-mix` so it works against oklch(), hex, and var() inputs.
 */
export function readableText(
  color: string,
  appearance: "dark" | "light",
): string {
  if (appearance === "dark") return color;
  // 40% color + 60% dark-grey lands near L=0.36 with ~0.08 chroma
  // retained. Enough contrast on a 0.99-L surface, hue still reads.
  return `color-mix(in oklch, ${color} 40%, oklch(0.22 0 0))`;
}
