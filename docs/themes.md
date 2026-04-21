# Themes

weft's theming is one Base24 palette → xterm terminal colors, Tailwind chrome CSS vars, and Monaco diff theme. The whole app moves as one palette on scheme switch.

Settings → **Appearance** is the control surface. Every pref has a UI slider/segmented/toggle; nothing is hidden in config files.

## What you can change

| Control | Range | Default |
|---|---|---|
| **Mode** | Light / Dark / System | System |
| **Dark scheme** | Tokyo Night, One Dark, + added | Tokyo Night |
| **Light scheme** | Catppuccin Latte, GitHub Light, + added | Catppuccin Latte |
| **Font family** | JetBrains Mono, Fira Code, Geist Mono, Source Code Pro, System | JetBrains Mono |
| **Font weight** | 400 / 500 / 600 (variable fonts only) | 400 |
| **Font size** | 10–20px, 1px step | 13 |
| **Line height** | 1.0–1.5, 0.05 step | 1.15 |
| **Ligatures** | on / off (font-gated) | on |
| **Padding horizontal** | 0–24px | 6 |
| **Padding vertical** | 0–24px | 4 |
| **Bold uses bright colors** | on / off | off |
| **Cursor style** | Block / Bar / Underline | Block |
| **Cursor blink** | on / off | on |
| **Bell** | Off / Visual / Audible / Both | Off |

Live preview at the top of the Appearance tab — no PTY, pre-rendered ANSI fixture so you can see `ls --color`, `git status`, a ligature-heavy code line, and all 16 ANSI bg swatches react in real time.

## Bundled schemes

All 4 bundled schemes are published with their original Base24 values:

| Scheme | Appearance | Notes |
|---|---|---|
| **Tokyo Night** | Dark | Default dark. Warm blue-black bg (`#1a1b26`), soft lavender fg. |
| **One Dark** | Dark | Atom's classic. Slate bg, warm orange accents. |
| **Catppuccin Latte** | Light | Default light. Modern, playful, pastel. |
| **GitHub Light** | Light | Clean, familiar, high contrast. |

## Presets

15 curated schemes ship under Settings → Appearance → **Add scheme…** → **Presets** tab. One click adds to your dark or light picker.

- Dracula, Nord, Gruvbox Dark, Gruvbox Light, Solarized Dark, Solarized Light, Monokai, Material, Ayu Dark, Ayu Light, Rosé Pine, Rosé Pine Dawn, Catppuccin Mocha, Catppuccin Frappé, Night Owl.

