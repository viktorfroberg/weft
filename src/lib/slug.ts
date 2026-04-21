/**
 * Client-side slug derivation — mirrors `task::derive_slug` +
 * `task::derive_slug_from_tickets` on the Rust side so the
 * TaskComposeCard can show a live branch preview (`feature/<slug>` or
 * `weft/<slug>`) as the user types.
 *
 * Used for display only. Server re-derives and uniquifies at submit,
 * so the final branch may differ (e.g. collision suffix `-2`).
 *
 * Keep in lockstep with `src-tauri/src/task.rs` — if the Rust rules
 * change, bump this alongside.
 */

const MAX_LEN = 50;

/** Mirror of Rust's `slug::slugify`: lowercase ASCII, hyphen-separated,
 * punctuation collapsed. Unicode letters are preserved-but-transliterated
 * approximately by the Rust `slug` crate; in the browser we strip
 * non-ASCII — good enough for preview. */
function slugify(input: string): string {
  const ascii = input
    .normalize("NFKD")
    // Drop combining marks (so "é" → "e").
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase();
  const out = ascii
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return out;
}

function truncate(s: string): string {
  if (s.length <= MAX_LEN) return s;
  return s.slice(0, MAX_LEN).replace(/-+$/g, "");
}

/** Default, name-driven slug. Empty when input is nothing but symbols. */
export function deriveSlug(name: string): string {
  return truncate(slugify(name));
}

/** Ticket-driven slug with shared team-prefix dedupe.
 *   ["ABC-123", "ABC-124"] → "abc-123-124"
 *   ["ABC-12", "XYZ-9"]    → "abc-12-xyz-9"
 */
export function deriveSlugFromTickets(ids: string[]): string {
  if (ids.length === 0) return "";
  type Part = { prefix: string | null; rest: string };
  const parts: Part[] = ids.map((id) => {
    const lc = id.toLowerCase();
    const idx = lc.indexOf("-");
    if (idx > 0 && idx < lc.length - 1) {
      return { prefix: lc.slice(0, idx), rest: lc.slice(idx + 1) };
    }
    return { prefix: null, rest: lc };
  });

  const allSamePrefix =
    parts.every((p) => p.prefix !== null) &&
    parts.every((p) => p.prefix === parts[0].prefix);

  const raw = allSamePrefix
    ? `${parts[0].prefix}-${parts.map((p) => p.rest).join("-")}`
    : parts.map((p) => (p.prefix ? `${p.prefix}-${p.rest}` : p.rest)).join("-");

  return truncate(raw);
}

/**
 * Branch-name preview: `feature/<slug>` when tickets are linked,
 * `weft/<slug>` otherwise. Empty string when neither the prompt nor
 * the tickets produce a usable slug.
 */
export function deriveBranchPreview(
  prompt: string,
  ticketIds: string[],
): string {
  const slug = ticketIds.length > 0
    ? deriveSlugFromTickets(ticketIds)
    : deriveSlug(prompt);
  if (!slug) return "";
  const prefix = ticketIds.length > 0 ? "feature/" : "weft/";
  return `${prefix}${slug}`;
}
