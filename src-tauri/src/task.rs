//! Task slug + branch-name derivation.
//!
//! Kept as a leaf module (no git / db deps) so it can be unit-tested cheaply
//! and reused from both the hook server and task-creation commands.

/// Default branch prefix. Can be overridden per-workspace in later phases;
/// for Phase 1 this is a constant.
pub const DEFAULT_BRANCH_PREFIX: &str = "weft/";

/// Derive a URL/filesystem-safe slug from a user-entered task name.
///
/// Rules:
/// - Lowercase, ASCII only, hyphen-separated
/// - Collapses whitespace and punctuation to single hyphens
/// - Trims leading/trailing hyphens
/// - Caps at 50 characters (arbitrary but keeps paths sane)
///
/// Collision handling lives in Phase 2 (DB-level uniqueness with `-1`, `-2`
/// suffixes). This function is deterministic and caller-side-stateless.
pub fn derive_slug(name: &str) -> String {
    let raw = slug::slugify(name);
    if raw.len() <= 50 {
        raw
    } else {
        raw.chars().take(50).collect::<String>().trim_end_matches('-').to_string()
    }
}

/// Derive the git branch name a task's worktrees sit on.
/// Example: `weft/chat-widget`
pub fn derive_branch_name(slug: &str) -> String {
    format!("{DEFAULT_BRANCH_PREFIX}{slug}")
}

/// Derive a slug from a list of ticket IDs (e.g. `["ABC-123", "ABC-124"]`).
///
/// Rules:
/// - Lowercased, hyphen-joined.
/// - Shared team-prefix dedupe: when every id has the same `<prefix>-`
///   part (e.g. `ABC-`), the prefix is kept only on the first id.
///   `["ABC-123", "ABC-124"]` → `"abc-123-124"`.
/// - Mixed teams keep full ids: `["ABC-12", "XYZ-9"]` → `"abc-12-xyz-9"`.
/// - Caps at 50 chars (same constraint as `derive_slug`) to keep
///   filesystem paths sane.
pub fn derive_slug_from_tickets(ids: &[&str]) -> String {
    if ids.is_empty() {
        return String::new();
    }

    let parts: Vec<(Option<String>, String)> = ids
        .iter()
        .map(|id| {
            let lc = id.to_lowercase();
            match lc.split_once('-') {
                Some((prefix, rest)) if !prefix.is_empty() && !rest.is_empty() => {
                    (Some(prefix.to_string()), rest.to_string())
                }
                _ => (None, lc),
            }
        })
        .collect();

    let all_same_prefix = parts
        .iter()
        .all(|(p, _)| p.is_some() && p.as_deref() == parts[0].0.as_deref());

    let slug = if all_same_prefix {
        let prefix = parts[0].0.as_deref().unwrap_or_default();
        let mut acc = format!("{prefix}-{}", parts[0].1);
        for p in &parts[1..] {
            acc.push('-');
            acc.push_str(&p.1);
        }
        acc
    } else {
        parts
            .iter()
            .map(|(p, rest)| match p {
                Some(pfx) => format!("{pfx}-{rest}"),
                None => rest.clone(),
            })
            .collect::<Vec<_>>()
            .join("-")
    };

    if slug.len() <= 50 {
        slug
    } else {
        slug.chars()
            .take(50)
            .collect::<String>()
            .trim_end_matches('-')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_basic() {
        assert_eq!(derive_slug("Chat Widget"), "chat-widget");
    }

    #[test]
    fn slug_punctuation() {
        assert_eq!(derive_slug("Fix bug #123: auth flow!"), "fix-bug-123-auth-flow");
    }

    #[test]
    fn slug_non_ascii() {
        assert_eq!(derive_slug("Fröberg's Tåsk"), "froberg-s-task");
    }

    #[test]
    fn slug_collapses_whitespace() {
        assert_eq!(derive_slug("   multiple   spaces   "), "multiple-spaces");
    }

    #[test]
    fn slug_empty_and_symbols_only() {
        assert_eq!(derive_slug("!!!"), "");
        assert_eq!(derive_slug(""), "");
    }

    #[test]
    fn slug_caps_at_50() {
        let long = "a".repeat(200);
        let s = derive_slug(&long);
        assert!(s.len() <= 50);
    }

    #[test]
    fn branch_name_format() {
        assert_eq!(derive_branch_name("chat-widget"), "weft/chat-widget");
    }

    #[test]
    fn slug_from_tickets_shared_prefix() {
        assert_eq!(
            derive_slug_from_tickets(&["ABC-123", "ABC-124"]),
            "abc-123-124"
        );
    }

    #[test]
    fn slug_from_tickets_mixed_prefixes() {
        assert_eq!(
            derive_slug_from_tickets(&["ABC-12", "XYZ-9"]),
            "abc-12-xyz-9"
        );
    }

    #[test]
    fn slug_from_tickets_single() {
        assert_eq!(derive_slug_from_tickets(&["ABC-123"]), "abc-123");
    }

    #[test]
    fn slug_from_tickets_empty() {
        assert_eq!(derive_slug_from_tickets(&[]), "");
    }

    #[test]
    fn slug_from_tickets_no_prefix() {
        // IDs without a team prefix fall back to joining verbatim.
        assert_eq!(derive_slug_from_tickets(&["just123", "just124"]), "just123-just124");
    }
}
