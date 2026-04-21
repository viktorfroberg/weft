/**
 * Ranker for the ⌘K command palette. Pure data transform — no React,
 * no DOM — so it's unit-testable in isolation and swappable if we ever
 * want a fuzzier scorer.
 *
 * Scoring ladder (higher is better):
 *   100 — exact match
 *    80 — prefix match (haystack starts with query)
 *    60 — word-start match (after whitespace/separator)
 *    20 — substring anywhere
 *     0 — no match
 */

export type RankableItem =
  | { kind: "workspace"; name: string }
  | { kind: "task"; name: string; workspace: string }
  | { kind: "action"; label: string };

export function rank<T extends RankableItem>(items: T[], raw: string): T[] {
  const q = raw.trim().toLowerCase();
  if (!q) return items;
  return items
    .map((item) => ({ item, score: scoreItem(item, q) }))
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score)
    .map((x) => x.item);
}

function scoreItem(item: RankableItem, q: string): number {
  let best = 0;
  for (const h of searchableFields(item)) {
    const s = matchScore(h, q);
    if (s > best) best = s;
  }
  return best;
}

function searchableFields(item: RankableItem): string[] {
  if (item.kind === "workspace") return [item.name];
  if (item.kind === "task") return [item.name, item.workspace];
  return [item.label];
}

function matchScore(haystack: string, q: string): number {
  const h = haystack.toLowerCase();
  if (!q) return 0;
  if (h === q) return 100;
  if (h.startsWith(q)) return 80;
  const wordStart = new RegExp(`(^|[\\s\\-_/])${escapeRe(q)}`).test(h);
  if (wordStart) return 60;
  if (h.includes(q)) return 20;
  return 0;
}

function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
