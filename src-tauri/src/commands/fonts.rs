use crate::services::fonts::{self, CustomFont};
use crate::AppState;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

pub const FONTS_CHANGED_EVENT: &str = "weft://custom-fonts-changed";

/// List the user's installed custom fonts.
#[tauri::command]
pub fn font_list(state: State<'_, AppState>) -> Result<Vec<CustomFont>, String> {
    fonts::list(&state.font_lock).map_err(|e| format!("{e:#}"))
}

/// Pop up the native file picker, then install the chosen font file.
/// Returns `None` if the user cancelled. The async wrapping is required
/// because `pick_file` is callback-based; we await on a tokio oneshot.
#[tauri::command]
pub async fn font_install_pick(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Option<CustomFont>, String> {
    crate::timed!("font_install_pick");

    // Native file picker — awaited via a oneshot since the plugin's API
    // is callback-style. The picker runs on the platform's file-dialog
    // thread; the await blocks the command future, not a tokio worker.
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<PathBuf>>();
    app.dialog()
        .file()
        .add_filter(
            "Fonts",
            &["ttf", "otf", "ttc", "woff", "woff2", "TTF", "OTF", "TTC"],
        )
        .set_title("Pick a font file")
        .pick_file(move |path| {
            let _ = tx.send(path.and_then(|p| p.into_path().ok()));
        });

    let picked = rx.await.map_err(|e| e.to_string())?;
    let Some(src) = picked else {
        return Ok(None);
    };

    // Install runs on a blocking task — file copy + manifest write are
    // synchronous and we don't want to occupy a tokio worker on a
    // multi-MB read/write.
    let lock = std::sync::Arc::clone(&state.font_lock);
    let src_clone = src.clone();
    let row = tokio::task::spawn_blocking(move || fonts::install(&lock, &src_clone))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;

    let _ = app.emit(FONTS_CHANGED_EVENT, ());
    Ok(Some(row))
}

#[tauri::command]
pub fn font_remove(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
    fonts::remove(&state.font_lock, &id).map_err(|e| format!("{e:#}"))?;
    let _ = app.emit(FONTS_CHANGED_EVENT, ());
    Ok(())
}

#[tauri::command]
pub fn font_rename(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
    name: String,
) -> Result<CustomFont, String> {
    let row = fonts::rename(&state.font_lock, &id, &name).map_err(|e| format!("{e:#}"))?;
    let _ = app.emit(FONTS_CHANGED_EVENT, ());
    Ok(row)
}

#[tauri::command]
pub fn font_set_ligatures(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
    on: bool,
) -> Result<CustomFont, String> {
    let row = fonts::set_ligatures(&state.font_lock, &id, on).map_err(|e| format!("{e:#}"))?;
    let _ = app.emit(FONTS_CHANGED_EVENT, ());
    Ok(row)
}

#[tauri::command]
pub fn font_set_variable(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
    on: bool,
) -> Result<CustomFont, String> {
    let row = fonts::set_variable(&state.font_lock, &id, on).map_err(|e| format!("{e:#}"))?;
    let _ = app.emit(FONTS_CHANGED_EVENT, ());
    Ok(row)
}

/// Pop the file picker, then pair the chosen italic-variant file with
/// an existing custom-font row. Replaces any prior pairing
/// idempotently.
#[tauri::command]
pub async fn font_pair_italic_pick(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
) -> Result<Option<CustomFont>, String> {
    crate::timed!("font_pair_italic_pick");

    let (tx, rx) = tokio::sync::oneshot::channel::<Option<PathBuf>>();
    app.dialog()
        .file()
        .add_filter(
            "Fonts",
            &["ttf", "otf", "ttc", "woff", "woff2", "TTF", "OTF", "TTC"],
        )
        .set_title("Pick the italic variant")
        .pick_file(move |path| {
            let _ = tx.send(path.and_then(|p| p.into_path().ok()));
        });

    let Some(src) = rx.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };

    let lock = std::sync::Arc::clone(&state.font_lock);
    let id_moved = id.clone();
    let row = tokio::task::spawn_blocking(move || fonts::pair_italic(&lock, &id_moved, &src))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;

    let _ = app.emit(FONTS_CHANGED_EVENT, ());
    Ok(Some(row))
}

#[tauri::command]
pub fn font_unpair_italic(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
) -> Result<CustomFont, String> {
    let row = fonts::unpair_italic(&state.font_lock, &id).map_err(|e| format!("{e:#}"))?;
    let _ = app.emit(FONTS_CHANGED_EVENT, ());
    Ok(row)
}
