//! Custom-font management.
//!
//! Why this exists: macOS WKWebView (Tauri's webview on this platform)
//! restricts CSS font resolution to **Apple-shipped fonts only**. Fonts
//! the user installs via Font Book or `~/Library/Fonts/` are NOT visible
//! to plain `font-family: 'X'` declarations — a privacy/fingerprinting
//! restriction baked into WebKit. The only reliable way to use a third-
//! party font in the terminal is to register it via `@font-face` with a
//! URL the webview can fetch. So we copy the font file into weft's data
//! dir, expose it via Tauri's asset protocol, and inject a `@font-face`
//! rule pointing at the asset URL.
//!
//! Storage layout (under `~/Library/Application Support/weft/fonts/`):
//!   - `fonts.json`    — manifest array of `CustomFont` rows
//!   - `<id>.<ext>`    — copied font file, `<id>` = UUID v7,
//!                       `<ext>` = original lower-cased extension
//!
//! Concurrency: every manifest mutation must hold the process-wide
//! `font_lock` mutex from `AppState`. Two concurrent installs would
//! otherwise read-modify-write race and lose a row.

use crate::db::data_dir;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

pub const MAX_FONT_BYTES: u64 = 20 * 1024 * 1024;

/// Lower-cased extensions we accept. `ttc` is included because macOS
/// system fonts (and many user-installed bundles) ship as collections.
const ALLOWED_EXTS: &[&str] = &["ttf", "otf", "ttc", "woff", "woff2"];

/// Magic bytes per format. Checked against the first 16 bytes of the
/// uploaded file so a `.png` renamed `.ttf` is rejected before we copy
/// it into the fonts dir.
struct Magic {
    bytes: &'static [u8],
    /// Acceptable extensions for this magic — handles e.g. `true` magic
    /// being a TTF variant.
    ext_kind: &'static str,
}

