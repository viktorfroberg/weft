//! weft-cli — development / ops harness for driving weft without the GUI.
//!
//! Shares the same SQLite database as the desktop app
//! (`~/Library/Application Support/weft/weft.db`). Runs reconcile on every
//! invocation so the state stays consistent if you bounce between CLI and
//! GUI.

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use weft_lib::db::repo::{
    NewProject, NewWorkspace, NewWorkspaceRepo, ProjectRepo, TaskRepo, TaskWorktreeRepo,
    WorkspaceRepoRepo, WorkspacesRepo,
};
use weft_lib::git;
use weft_lib::services::task_create::{
    cleanup_task, create_task_with_worktrees, CreateTaskInput,
};
use weft_lib::services::{reconcile::reconcile_worktrees, worktrees_base_dir};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Parser)]
#[command(name = "weft-cli", version, about = "weft ops harness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage registered git projects.
    #[command(subcommand)]
    Projects(ProjectsCmd),
    /// Manage workspaces and their attached repos.
    #[command(subcommand)]
    Workspaces(WorkspacesCmd),
    /// Manage tasks and their worktrees.
    #[command(subcommand)]
    Task(TaskCmd),
    /// Reconcile task_worktrees rows against disk.
    Reconcile,
}

#[derive(Subcommand)]
enum ProjectsCmd {
    /// Register a new project (git repo).
    Add {
        /// Absolute path to the repo.
        #[arg(long)]
        path: PathBuf,
        /// Display name (defaults to directory basename).
        #[arg(long)]
        name: Option<String>,
        /// Override detected default branch.
        #[arg(long)]
        default_branch: Option<String>,
    },
    List,
}

#[derive(Subcommand)]
enum WorkspacesCmd {
    New {
        #[arg(long)]
        name: String,
    },
    AddRepo {
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        base_branch: Option<String>,
    },
    List,
    Show {
        #[arg(long)]
        workspace: String,
    },
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Create a task + fan out worktrees across all workspace repos.
    New {
        #[arg(long)]
        workspace: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        agent_preset: Option<String>,
    },
    List {
        #[arg(long)]
        workspace: String,
    },
    /// Remove all worktrees + delete the task row.
    Cleanup {
        #[arg(long)]
        task: String,
    },
}

fn main() -> Result<()> {
    // Stream logs to stderr so CLI stdout stays clean for grep/jq pipelines.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "weft=info,warn".into()),
        )
        .init();

    let cli = Cli::parse();
    let conn = weft_lib::db::open_and_migrate().context("open db")?;

    // If the desktop app is running, surface a heads-up — writes still
    // succeed (SQLite WAL handles concurrency) but the running UI won't
    // see them until it refetches. Advisory only; not blocking.
    if let Some(pid) = weft_lib::db::read_app_pid() {
        if weft_lib::db::is_process_alive(pid) {
            eprintln!(
                "warning: weft app is running (pid {pid}); UI won't see changes until it refetches"
            );
        }
    }

    let db = Arc::new(Mutex::new(conn));
    match cli.command {
        Command::Projects(c) => {
            let conn = db.lock().unwrap();
            projects(c, &conn)?
        }
        Command::Workspaces(c) => {
            let conn = db.lock().unwrap();
            workspaces(c, &conn)?
        }
        Command::Task(c) => tasks(c, &db)?,
        Command::Reconcile => {
            let conn = db.lock().unwrap();
            let report = reconcile_worktrees(&conn)?;
            println!(
                "reconcile: total={} still_ready={} marked_missing={}",
                report.total,
                report.still_ready,
                report.marked_missing.len()
            );
            for (tid, pid) in &report.marked_missing {
                println!("  missing: task={tid} project={pid}");
            }
        }
    }

    Ok(())
}

