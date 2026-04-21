//! Integration tests for all repos against an in-memory SQLite.
//!
//! Loaded via `#[cfg(test)] mod tests` in each repo's file would split them,
//! but keeping the end-to-end flows in one place makes multi-repo (workspace +
//! projects + junction) scenarios easier to read.

use super::{NewAgentPreset, NewProject, NewTask, NewWorkspace, NewWorkspaceRepo, PresetPatch, PresetRepo, ProjectRepo, TaskRepo, WorkspaceRepoRepo, WorkspacesRepo};
use crate::db::events::{Entity, Op};
use rusqlite::Connection;

/// Fresh in-memory DB with all migrations applied.
fn mk_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn.execute_batch(include_str!("../../../migrations/0001_init.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../../../migrations/0002_schema.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../../../migrations/0004_task_tickets_and_branch.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../../../migrations/0006_initial_prompt.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../../../migrations/0010_task_name_locked_at.sql"))
        .unwrap();
    conn
}

fn mk_project(conn: &Connection, name: &str, path: &str) -> String {
    let (p, _) = ProjectRepo::new(conn)
        .insert(NewProject {
            name: name.into(),
            main_repo_path: path.into(),
            default_branch: "main".into(),
            color: None,
        })
        .unwrap();
    p.id
}

#[test]
fn project_insert_list_delete_roundtrip() {
    let conn = mk_db();
    let repo = ProjectRepo::new(&conn);

    let (p, ev) = repo
        .insert(NewProject {
            name: "admin".into(),
            main_repo_path: "/repos/admin".into(),
            default_branch: "main".into(),
            color: Some("purple".into()),
        })
        .unwrap();
    assert_eq!(ev.entity, Entity::Project);
    assert_eq!(ev.op, Op::Insert);
    assert_eq!(ev.id, p.id);

    let all = repo.list().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "admin");

    let del = repo.delete(&p.id).unwrap();
    assert_eq!(del.op, Op::Delete);
    assert_eq!(repo.list().unwrap().len(), 0);
}

#[test]
fn project_main_repo_path_is_unique() {
    let conn = mk_db();
    let repo = ProjectRepo::new(&conn);
    repo.insert(NewProject {
        name: "a".into(),
        main_repo_path: "/repos/dup".into(),
        default_branch: "main".into(),
        color: None,
    })
    .unwrap();
    let err = repo
        .insert(NewProject {
            name: "b".into(),
            main_repo_path: "/repos/dup".into(),
            default_branch: "main".into(),
            color: None,
        })
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("unique"));
}

#[test]
fn workspace_with_two_repos_roundtrip() {
    let conn = mk_db();
    let admin = mk_project(&conn, "admin", "/repos/admin");
    let api = mk_project(&conn, "api", "/repos/api");

    let (ws, ws_ev) = WorkspacesRepo::new(&conn)
        .insert(NewWorkspace {
            name: "chat-widget".into(),
            sort_order: None,
        })
        .unwrap();
    assert_eq!(ws_ev.entity, Entity::Workspace);
    assert_eq!(ws_ev.op, Op::Insert);

    let junctions = WorkspaceRepoRepo::new(&conn);
    let (_, ev1) = junctions
        .insert(NewWorkspaceRepo {
            workspace_id: ws.id.clone(),
            project_id: admin.clone(),
            base_branch: None,
            sort_order: Some(0),
        })
        .unwrap();
    let (_, ev2) = junctions
        .insert(NewWorkspaceRepo {
            workspace_id: ws.id.clone(),
            project_id: api.clone(),
            base_branch: Some("develop".into()),
            sort_order: Some(1),
        })
        .unwrap();

    // Events use composite packed id
    assert!(ev1.id.contains(&ws.id) && ev1.id.contains(&admin));
    assert!(ev2.id.contains(&ws.id) && ev2.id.contains(&api));

    let rows = junctions.list_for_workspace(&ws.id).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].project_id, admin);
    assert_eq!(rows[0].base_branch, None);
    assert_eq!(rows[1].project_id, api);
    assert_eq!(rows[1].base_branch.as_deref(), Some("develop"));
}