Sourced from [tinted-theming's base24 gallery](https://github.com/tinted-theming/base24-gallery) — ported into Base24 records inline in `src/lib/themes/presets.ts`.

## Importing a scheme

Anything in two formats works, pasted into the **Paste** tab of the Add scheme dialog:

### base16 / base24 YAML

Format: [tinted-theming spec](https://github.com/tinted-theming/home#scheme-repositories). Example:

```yaml
scheme: "Nord"
author: "Arctic Ice Studio"
variant: "dark"
base00: "2e3440"
base01: "3b4252"
base02: "434c5e"
base03: "4c566a"
base04: "d8dee9"
base05: "e5e9f0"
base06: "eceff4"
base07: "8fbcbb"
base08: "bf616a"
base09: "d08770"
base0A: "ebcb8b"
base0B: "a3be8c"
base0C: "88c0d0"
base0D: "81a1c1"
base0E: "b48ead"
base0F: "5e81ac"
```

Lowercase `base0a` is also accepted (tinted-theming's canonical form).

Pull hundreds of schemes from [tinted-theming/base16-schemes](https://github.com/tinted-theming/base16-schemes) — any of them paste-imports cleanly. Base16 schemes don't publish base10-17 but the accent colors (base08-0F) are all there, which is what weft's derivation reads.

### iTerm2 `.itermcolors`

Format: XML plist. [iTerm2-Color-Schemes](https://github.com/mbadolato/iTerm2-Color-Schemes) has ~400 ports — most paste-import cleanly. weft synthesizes the missing Base24 slots (`base03`, `base04`, `base06`, `base09`, `base0F`) from the iTerm ANSI values.

Open any `.itermcolors` file in a text editor, paste the full contents, click **Add scheme**.

## How it works under the hood

### Data model

One `ColorScheme` = a `Base24` palette + derived `XtermTheme` + `ChromeTokens` + `MonacoTheme`, all computed by a single pure fn.

```ts
type ColorScheme = {
  id: string;
  name: string;
  appearance: "dark" | "light";
  base: Base24;       // base00..base0F
  terminal: XtermTheme;   // bg, fg, cursor, 16 ANSI + selection/cursorAccent
  chrome: ChromeTokens;   // bg, fg, surface, sidebar, border, muted, accent, destructive
  monaco: MonacoTheme;    // IStandaloneThemeData shape
};
```

### Chrome derivation

```
chrome.background  ← base00
chrome.surface     ← base01           (cards, popovers)
chrome.sidebar     ← mix(base00, base01, 50/50)
chrome.border      ← base02
chrome.muted       ← base01
chrome.foreground  ← base05
chrome.muted-fg    ← base04 (dark) or base03 (light)   // keeps a11y band
chrome.accent      ← base0D (blue / interactive)
chrome.destructive ← base08 (red)
```

Deterministic, so imported schemes look native in the sidebar and toolbar without any hand-tuning. The same function is applied to the 4 bundled schemes and to imports — no two code paths.

### Runtime application

`src/lib/themes/apply.ts` exports a single `applyTheme(theme, scheme)` fn. It:

1. Toggles `<html class="dark">`.
2. Writes `<html data-scheme="…">` for CSS selectors that want to key off the scheme id.
3. Writes every shadcn/Nova CSS var (`--background`, `--foreground`, `--primary`, `--ring`, `--sidebar`, …) onto `document.documentElement` via `setProperty`.
4. Calls `monaco.editor.defineTheme('weft-<id>', scheme.monaco)` + `setTheme(…)` if monaco is loaded (lazy).

All in one synchronous pass. Splitting this across two effects caused mid-frame desync on OS theme flips (`.dark` removed while chrome still dark).

### Terminal pipeline

`Terminal.tsx` reads the active scheme + every font/cursor/bell/padding pref from Zustand. Live-updatable via `xterm.options.*` + `fit()`; ligatures require a remount (the `@xterm/addon-ligatures` addon caches OT font metadata on load). Ligature toggle keys the mount effect.

Bell is:
- **Visual:** CSS ring on the terminal wrapper flashes `primary` color for 150ms on xterm's `onBell`.
- **Audible:** Web Audio `OscillatorNode`, 880Hz sine, 80ms, volume 0.1. No `.wav` file shipped.
- **Both:** both paths fire.

Test bell button in Settings uses a tiny pub/sub channel (`onTestBell` / `fireTestBell`) to reach the mounted `TerminalPreview` without prop plumbing.

### Baked defaults (not user-facing)

Terminal.tsx hardcodes these — they're safe universal values, not personality choices:

```ts
letterSpacing: 0,
minimumContrastRatio: 4.5,   // a11y floor — imports with pathological base05 lift against base00
allowTransparency: false,    // v1.0.5 doesn't do opacity (deferred to a future NSVisualEffectView phase)
scrollback: 5000,
```

### Tailwind v4 `@theme inline` — load-bearing

The `@theme inline` block in `src/index.css` is why runtime CSS-var overrides work. Without `inline`, Tailwind bakes the resolved value at build time and every scheme switch becomes a silent no-op.

A comment in `index.css` guards this. Don't remove `inline` without migrating the theming layer first.

## Files

| Path | Purpose |
|---|---|
| `src/lib/themes/schemes.ts` | `ColorScheme` type + 4 bundled Base24 records |
| `src/lib/themes/derive.ts` | Base24 → XtermTheme/ChromeTokens/MonacoTheme |
| `src/lib/themes/apply.ts` | Unified `applyTheme(theme, scheme)` |
| `src/lib/themes/fonts.ts` | Font registry (5 entries) |
| `src/lib/themes/bell.ts` | Web Audio bell + test pub/sub |
| `src/lib/themes/presets.ts` | 15 curated presets |
| `src/lib/themes/import/base24.ts` | base16/base24 YAML parser |
| `src/lib/themes/import/iterm.ts` | iTerm `.itermcolors` plist parser |
| `src/components/Terminal.tsx` | xterm.js wrapper reading prefs |
| `src/components/TerminalPreview.tsx` | Settings live preview (no PTY) |
| `src/components/SettingsView/AppearanceTab.tsx` | Full UI |
| `src/components/SettingsView/AddSchemeDialog.tsx` | Paste-in + presets grid |

## Roadmap

- Background opacity + blur via `NSVisualEffectView` (needs its own Tauri plugin work).
- Authoring UI for editing an imported scheme's individual slots.
- UI-font (Geist) customization.
- Sync Monaco `tokenColors` to syntax highlighting for more languages (currently the core 8: comment, string, number, keyword, type, function, variable, constant, operator).
