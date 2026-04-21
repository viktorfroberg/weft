# Images & screenshots

Drop marketing assets here. Referenced from the root README and `docs/*`.

## `logo-mark.png` — header

The app mark (kept in sync with `src/assets/logo-mark.png`). Referenced from the very top of the root README. When you update the in-app mark, re-run:

```bash
cp src/assets/logo-mark.png .github/assets/logo-mark.png
```

## Optional screenshots

Not required, not linked from the README by default — add a `<img>` tag manually when you're ready to ship one.

- `hero.png` — a clean shot of weft with Claude Code running across 2 repos. Tokyo Night (default) dark scheme, 2+ repos attached, terminal mid-output, Changes panel with a few files across repos. 1400×900 (Tauri default) or 1600×1000. Export as 2x retina PNG, compress with [`squoosh`](https://squoosh.app) or `pngcrush` to ≤200KB.
- `settings-appearance.png` — Settings → Appearance with TerminalPreview and scheme grid. For `docs/themes.md`.
- `ticket-linking.png` — Home compose card with ticket chips. For `docs/integrations.md`.
- `multi-repo-diff.png` — Changes panel with 3 per-repo sections expanded. For `docs/architecture.md`.

Keep filenames lowercase-kebab. Don't commit PSDs or Figma exports — just the flattened PNGs.