fn projects(c: ProjectsCmd, conn: &rusqlite::Connection) -> Result<()> {
    let repo = ProjectRepo::new(conn);
    match c {
        ProjectsCmd::Add {
            path,
            name,
            default_branch,
        } => {
            if !git::is_git_repo(&path) {
                return Err(anyhow!("{} is not a git repository", path.display()));
            }
            let default_branch = match default_branch {
                Some(b) => b,
                None => git::default_branch(&path).unwrap_or_else(|_| "main".into()),
            };
            let name = name.unwrap_or_else(|| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("repo")
                    .to_string()
            });
            let (p, _) = repo.insert(NewProject {
                name,
                main_repo_path: path.to_string_lossy().into_owned(),
                default_branch,
                color: None,
            })?;
            println!("{}\t{}\t{}", p.id, p.name, p.main_repo_path);
        }
        ProjectsCmd::List => {
            for p in repo.list()? {
                println!(
                    "{}\t{}\t{}\t{}",
                    p.id, p.name, p.default_branch, p.main_repo_path
                );
            }
        }
    }
    Ok(())
}

fn workspaces(c: WorkspacesCmd, conn: &rusqlite::Connection) -> Result<()> {
    match c {
        WorkspacesCmd::New { name } => {
            let (ws, _) = WorkspacesRepo::new(conn).insert(NewWorkspace {
                name,
                sort_order: None,
            })?;
            println!("{}\t{}", ws.id, ws.name);
        }
        WorkspacesCmd::AddRepo {
            workspace,
            project,
            base_branch,
        } => {
            let (row, _) = WorkspaceRepoRepo::new(conn).insert(NewWorkspaceRepo {
                workspace_id: workspace,
                project_id: project,
                base_branch,
                sort_order: None,
            })?;
            println!("{}:{}", row.workspace_id, row.project_id);
        }
        WorkspacesCmd::List => {
            for w in WorkspacesRepo::new(conn).list()? {
                println!("{}\t{}", w.id, w.name);
            }
        }
        WorkspacesCmd::Show { workspace } => {
            let rows = WorkspaceRepoRepo::new(conn).list_for_workspace(&workspace)?;
            for r in rows {
                let p = ProjectRepo::new(conn).get(&r.project_id)?;
                let name = p.as_ref().map(|x| x.name.clone()).unwrap_or_else(|| "?".into());
                let base = r.base_branch.clone().unwrap_or_else(|| {
                    p.as_ref()
                        .map(|x| x.default_branch.clone())
                        .unwrap_or_default()
                });
                println!("{}\t{}\t{}", r.project_id, name, base);
            }
        }
    }
    Ok(())
}

fn tasks(c: TaskCmd, db: &Arc<Mutex<rusqlite::Connection>>) -> Result<()> {
    match c {
        TaskCmd::New {
            workspace,
            name,
            agent_preset,
        } => {
            let base = worktrees_base_dir()?;
            let fallbacks = std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            ));
            let out = create_task_with_worktrees(
                db,
                &base,
                CreateTaskInput {
                    workspace_id: Some(workspace),
                    name,
                    agent_preset,
                    project_ids: vec![],
                    base_branches: Default::default(),
                    tickets: vec![],
                    warm_links: true,
                    initial_prompt: None,
                },
                fallbacks,
            )?;
            println!("task\t{}\t{}\t{}", out.task.id, out.task.slug, out.task.name);
            for w in &out.worktrees {
                println!(
                    "worktree\t{}\t{}\t{}",
                    w.project_name,
                    w.worktree_path.display(),
                    w.task_branch
                );
            }
        }
        TaskCmd::List { workspace } => {
            let conn = db.lock().unwrap();
            let tasks = TaskRepo::new(&conn).list_for_workspace(&workspace)?;
            for t in tasks {
                let wts = TaskWorktreeRepo::new(&conn).list_for_task(&t.id)?;
                println!(
                    "{}\t{}\t{}\t{}\t{} worktrees",
                    t.id,
                    t.slug,
                    t.name,
                    format!("{:?}", t.status).to_lowercase(),
                    wts.len()
                );
                for w in wts {
                    println!("  {}\t{}\t{}", w.project_id, w.status, w.worktree_path);
                }
            }
        }
        TaskCmd::Cleanup { task } => {
            let report = cleanup_task(db, &task)?;
            println!("cleaned up task {task}");
            for pb in &report.preserved_branches {
                println!(
                    "  kept branch {} in {} (has unmerged commits vs {})",
                    pb.branch, pb.project_name, pb.base_branch
                );
            }
        }
    }
    Ok(())
}