const MAGICS: &[Magic] = &[
    Magic {
        bytes: &[0x00, 0x01, 0x00, 0x00],
        ext_kind: "ttf",
    },
    Magic {
        bytes: b"OTTO",
        ext_kind: "otf",
    },
    Magic {
        bytes: b"ttcf",
        ext_kind: "ttc",
    },
    Magic {
        bytes: b"true",
        ext_kind: "ttf",
    },
    Magic {
        bytes: b"typ1",
        ext_kind: "otf",
    },
    Magic {
        bytes: b"wOFF",
        ext_kind: "woff",
    },
    Magic {
        bytes: b"wOF2",
        ext_kind: "woff2",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomFont {
    pub id: String,
    pub display_name: String,
    pub file_basename: String,
    pub ligatures: bool,
    pub variable: bool,
    pub byte_size: u64,
    pub installed_at: i64,
    /// Optional italic-variant pairing. When present, `injectFontFaces`
    /// emits TWO `@font-face` blocks for the same `weft-custom-<id>`
    /// family — one with `font-style: normal` (this row's regular file)
    /// and one with `font-style: italic` pointing at
    /// `<id>.italic.<ext>`. Lets xterm's italic ANSI escapes
    /// (`\x1b[3m`) resolve to the proper italic face instead of
    /// rendering with the regular file. `#[serde(default)]` so manifests
    /// written before this field existed deserialize as `None`.
    #[serde(default)]
    pub italic_file_basename: Option<String>,
}

/// Full path to a font file on disk for a given row id.
pub fn font_file_path(id: &str, file_basename: &str) -> Result<PathBuf> {
    let ext = std::path::Path::new(file_basename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("ttf")
        .to_ascii_lowercase();
    Ok(fonts_dir()?.join(format!("{id}.{ext}")))
}

/// Path to the paired italic file for a row. Stored as
/// `<id>.italic.<ext>` so it sorts next to the regular file under
/// `~/Library/Application Support/weft/fonts/`.
pub fn font_italic_file_path(id: &str, italic_basename: &str) -> Result<PathBuf> {
    let ext = std::path::Path::new(italic_basename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("ttf")
        .to_ascii_lowercase();
    Ok(fonts_dir()?.join(format!("{id}.italic.{ext}")))
}

/// Heuristic: is this filename a variable font?
///
/// GoogleFonts / standard convention encodes axes in brackets:
///   `Foo[wght].ttf`, `Foo[wght,ital].ttf`, `Foo-Italic[wght].ttf`,
///   `Foo[ital,wght,wdth].ttf`, ...
///
/// We also accept the older `*-VariableFont_*.ttf` pattern. Anything
/// else falls through to `false` and the user can flip the toggle
/// themselves. False positives here only enable the weight slider
/// against a static font — no rendering damage, just a slider that
/// quietly does nothing.
pub fn is_variable_filename(basename: &str) -> bool {
    let lower = basename.to_ascii_lowercase();
    if lower.contains("variablefont") {
        return true;
    }
    // Look for `[...wght...]` (case-insensitive). Bracket required so
    // we don't false-positive on a literal "wght" in a font name.
    if let (Some(open), Some(close)) = (lower.find('['), lower.find(']')) {
        if open < close {
            let inside = &lower[open + 1..close];
            if inside.split(',').any(|axis| axis.trim() == "wght") {
                return true;
            }
        }
    }
    false
}

pub fn fonts_dir() -> Result<PathBuf> {
    let dir = data_dir()?.join("fonts");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    Ok(dir)
}

fn manifest_path() -> Result<PathBuf> {
    Ok(fonts_dir()?.join("fonts.json"))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    fonts: Vec<CustomFont>,
}

fn read_manifest() -> Result<Manifest> {
    let path = manifest_path()?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)
            .with_context(|| format!("parse {}", path.display()))?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::default()),
        Err(e) => Err(anyhow!("read {}: {}", path.display(), e)),
    }
}

fn write_manifest(m: &Manifest) -> Result<()> {
    let path = manifest_path()?;
    let bytes = serde_json::to_vec_pretty(m)?;
    // Temp-file rename for atomicity — a crash between write + rename
    // leaves the prior manifest intact rather than truncated.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("rename {}", tmp.display()))?;
    Ok(())
}

pub fn list(_lock: &Mutex<()>) -> Result<Vec<CustomFont>> {
    let _g = _lock.lock().unwrap_or_else(|e| e.into_inner());
    Ok(read_manifest()?.fonts)
}

pub fn install(lock: &Mutex<()>, src_path: &std::path::Path) -> Result<CustomFont> {
    // ---- Pre-flight (no lock needed) ---------------------------------
    let basename = src_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("source path has no filename"))?
        .to_string();

    let ext = src_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| anyhow!("source path has no extension"))?;
    if !ALLOWED_EXTS.contains(&ext.as_str()) {
        bail!(
            "unsupported font format '.{ext}'. Pick a .ttf, .otf, .ttc, .woff, or .woff2 file."
        );
    }

    let meta = std::fs::metadata(src_path)
        .with_context(|| format!("stat {}", src_path.display()))?;
    let byte_size = meta.len();
    if byte_size > MAX_FONT_BYTES {
        bail!(
            "font file is {} MB; weft caps custom fonts at 20 MB to keep the data dir lean.",
            byte_size / 1024 / 1024
        );
    }

    let bytes =
        std::fs::read(src_path).with_context(|| format!("read {}", src_path.display()))?;
    if !magic_matches(&bytes, &ext) {
        bail!(
            "this file isn't a valid font (extension '.{ext}' but the bytes don't look like one)."
        );
    }

    let id = uuid::Uuid::now_v7().to_string();
    let display_name = strip_ext(&basename).to_string();
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    // Auto-detect variable axes from the filename — saves the user a
    // guess. Only flips the weight slider on; cell-jitter risk for
    // single-weight files is unaffected because we're correctly
    // identifying actual variable fonts here.
    let variable = is_variable_filename(&basename);

    let row = CustomFont {
        id: id.clone(),
        display_name,
        file_basename: basename.clone(),
        ligatures: false,
        variable,
        byte_size,
        installed_at: now,
        italic_file_basename: None,
    };

    // ---- Copy bytes BEFORE manifest -- so a failed manifest write
    //      can rollback without leaving a phantom row pointing at
    //      missing bytes. ----------------------------------------------
    let dest = font_file_path(&id, &basename)?;
    std::fs::write(&dest, &bytes).with_context(|| format!("write {}", dest.display()))?;

    // ---- Manifest update under the process-wide lock. ---------------
    let result = (|| -> Result<()> {
        let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut m = read_manifest()?;
        m.fonts.push(row.clone());
        write_manifest(&m)?;
        Ok(())
    })();

    if let Err(e) = result {
        // Rollback: unlink the file we just copied so the next
        // reconciler pass doesn't see an orphan.
        let _ = std::fs::remove_file(&dest);
        return Err(e);
    }

    Ok(row)
}

