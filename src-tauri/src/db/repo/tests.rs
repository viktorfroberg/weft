//! Integration tests for all repos against an in-memory SQLite.
//!
//! Loaded via `#[cfg(test)] mod tests` in each repo's file would split them,
//! but keeping the end-to-end flows in one place makes multi-repo (workspace +
//! projects + junction) scenarios easier to read.

use super::{NewProject, NewTask, NewWorkspace, NewWorkspaceRepo, ProjectRepo, TaskRepo, WorkspaceRepoRepo, WorkspacesRepo};
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