#[test]
fn workspace_delete_cascades_junction_rows_and_tasks() {
    let conn = mk_db();
    let admin = mk_project(&conn, "admin", "/repos/admin");
    let (ws, _) = WorkspacesRepo::new(&conn)
        .insert(NewWorkspace {
            name: "scratch".into(),
            sort_order: None,
        })
        .unwrap();
    WorkspaceRepoRepo::new(&conn)
        .insert(NewWorkspaceRepo {
            workspace_id: ws.id.clone(),
            project_id: admin,
            base_branch: None,
            sort_order: None,
        })
        .unwrap();
    TaskRepo::new(&conn)
        .insert(NewTask {
            workspace_id: Some(ws.id.clone()),
            name: "first task".into(),
            agent_preset: None,
            initial_prompt: None,
        })
        .unwrap();

    WorkspacesRepo::new(&conn).delete(&ws.id).unwrap();
    assert!(WorkspaceRepoRepo::new(&conn)
        .list_for_workspace(&ws.id)
        .unwrap()
        .is_empty());
    assert!(TaskRepo::new(&conn)
        .list_for_workspace(&ws.id)
        .unwrap()
        .is_empty());
}

#[test]
fn task_slug_auto_suffixes_on_collision() {
    let conn = mk_db();
    let (ws, _) = WorkspacesRepo::new(&conn)
        .insert(NewWorkspace {
            name: "w".into(),
            sort_order: None,
        })
        .unwrap();
    let repo = TaskRepo::new(&conn);

    let (t1, _) = repo
        .insert(NewTask {
            workspace_id: Some(ws.id.clone()),
            name: "Chat Widget".into(),
            agent_preset: None,
            initial_prompt: None,
        })
        .unwrap();
    assert_eq!(t1.slug, "chat-widget");

    let (t2, _) = repo
        .insert(NewTask {
            workspace_id: Some(ws.id.clone()),
            name: "Chat Widget".into(),
            agent_preset: None,
            initial_prompt: None,
        })
        .unwrap();
    assert_eq!(t2.slug, "chat-widget-2");

    let (t3, _) = repo
        .insert(NewTask {
            workspace_id: Some(ws.id.clone()),
            name: "chat widget".into(),
            agent_preset: None,
            initial_prompt: None,
        })
        .unwrap();
    assert_eq!(t3.slug, "chat-widget-3");
}

#[test]
fn task_rejects_empty_slug_from_symbols_only_name() {
    let conn = mk_db();
    let (ws, _) = WorkspacesRepo::new(&conn)
        .insert(NewWorkspace {
            name: "w".into(),
            sort_order: None,
        })
        .unwrap();
    let err = TaskRepo::new(&conn)
        .insert(NewTask {
            workspace_id: Some(ws.id),
            name: "!!!".into(),
            agent_preset: None,
            initial_prompt: None,
        })
        .unwrap_err();
    assert!(err.to_string().contains("empty slug"));
}

// ---- Agent preset CRUD ----

fn mk_preset_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    for sql in [
        include_str!("../../../migrations/0001_init.sql"),
        include_str!("../../../migrations/0002_schema.sql"),
        include_str!("../../../migrations/0003_agent_presets.sql"),
        include_str!("../../../migrations/0004_task_tickets_and_branch.sql"),
        include_str!("../../../migrations/0007_claude_preset_prompt_arg.sql"),
        include_str!("../../../migrations/0008_claude_preset_prompt_before_addir.sql"),
        include_str!("../../../migrations/0009_task_context_shared.sql"),
    ] {
        conn.execute_batch(sql).unwrap();
    }
    conn
}

fn new_toy(name: &str) -> NewAgentPreset {
    NewAgentPreset {
        name: name.into(),
        command: "echo".into(),
        args_json: "[\"hi\"]".into(),
        env_json: "{}".into(),
        sort_order: None,
        bootstrap_prompt_template: None,
        bootstrap_delivery: None,
    }
}

fn toy_patch(name: &str) -> PresetPatch {
    PresetPatch {
        name: name.into(),
        command: "echo".into(),
        args_json: "[\"hi\"]".into(),
        env_json: "{}".into(),
        sort_order: 1,
        bootstrap_prompt_template: None,
        bootstrap_delivery: None,
    }
}