pub fn remove(lock: &Mutex<()>, id: &str) -> Result<()> {
    let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut m = read_manifest()?;
    let idx = m.fonts.iter().position(|f| f.id == id);
    let Some(idx) = idx else {
        // Idempotent — already gone.
        return Ok(());
    };
    let row = m.fonts.remove(idx);
    write_manifest(&m)?;
    if let Ok(path) = font_file_path(&row.id, &row.file_basename) {
        let _ = std::fs::remove_file(&path);
    }
    if let Some(italic) = row.italic_file_basename.as_ref() {
        if let Ok(path) = font_italic_file_path(&row.id, italic) {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}

pub fn rename(lock: &Mutex<()>, id: &str, new_name: &str) -> Result<CustomFont> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        bail!("display name cannot be empty");
    }
    if trimmed.len() > 100 {
        bail!("display name must be ≤100 characters");
    }
    let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut m = read_manifest()?;
    let row = m
        .fonts
        .iter_mut()
        .find(|f| f.id == id)
        .ok_or_else(|| anyhow!("font {id} not found"))?;
    row.display_name = trimmed.to_string();
    let cloned = row.clone();
    write_manifest(&m)?;
    Ok(cloned)
}

pub fn set_ligatures(lock: &Mutex<()>, id: &str, on: bool) -> Result<CustomFont> {
    let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut m = read_manifest()?;
    let row = m
        .fonts
        .iter_mut()
        .find(|f| f.id == id)
        .ok_or_else(|| anyhow!("font {id} not found"))?;
    row.ligatures = on;
    let cloned = row.clone();
    write_manifest(&m)?;
    Ok(cloned)
}

pub fn set_variable(lock: &Mutex<()>, id: &str, on: bool) -> Result<CustomFont> {
    let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut m = read_manifest()?;
    let row = m
        .fonts
        .iter_mut()
        .find(|f| f.id == id)
        .ok_or_else(|| anyhow!("font {id} not found"))?;
    row.variable = on;
    let cloned = row.clone();
    write_manifest(&m)?;
    Ok(cloned)
}

