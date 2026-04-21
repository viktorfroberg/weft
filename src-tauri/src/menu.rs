//! Native macOS menu bar.
//!
//! macOS users look at the menu bar first when hunting for a feature or a
//! keyboard shortcut. Every global action weft has a shortcut for also has
//! a menu entry here, so the shortcut is discoverable without a cheatsheet.
//!
//! Menu items don't call the frontend directly — they emit `menu:<id>`
//! Tauri events which `src/lib/menu.ts` listens to and dispatches.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Emitter, Runtime,
};

/// Event name. One channel, payload is the menu id (e.g. "new_workspace").
pub const MENU_EVENT: &str = "menu";

pub fn build_and_install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    // App menu (first submenu on macOS). We override the default About item
    // with our own id so the frontend can render a custom dialog (logo +
    // version + GitHub link) instead of Tauri's generic folder-icon panel.
    let app_menu = Submenu::with_items(
        app,
        "weft",
        true,
        &[
            &MenuItem::with_id(app, "about", "About weft", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItem::with_id(
                app,
                "new_workspace",
                "New Workspace",
                true,
                Some("CmdOrCtrl+N"),
            )?,
            &MenuItem::with_id(
                app,
                "new_task",
                "New Task",
                true,
                Some("CmdOrCtrl+Shift+N"),
            )?,
            &MenuItem::with_id(
                app,
                "add_project",
                "Add Project…",
                true,
                Some("CmdOrCtrl+P"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &MenuItem::with_id(
                app,
                "toggle_sidebar",
                "Toggle Sidebar",
                true,
                Some("CmdOrCtrl+B"),
            )?,
            &MenuItem::with_id(
                app,
                "toggle_mode",
                "Toggle Changes Panel",
                true,
                Some("CmdOrCtrl+\\"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "back", "Back", true, Some("Escape"))?,
        ],
    )?;

    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    let help_menu = Submenu::with_items(
        app,
        "Help",
        true,
        &[&MenuItem::with_id(
            app,
            "shortcuts",
            "Keyboard Shortcuts",
            true,
            Some("CmdOrCtrl+/"),
        )?],
    )?;

    let menu = Menu::with_items(
        app,
        &[
            &app_menu,
            &file_menu,
            &edit_menu,
            &view_menu,
            &window_menu,
            &help_menu,
        ],
    )?;
    app.set_menu(menu)?;

    let app_for_handler = app.clone();
    app.on_menu_event(move |_app, event| {
        let id = event.id().0.as_str();
        // Emit to frontend. Frontend has the state (current route, etc.)
        // so Rust stays ignorant of UI concerns.
        if let Err(e) = app_for_handler.emit(MENU_EVENT, id) {
            tracing::warn!(error = %e, "emit menu event failed");
        }
    });

    Ok(())
}
