//! Linear GraphQL client.
//!
//! Two queries we actually need:
//! 1. `viewer_backlog` — open issues assigned to the authenticated user,
//!    to populate the ticket picker.
//! 2. `issue_by_identifier` — single issue by `ABC-123` form, for live
//!    chip render (title refresh, deleted-ticket detection).
//!
//! The picker backlog has an in-memory 30s TTL cache so opening and
//! re-opening the popover doesn't thrash the API. Single-issue fetches
//! are not cached — they power live chip titles and need to reflect
//! Linear's state now.

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{store::BacklogScope, AuthStatus, Ticket};

const API_URL: &str = "https://api.linear.app/graphql";
const BACKLOG_TTL: Duration = Duration::from_secs(30);

/// Cached backlog keyed by `(token, scope)`. The token portion blows the
/// cache on auth rotation; the scope portion prevents a stale fetch from
/// the previous filter leaking through after the user changes scope in
/// settings. Never logs the token.
#[derive(Default)]
pub struct BacklogCache {
    entries: Mutex<HashMap<(String, BacklogScope), (Instant, Vec<Ticket>)>>,
}

impl BacklogCache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn get(&self, token: &str, scope: BacklogScope) -> Option<Vec<Ticket>> {
        let entries = self.entries.lock();
        if let Some((at, tickets)) = entries.get(&(token.to_string(), scope)) {
            if at.elapsed() < BACKLOG_TTL {
                return Some(tickets.clone());
            }
        }
        None
    }

    fn put(&self, token: &str, scope: BacklogScope, tickets: Vec<Ticket>) {
        self.entries
            .lock()
            .insert((token.to_string(), scope), (Instant::now(), tickets));
    }

    pub fn clear(&self) {
        self.entries.lock().clear();
    }
}

#[derive(Serialize)]
struct GraphQlBody<'a> {
    query: &'a str,
    variables: serde_json::Value,
}

#[derive(Deserialize)]
struct GraphQlResponse<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

#[derive(Deserialize, Debug)]
struct GraphQlError {
    message: String,
}

async fn execute<T: for<'de> Deserialize<'de>>(
    token: &str,
    query: &str,
    variables: serde_json::Value,
) -> Result<T> {
    let client = reqwest::Client::new();
    // Linear's personal API keys (`lin_api_…`) are sent RAW in the
    // Authorization header — NO `Bearer ` prefix. That prefix is only
    // for OAuth access tokens, which we don't use in v1.0.x. If you're
    // tempted to "fix" this to `Bearer {token}`, don't — it'll break
    // auth against Linear. See their API docs.
    let resp = client
        .post(API_URL)
        .header("Authorization", token)
        .header("Content-Type", "application/json")
        .json(&GraphQlBody { query, variables })
        .send()
        .await
        .context("linear: POST /graphql")?;

    let status = resp.status();
    if !status.is_success() {
        // Read body for context but never log the token.
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "linear API error ({status}): {}",
            body.chars().take(300).collect::<String>()
        ));
    }

    let parsed: GraphQlResponse<T> = resp
        .json()
        .await
        .context("linear: decode GraphQL response")?;
    if !parsed.errors.is_empty() {
        let msgs = parsed
            .errors
            .into_iter()
            .map(|e| e.message)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(anyhow!("linear GraphQL errors: {msgs}"));
    }
    parsed
        .data
        .ok_or_else(|| anyhow!("linear: empty data in GraphQL response"))
}

// --- test_auth --------------------------------------------------------------

#[derive(Deserialize)]
struct ViewerResp {
    viewer: Viewer,
}
#[derive(Deserialize)]
struct Viewer {
    name: String,
    email: String,
}

/// Auth probe: `viewer { name email }`. Returns `ok=false` on API failure,
/// NOT Err — the settings UI wants to render the failure inline rather
/// than throw.
pub async fn test_auth(token: &str) -> AuthStatus {
    let q = "query Viewer { viewer { name email } }";
    match execute::<ViewerResp>(token, q, serde_json::json!({})).await {
        Ok(r) => AuthStatus {
            ok: true,
            viewer: Some(format!("{} <{}>", r.viewer.name, r.viewer.email)),
            error: None,
        },
        Err(e) => AuthStatus {
            ok: false,
            viewer: None,
            error: Some(e.to_string()),
        },
    }
}

// --- viewer_backlog ---------------------------------------------------------

#[derive(Deserialize)]
struct BacklogResp {
    viewer: BacklogViewer,
}
#[derive(Deserialize)]
struct BacklogViewer {
    #[serde(rename = "assignedIssues")]
    assigned_issues: IssueConnection,
}
#[derive(Deserialize)]
struct IssueConnection {
    nodes: Vec<IssueNode>,
}
#[derive(Deserialize)]
struct IssueNode {
    identifier: String,
    title: String,
    url: String,
    state: Option<IssueState>,
    assignee: Option<IssueAssignee>,
    /// Linear returns priority as a Float (0/1/2/3/4); we store as u8.
    #[serde(default)]
    priority: Option<f64>,
    cycle: Option<IssueCycle>,
}
#[derive(Deserialize)]
struct IssueState {
    name: String,
}
#[derive(Deserialize)]
struct IssueAssignee {
    name: String,
}
#[derive(Deserialize)]
struct IssueCycle {
    name: Option<String>,
    number: Option<i32>,
}