/// Pair an italic-variant file with an existing custom-font row.
/// Reuses `install`'s validation (extension + magic + size cap) plus
/// the same atomic-rollback semantics: copy bytes first, manifest
/// update second, unlink on failure.
pub fn pair_italic(
    lock: &Mutex<()>,
    id: &str,
    src_path: &std::path::Path,
) -> Result<CustomFont> {
    let basename = src_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("source path has no filename"))?
        .to_string();

    let ext = src_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| anyhow!("source path has no extension"))?;
    if !ALLOWED_EXTS.contains(&ext.as_str()) {
        bail!(
            "unsupported font format '.{ext}'. Pick a .ttf, .otf, .ttc, .woff, or .woff2 file."
        );
    }

    let meta = std::fs::metadata(src_path)
        .with_context(|| format!("stat {}", src_path.display()))?;
    if meta.len() > MAX_FONT_BYTES {
        bail!(
            "italic file is {} MB; weft caps custom fonts at 20 MB.",
            meta.len() / 1024 / 1024
        );
    }

    let bytes =
        std::fs::read(src_path).with_context(|| format!("read {}", src_path.display()))?;
    if !magic_matches(&bytes, &ext) {
        bail!("this file isn't a valid font.");
    }

    let dest = font_italic_file_path(id, &basename)?;
    std::fs::write(&dest, &bytes).with_context(|| format!("write {}", dest.display()))?;

    let result = (|| -> Result<CustomFont> {
        let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut m = read_manifest()?;
        let row = m
            .fonts
            .iter_mut()
            .find(|f| f.id == id)
            .ok_or_else(|| anyhow!("font {id} not found"))?;
        // If a previous italic was paired, drop its file so we don't
        // leave an orphan around.
        if let Some(prev) = row.italic_file_basename.clone() {
            if let Ok(prev_path) = font_italic_file_path(id, &prev) {
                if prev_path != dest {
                    let _ = std::fs::remove_file(&prev_path);
                }
            }
        }
        row.italic_file_basename = Some(basename.clone());
        let cloned = row.clone();
        write_manifest(&m)?;
        Ok(cloned)
    })();

    if let Err(e) = result {
        let _ = std::fs::remove_file(&dest);
        return Err(e);
    }

    result
}

pub fn unpair_italic(lock: &Mutex<()>, id: &str) -> Result<CustomFont> {
    let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut m = read_manifest()?;
    let row = m
        .fonts
        .iter_mut()
        .find(|f| f.id == id)
        .ok_or_else(|| anyhow!("font {id} not found"))?;
    if let Some(italic) = row.italic_file_basename.take() {
        if let Ok(path) = font_italic_file_path(id, &italic) {
            let _ = std::fs::remove_file(&path);
        }
    }
    let cloned = row.clone();
    write_manifest(&m)?;
    Ok(cloned)
}

/// Startup reconciler — orphaned font files (manifest row gone but
/// `<id>.<ext>` still on disk) get unlinked. Catches anything that fell
/// through the cracks of `remove`'s best-effort unlink, and any future
/// "user nuked the manifest by hand" debugging.
pub fn reconcile_orphans(lock: &Mutex<()>) -> Result<usize> {
    let _g = lock.lock().unwrap_or_else(|e| e.into_inner());
    let dir = match fonts_dir() {
        Ok(d) => d,
        Err(_) => return Ok(0),
    };
    if !dir.exists() {
        return Ok(0);
    }
    let manifest = read_manifest()?;
    // Known ids — the regular file is `<id>.<ext>`, italic file is
    // `<id>.italic.<ext>`. Both share the leading id segment, so the
    // orphan check just needs the `<id>` portion.
    let known: std::collections::HashSet<String> =
        manifest.fonts.iter().map(|f| f.id.clone()).collect();
    let mut removed = 0usize;
    for entry in std::fs::read_dir(&dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // Skip the manifest itself + tmp files.
        if name == "fonts.json" || name == "fonts.json.tmp" {
            continue;
        }
        // Recover the id segment. For `<id>.<ext>` it's the file_stem.
        // For `<id>.italic.<ext>` the stem is `<id>.italic`; split on
        // `.` and take the first piece so both formats reduce to id.
        let stem = std::path::Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let id_segment = stem.split('.').next().unwrap_or("");
        if known.contains(id_segment) {
            // If this is an italic file but the row no longer references
            // it, also unlink. Catches "user paired then unpaired but a
            // crash mid-write left the file behind".
            if name.contains(".italic.") {
                let row = manifest.fonts.iter().find(|f| f.id == id_segment);
                let still_paired = row
                    .and_then(|r| r.italic_file_basename.as_ref())
                    .map(|basename| {
                        font_italic_file_path(id_segment, basename)
                            .map(|p| p == path)
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !still_paired {
                    if let Err(e) = std::fs::remove_file(&path) {
                        tracing::warn!(path = %path.display(), error = %e, "orphan italic unlink failed");
                    } else {
                        removed += 1;
                    }
                }
            }
            continue;
        }
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!(path = %path.display(), error = %e, "orphan font unlink failed");
        } else {
            removed += 1;
        }
    }
    if removed > 0 {
        tracing::info!(removed, "startup reconcile: removed orphan font files");
    }
    Ok(removed)
}

fn magic_matches(bytes: &[u8], ext: &str) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let head = &bytes[..bytes.len().min(16)];
    for m in MAGICS {
        if head.starts_with(m.bytes) && m.ext_kind == ext {
            return true;
        }
    }
    // Slight lenience: ttf/otf containers occasionally swap (some
    // foundries pack PostScript outlines in `.ttf`-named files). Allow
    // any ttc/ttf/otf magic to pass for ttf/otf/ttc extensions to keep
    // honest mismatches from blocking real fonts.
    if matches!(ext, "ttf" | "otf" | "ttc") {
        for m in MAGICS {
            if head.starts_with(m.bytes) && matches!(m.ext_kind, "ttf" | "otf" | "ttc") {
                return true;
            }
        }
    }
    false
}

