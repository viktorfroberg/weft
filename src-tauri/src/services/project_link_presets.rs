//! Warm-worktree preset catalog. One source of truth for what each
//! preset name expands to; callable from both the Tauri
//! `project_links_preset_apply` command and from
//! `AddProjectDialog` on project creation.
//!
//! Adding a new preset: one new entry in `PRESETS`. The frontend
//! queries `list_presets()` to render picker options, so no UI-side
//! list to keep in sync.

use crate::db::repo::{LinkType, ProjectLinkInput};

/// A named preset — what the dropdown / radio shows, plus the list of
/// `(path, link_type)` pairs it expands to.
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub links: &'static [(&'static str, LinkType)],
}

/// Canonical preset list. Expand by appending, not inserting — the
/// `id` is a stable key the frontend segments on.
pub const PRESETS: &[Preset] = &[
    Preset {
        id: "node",
        name: "Node / Bun",
        links: &[
            ("node_modules", LinkType::Symlink),
            (".env", LinkType::Symlink),
            (".env.local", LinkType::Symlink),
            (".env.development.local", LinkType::Symlink),
            (".vite", LinkType::Clone),
            (".turbo", LinkType::Clone),
            ("dist", LinkType::Clone),
            (".cache", LinkType::Clone),
        ],
    },
    Preset {
        id: "next",
        name: "Next.js",
        // Inherits Node/Bun. If you change Node, you should revisit Next.
        links: &[
            ("node_modules", LinkType::Symlink),
            (".env", LinkType::Symlink),
            (".env.local", LinkType::Symlink),
            (".env.development.local", LinkType::Symlink),
            (".next", LinkType::Clone),
            ("next-env.d.ts", LinkType::Symlink),
            (".vite", LinkType::Clone),
            (".turbo", LinkType::Clone),
            ("dist", LinkType::Clone),
            (".cache", LinkType::Clone),
        ],
    },
    Preset {
        id: "rust",
        name: "Rust",
        links: &[
            ("target", LinkType::Clone),
            (".env", LinkType::Symlink),
        ],
    },
    Preset {
        id: "python",
        name: "Python",
        links: &[
            (".venv", LinkType::Symlink),
            ("__pycache__", LinkType::Clone),
            (".pytest_cache", LinkType::Clone),
            (".ruff_cache", LinkType::Clone),
            (".mypy_cache", LinkType::Clone),
            (".env", LinkType::Symlink),
        ],
    },
];

/// Resolve a preset id to its link list. Returns `None` for unknown ids
/// (including "custom" — the frontend shouldn't call preset-apply for
/// Custom; it should edit links directly).
pub fn preset_inputs(preset_id: &str) -> Option<Vec<ProjectLinkInput>> {
    PRESETS.iter().find(|p| p.id == preset_id).map(|p| {
        p.links
            .iter()
            .map(|(path, link_type)| ProjectLinkInput {
                path: (*path).to_string(),
                link_type: *link_type,
            })
            .collect()
    })
}

/// JSON-friendly descriptor the frontend renders as a segmented control.
#[derive(serde::Serialize)]
pub struct PresetDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub paths: Vec<String>,
}

pub fn list_descriptors() -> Vec<PresetDescriptor> {
    PRESETS
        .iter()
        .map(|p| PresetDescriptor {
            id: p.id,
            name: p.name,
            paths: p.links.iter().map(|(p, _)| (*p).to_string()).collect(),
        })
        .collect()
}

/// Detect the most likely preset for a freshly-added project by
/// scanning for lockfiles. Returns `None` when nothing matches — the
/// AddProjectDialog shows "Leave cold" pre-selected in that case.
pub fn detect_preset_from_path(repo_path: &std::path::Path) -> Option<&'static str> {
    let has = |name: &str| repo_path.join(name).exists();

    // Prefer Next.js over plain Node when next.config exists.
    if has("next.config.js")
        || has("next.config.mjs")
        || has("next.config.ts")
    {
        return Some("next");
    }
    if has("bun.lockb")
        || has("package-lock.json")
        || has("pnpm-lock.yaml")
        || has("yarn.lock")
        || has("package.json")
    {
        return Some("node");
    }
    if has("Cargo.toml") {
        return Some("rust");
    }
    if has("pyproject.toml") || has("requirements.txt") || has("Pipfile") {
        return Some("python");
    }
    None
}