#[test]
fn preset_create_list_contains_seed_plus_new() {
    let conn = mk_preset_db();
    let repo = PresetRepo::new(&conn);
    let (p, ev) = repo.insert(new_toy("Toy")).unwrap();
    assert_eq!(ev.entity, Entity::Preset);
    assert_eq!(ev.op, Op::Insert);
    assert_eq!(p.name, "Toy");
    let list = repo.list().unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.iter().any(|p| p.name == "Claude Code"));
    assert!(list.iter().any(|p| p.name == "Toy"));
}

#[test]
fn preset_create_rejects_malformed_args_json() {
    let conn = mk_preset_db();
    let mut bad = new_toy("Bad");
    bad.args_json = "[not json".into();
    let err = PresetRepo::new(&conn).insert(bad).unwrap_err();
    assert!(err.to_string().contains("args must be a JSON array"));
}

#[test]
fn preset_create_rejects_malformed_env_json() {
    let conn = mk_preset_db();
    let mut bad = new_toy("Bad");
    bad.env_json = "[]".into();
    let err = PresetRepo::new(&conn).insert(bad).unwrap_err();
    assert!(err.to_string().contains("env must be a JSON object"));
}

#[test]
fn preset_update_roundtrips_and_rejects_unknown() {
    let conn = mk_preset_db();
    let repo = PresetRepo::new(&conn);
    let (p, _) = repo.insert(new_toy("Toy")).unwrap();
    let (updated, ev) = repo.update(&p.id, toy_patch("Toy Renamed")).unwrap();
    assert_eq!(updated.name, "Toy Renamed");
    assert_eq!(ev.op, Op::Update);

    let err = repo
        .update("does-not-exist", toy_patch("nope"))
        .unwrap_err();
    assert!(err.to_string().contains("preset not found"));
}

#[test]
fn preset_set_default_promotes_exactly_one() {
    let conn = mk_preset_db();
    let repo = PresetRepo::new(&conn);
    let (toy, _) = repo.insert(new_toy("Toy")).unwrap();
    repo.set_default(&toy.id).unwrap();

    let defaults: Vec<_> = repo
        .list()
        .unwrap()
        .into_iter()
        .filter(|p| p.is_default)
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].id, toy.id);
}

#[test]
fn preset_set_default_on_missing_id_rejects_and_leaves_existing_default() {
    let conn = mk_preset_db();
    let repo = PresetRepo::new(&conn);
    // Seed row has is_default = 1. A bogus set_default should not wipe it.
    let err = repo.set_default("does-not-exist").unwrap_err();
    assert!(err.to_string().contains("preset not found"));
    let defaults: Vec<_> = repo
        .list()
        .unwrap()
        .into_iter()
        .filter(|p| p.is_default)
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].name, "Claude Code");
}

#[test]
fn preset_delete_rejects_last_row() {
    let conn = mk_preset_db();
    let repo = PresetRepo::new(&conn);
    // Seed is the only row.
    let seed = repo
        .list()
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let err = repo.delete(&seed.id).unwrap_err();
    assert!(err.to_string().contains("cannot delete the only"));
    // Row still there.
    assert_eq!(repo.list().unwrap().len(), 1);
}

#[test]
fn preset_delete_of_default_promotes_next() {
    let conn = mk_preset_db();
    let repo = PresetRepo::new(&conn);
    let (toy, _) = repo.insert(new_toy("Toy")).unwrap();
    let seed_id = repo
        .list()
        .unwrap()
        .into_iter()
        .find(|p| p.name == "Claude Code")
        .unwrap()
        .id;

    // Seed is currently default. Deleting it should promote Toy.
    let ev = repo.delete(&seed_id).unwrap();
    assert_eq!(ev.op, Op::Delete);

    let defaults: Vec<_> = repo
        .list()
        .unwrap()
        .into_iter()
        .filter(|p| p.is_default)
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].id, toy.id);

    // And get_default() agrees.
    let d = repo.get_default().unwrap().unwrap();
    assert_eq!(d.id, toy.id);
}

#[test]
fn preset_delete_nondefault_leaves_default_alone() {
    let conn = mk_preset_db();
    let repo = PresetRepo::new(&conn);
    let (toy, _) = repo.insert(new_toy("Toy")).unwrap();
    repo.delete(&toy.id).unwrap();
    let d = repo.get_default().unwrap().unwrap();
    assert_eq!(d.name, "Claude Code");
}