/// State-type filter fragment per scope. `viewer.assignedIssues` already
/// scopes to the auth user; we only vary the state-type predicate.
fn backlog_filter(scope: BacklogScope) -> serde_json::Value {
    match scope {
        BacklogScope::InProgress => serde_json::json!({
            "state": { "type": { "in": ["started"] } }
        }),
        BacklogScope::Actionable => serde_json::json!({
            "state": { "type": { "in": ["started", "unstarted"] } }
        }),
        BacklogScope::AllOpen => serde_json::json!({
            "state": { "type": { "nin": ["completed", "canceled"] } }
        }),
    }
}

const BACKLOG_QUERY: &str = r#"
query Backlog($filter: IssueFilter) {
  viewer {
    assignedIssues(
      filter: $filter
      first: 50
      orderBy: updatedAt
    ) {
      nodes {
        identifier
        title
        url
        priority
        state { name }
        assignee { name }
        cycle { name number }
      }
    }
  }
}
"#;

/// Fetch issues assigned to the viewer, filtered by the chosen scope.
/// Cached for 30s per (token, scope).
pub async fn viewer_backlog(
    cache: &BacklogCache,
    token: &str,
    scope: BacklogScope,
) -> Result<Vec<Ticket>> {
    if let Some(cached) = cache.get(token, scope) {
        return Ok(cached);
    }
    let vars = serde_json::json!({ "filter": backlog_filter(scope) });
    let resp: BacklogResp = execute(token, BACKLOG_QUERY, vars).await?;
    let mut tickets = resp
        .viewer
        .assigned_issues
        .nodes
        .into_iter()
        .map(|n| Ticket {
            provider: "linear".into(),
            external_id: n.identifier,
            title: n.title,
            url: n.url,
            status: n.state.map(|s| s.name),
            assignee: n.assignee.map(|a| a.name),
            priority: n.priority.map(|p| p as u8),
            cycle_name: n.cycle.as_ref().and_then(|c| c.name.clone()),
            cycle_number: n.cycle.as_ref().and_then(|c| c.number),
        })
        .collect::<Vec<_>>();
    // Linear's `orderBy: updatedAt` already sorted the list. Re-sort by
    // priority (Urgent first, "No priority" last) so the top of the strip
    // is always the most urgent work; updatedAt order is preserved within
    // each priority bucket because `sort_by_key` is stable.
    tickets.sort_by_key(|t| priority_sort_key(t.priority));
    cache.put(token, scope, tickets.clone());
    Ok(tickets)
}

/// Map Linear priority to a sort key. Linear uses 0=No priority, 1=Urgent,
/// 2=High, 3=Medium, 4=Low — so 1..=4 are already correctly ascending,
/// but 0 ("No priority") needs to sink to the bottom.
fn priority_sort_key(p: Option<u8>) -> u8 {
    match p {
        Some(0) | None => u8::MAX,
        Some(n) => n,
    }
}

// --- issue_by_identifier ----------------------------------------------------

#[derive(Deserialize)]
struct IssueResp {
    issue: Option<IssueNode>,
}

const ISSUE_QUERY: &str = r#"
query Issue($id: String!) {
  issue(id: $id) {
    identifier
    title
    url
    priority
    state { name type }
    assignee { name }
    cycle { name number }
  }
}
"#;

/// Fetch a single issue by its `ABC-123` identifier. Returns `None` if
/// the issue doesn't exist OR we don't have access to it — UI renders
/// this as `ABC-123 (unavailable)` instead of crashing.
pub async fn issue_by_identifier(token: &str, external_id: &str) -> Result<Option<Ticket>> {
    let resp: IssueResp = execute(
        token,
        ISSUE_QUERY,
        serde_json::json!({ "id": external_id }),
    )
    .await?;
    Ok(resp.issue.map(|n| Ticket {
        provider: "linear".into(),
        external_id: n.identifier,
        title: n.title,
        url: n.url,
        status: n.state.map(|s| s.name),
        assignee: n.assignee.map(|a| a.name),
        priority: n.priority.map(|p| p as u8),
        cycle_name: n.cycle.as_ref().and_then(|c| c.name.clone()),
        cycle_number: n.cycle.as_ref().and_then(|c| c.number),
    }))
}

// (Previously: `fetch_markdown` + `format_ticket_md` helpers powered the
// `.weft/tickets.md` sidecar file. That pipeline was dropped in favor of
// inlining ticket context into the agent's first user message. See
// `services/task_tickets.rs` header comment.)