fn strip_ext(basename: &str) -> &str {
    std::path::Path::new(basename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(basename)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ttf_bytes() -> Vec<u8> {
        let mut v = vec![0x00, 0x01, 0x00, 0x00];
        v.extend(std::iter::repeat(0u8).take(64));
        v
    }

    #[test]
    fn magic_accepts_ttf() {
        assert!(magic_matches(&ttf_bytes(), "ttf"));
    }

    #[test]
    fn magic_accepts_ttc_with_ttcf_header() {
        let mut v = b"ttcf".to_vec();
        v.extend(std::iter::repeat(0u8).take(64));
        assert!(magic_matches(&v, "ttc"));
    }

    #[test]
    fn magic_rejects_png_renamed_ttf() {
        let v = b"\x89PNG\r\n\x1a\n".to_vec();
        assert!(!magic_matches(&v, "ttf"));
    }

    #[test]
    fn magic_rejects_too_short() {
        assert!(!magic_matches(&[0x00, 0x01], "ttf"));
    }

    #[test]
    fn strip_ext_works() {
        assert_eq!(strip_ext("MapleMono-Regular.ttf"), "MapleMono-Regular");
        assert_eq!(strip_ext("noext"), "noext");
    }

    #[test]
    fn variable_detect_recognizes_brackets() {
        assert!(is_variable_filename("MapleMono[wght].ttf"));
        assert!(is_variable_filename("MapleMono-Italic[wght].ttf"));
        assert!(is_variable_filename("Foo[wght,ital].ttf"));
        assert!(is_variable_filename("Foo[ital,wght].ttf"));
        assert!(is_variable_filename("Foo[wdth,wght,ital].ttf"));
        assert!(is_variable_filename("FOO[WGHT].ttf")); // case-insensitive
    }

    #[test]
    fn variable_detect_recognizes_googlefonts_legacy() {
        assert!(is_variable_filename("JetBrainsMono-VariableFont_wght.ttf"));
    }

    #[test]
    fn variable_detect_negatives() {
        assert!(!is_variable_filename("Menlo-Regular.ttf"));
        assert!(!is_variable_filename("Operator-Mono-Bold.otf"));
        // Bracket without wght axis — not variable for our purposes.
        assert!(!is_variable_filename("Foo[wdth].ttf"));
        // Literal "wght" outside brackets — don't false-positive.
        assert!(!is_variable_filename("FooWghtBold.ttf"));
    }
}
