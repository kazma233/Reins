use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, params_from_iter};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    ContentBlock, SessionDetail, SessionEvent, SessionEventPage, SessionExporter, SessionFileEntry,
    SessionMessage, SessionMessagePage, SessionOverview, SessionReader, SessionSummary, SourceApp,
    TimelineCacheEntry,
    family_index::{Family, FamilyIndexCacheEntry, FamilyRow},
    family_timeline::{
        FamilyAgentLabel, cached_family_events, cached_family_messages, family_agents,
    },
};

#[derive(Clone, Debug)]
struct OpenCodeSessionRow {
    id: String,
    parent_id: Option<String>,
    directory: String,
    title: String,
    time_created: i64,
    time_updated: i64,
}

type OpenCodeSessionFamily = Family<OpenCodeSessionRow>;
type OpenCodeFamilyIndexCacheEntry = FamilyIndexCacheEntry<OpenCodeSessionRow>;

impl FamilyRow for OpenCodeSessionRow {
    // OpenCode rows come from SQLite and have no path column; the transcript
    // path is derived from the session id on demand.
    fn member_path(&self) -> std::borrow::Cow<'_, Path> {
        std::borrow::Cow::Owned(session_path(&self.id))
    }

    fn family_root_id(&self) -> &str {
        &self.id
    }

    fn member_id(&self) -> &str {
        &self.id
    }

    fn member_created_at(&self) -> Option<i64> {
        Some(self.time_created)
    }

    fn member_updated_at(&self) -> Option<i64> {
        Some(self.time_updated)
    }

    fn member_tie_breaker(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Borrowed(&self.id)
    }
}

#[derive(Clone)]
struct OpenCodeMessageRow {
    id: String,
    session_id: String,
    time_created: i64,
    value: Value,
}

#[derive(Clone)]
struct OpenCodePartRow {
    id: String,
    session_id: String,
    message_id: Option<String>,
    time_created: i64,
    value: Value,
}

#[derive(Default)]
struct OpenCodeImportStats {
    additions: usize,
    deletions: usize,
    files: usize,
}

static OPEN_CODE_TIMELINE_CACHE: LazyLock<Mutex<HashMap<String, TimelineCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static OPEN_CODE_FAMILY_INDEX_CACHE: LazyLock<Mutex<Option<OpenCodeFamilyIndexCacheEntry>>> =
    LazyLock::new(|| Mutex::new(None));

pub(crate) struct OpenCodeBackend;

pub(crate) static BACKEND: OpenCodeBackend = OpenCodeBackend;

fn lock_timeline_cache()
-> Result<std::sync::MutexGuard<'static, HashMap<String, TimelineCacheEntry>>> {
    OPEN_CODE_TIMELINE_CACHE
        .lock()
        .map_err(|_| anyhow!("OpenCode timeline cache lock was poisoned"))
}

fn lock_family_index_cache()
-> Result<std::sync::MutexGuard<'static, Option<OpenCodeFamilyIndexCacheEntry>>> {
    OPEN_CODE_FAMILY_INDEX_CACHE
        .lock()
        .map_err(|_| anyhow!("OpenCode family index cache lock was poisoned"))
}

impl SessionReader for OpenCodeBackend {
    fn list_entries(&self) -> Result<Vec<SessionFileEntry>> {
        let mut entries = list_session_families()?
            .into_iter()
            .map(|family| {
                let summary = family_summary(&family);
                SessionFileEntry {
                    path: session_path(&family.root.id),
                    sort_timestamp: family_updated_at(&family),
                    summary: Some(summary),
                }
            })
            .collect::<Vec<_>>();

        super::sort_entries(&mut entries);
        Ok(entries)
    }

    fn clear_cache(&self) -> Result<()> {
        lock_timeline_cache()?.clear();
        *lock_family_index_cache()? = None;
        Ok(())
    }

    fn resolve_path(&self, source_session_id: &str) -> Result<PathBuf> {
        Ok(session_path(source_session_id))
    }

    fn parse_summary(&self, path: &Path) -> Result<SessionSummary> {
        self::parse_summary(path)
    }

    fn parse_overview(&self, path: &Path) -> Result<SessionOverview> {
        self::parse_overview(path)
    }

    fn parse_messages_page(
        &self,
        path: &Path,
        offset: usize,
        limit: usize,
    ) -> Result<SessionMessagePage> {
        self::parse_messages_page(path, offset, limit)
    }

    fn parse_events_page(
        &self,
        path: &Path,
        offset: usize,
        limit: usize,
    ) -> Result<SessionEventPage> {
        self::parse_events_page(path, offset, limit)
    }

    fn parse_agent_messages(
        &self,
        path: &Path,
        agent_session_id: &str,
    ) -> Result<Vec<SessionMessage>> {
        let family = session_family_for_path(path)?;
        let messages = cached_messages_for_family(&family)?;
        Ok(super::family_timeline::agent_messages(
            messages,
            agent_session_id,
        ))
    }

    fn parse_detail(&self, path: &Path) -> Result<SessionDetail> {
        self::parse_detail(path)
    }
}

impl SessionExporter for OpenCodeBackend {
    fn planned_import_paths(
        &self,
        _summary: &SessionSummary,
        _session_id: &str,
    ) -> Result<Vec<String>> {
        Ok(vec![db_path()?.display().to_string()])
    }

    fn export_session(
        &self,
        detail: &SessionDetail,
        new_session_id: &str,
    ) -> Result<(String, Vec<String>)> {
        write_session(detail, new_session_id)
    }
}

pub(crate) fn root() -> Result<PathBuf> {
    Ok(crate::support::fs::user_home_dir()
        .context("Unable to determine home directory")?
        .join(".local")
        .join("share")
        .join("opencode"))
}

pub(crate) fn db_path() -> Result<PathBuf> {
    Ok(root()?.join("opencode.db"))
}

pub(crate) fn session_path(session_id: &str) -> PathBuf {
    root()
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .join("session")
        .join(format!("{session_id}.opencode"))
}

pub(crate) fn delete_session(path: &Path) -> Result<()> {
    let family = session_family_for_path(path)?;

    for member in &family.members {
        let output = Command::new("opencode")
            .arg("session")
            .arg("delete")
            .arg(&member.id)
            .output()
            .with_context(|| format!("Failed to execute opencode session delete {}", member.id))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let exit_code = output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string());
            bail!(
                "OpenCode delete command failed for {}. exit_code: {exit_code}\nstdout:\n{}\nstderr:\n{}",
                member.id,
                stdout.trim(),
                stderr.trim()
            );
        }

        let diff_path = root()?
            .join("storage/session_diff")
            .join(format!("{}.json", member.id));
        if diff_path.exists() {
            fs::remove_file(&diff_path)
                .with_context(|| format!("Failed to delete {}", diff_path.display()))?;
        }
    }

    lock_timeline_cache()?.clear();
    *lock_family_index_cache()? = None;
    Ok(())
}

fn open_connection() -> Result<Connection> {
    Connection::open(db_path()?).context("Failed to open OpenCode sqlite database")
}

fn list_session_rows() -> Result<Vec<OpenCodeSessionRow>> {
    let connection = open_connection()?;
    let mut statement = connection.prepare(
        "SELECT id, parent_id, directory, title, time_created, time_updated FROM session ORDER BY time_updated DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(OpenCodeSessionRow {
            id: row.get(0)?,
            parent_id: row.get(1)?,
            directory: row.get(2)?,
            title: row.get(3)?,
            time_created: row.get(4)?,
            time_updated: row.get(5)?,
        })
    })?;

    rows.collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to read OpenCode sessions")
}

fn list_session_families() -> Result<Vec<OpenCodeSessionFamily>> {
    Ok(family_index()?.index.families)
}

// Member/family ordering and the path map live in the shared engine; this
// only groups rows along the SQL parent_id chain and picks each family root.
fn build_session_families(rows: Vec<OpenCodeSessionRow>) -> Vec<OpenCodeSessionFamily> {
    let by_id = rows
        .iter()
        .cloned()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    let mut grouped = HashMap::<String, Vec<OpenCodeSessionRow>>::new();

    for row in rows {
        let family_root_id = root_session_id(&row, &by_id);
        grouped.entry(family_root_id).or_default().push(row);
    }

    grouped
        .into_iter()
        .filter_map(|(root_id, members)| {
            let root = members
                .iter()
                .find(|row| row.id == root_id)
                .cloned()
                .or_else(|| members.first().cloned())?;

            Some(OpenCodeSessionFamily { root, members })
        })
        .collect()
}

fn opencode_db_timestamp() -> Result<i64> {
    crate::support::time::file_modified_timestamp_millis(&db_path()?)
}

fn family_index() -> Result<OpenCodeFamilyIndexCacheEntry> {
    let source_key = db_path()?.display().to_string();
    let updated_at = opencode_db_timestamp()?;

    if let Some(entry) = lock_family_index_cache()?
        .as_ref()
        .filter(|entry| entry.is_valid(&source_key, updated_at))
        .cloned()
    {
        return Ok(entry);
    }

    // No id map here: OpenCode resolves session ids straight from the
    // database, so the engine runs without the dual-write map.
    let entry = OpenCodeFamilyIndexCacheEntry {
        source_key,
        updated_at,
        index: super::family_index::FamilyIndex::build(
            build_session_families(list_session_rows()?),
        ),
    };

    *lock_family_index_cache()? = Some(entry.clone());

    Ok(entry)
}

fn root_session_id(
    row: &OpenCodeSessionRow,
    by_id: &HashMap<String, OpenCodeSessionRow>,
) -> String {
    let mut current_id = row.id.clone();
    let mut parent_id = row.parent_id.clone();
    let mut visited = HashSet::from([current_id.clone()]);

    while let Some(next_parent_id) = parent_id {
        if !visited.insert(next_parent_id.clone()) {
            break;
        }

        let Some(parent) = by_id.get(&next_parent_id) else {
            current_id = next_parent_id;
            break;
        };

        current_id = parent.id.clone();
        parent_id = parent.parent_id.clone();
    }

    current_id
}

fn parse_summary(path: &Path) -> Result<SessionSummary> {
    let family = session_family_for_path(path)?;
    Ok(family_summary(&family))
}

fn parse_overview(path: &Path) -> Result<SessionOverview> {
    let family = session_family_for_path(path)?;
    let summary = family_summary(&family);
    let member_ids: Vec<&str> = family.members.iter().map(|row| row.id.as_str()).collect();
    let marker_count = family.members.len().saturating_sub(1);
    let (message_count, event_count) = count_family_records(&member_ids)?;

    Ok(SessionOverview {
        summary,
        source_paths: family_source_paths(&family),
        message_count: Some(message_count + marker_count),
        event_count: Some(event_count + marker_count),
        agents: family_agents(&family, |row| {
            if row.id == family.root.id {
                FamilyAgentLabel::Root
            } else {
                FamilyAgentLabel::Child(row.title.clone())
            }
        }),
    })
}

fn parse_messages_page(
    path: &Path,
    offset: usize,
    limit: usize,
) -> Result<SessionMessagePage> {
    let family = session_family_for_path(path)?;
    let all_messages = cached_messages_for_family(&family)?;
    let (messages, start, next_offset, total_count) =
        crate::support::paging::slice_page(&all_messages, offset, limit);

    Ok(SessionMessagePage {
        messages,
        offset: start,
        limit,
        next_offset,
        total_count,
        has_more: next_offset.is_some(),
    })
}

fn parse_events_page(path: &Path, offset: usize, limit: usize) -> Result<SessionEventPage> {
    let family = session_family_for_path(path)?;
    let all_events = cached_events_for_family(&family)?;
    let (page_events, start, next_offset, total_count) =
        crate::support::paging::slice_page(&all_events, offset, limit);

    Ok(SessionEventPage {
        events: page_events,
        offset: start,
        limit,
        next_offset,
        total_count,
        has_more: next_offset.is_some(),
    })
}

fn parse_detail(path: &Path) -> Result<SessionDetail> {
    let family = session_family_for_path(path)?;
    let summary = family_summary(&family);
    let messages = cached_messages_for_family(&family)?;
    let events = cached_events_for_family(&family)?;

    Ok(SessionDetail {
        summary,
        source_paths: family_source_paths(&family),
        messages,
        events,
    })
}

fn session_family_for_path(path: &Path) -> Result<OpenCodeSessionFamily> {
    let key = crate::support::fs::path_key(path);
    family_index()?
        .index
        .sessions_by_path
        .get(&key)
        .cloned()
        .ok_or_else(|| anyhow!("Could not find OpenCode session for {}", path.display()))
}

fn family_title(family: &OpenCodeSessionFamily) -> String {
    let child_count = family.members.len().saturating_sub(1);

    if child_count == 0 {
        return family.root.title.clone();
    }

    format!("{} (+{} subagents)", family.root.title, child_count)
}

fn family_summary(family: &OpenCodeSessionFamily) -> SessionSummary {
    SessionSummary {
        source_app: SourceApp::OpenCode,
        source_session_id: family.root.id.clone(),
        title: family_title(family),
        cwd: Some(family.root.directory.clone()),
        git_branch: None,
        transcript_path: session_path(&family.root.id).display().to_string(),
        created_at: Some(family_created_at(family)),
        updated_at: Some(family_updated_at(family)),
    }
}

// OpenCode time columns are NOT NULL, so the engine's Option-based
// aggregations collapse to plain i64 at the backend boundary.
fn family_created_at(family: &OpenCodeSessionFamily) -> i64 {
    family.created_at().unwrap_or_default()
}

fn family_updated_at(family: &OpenCodeSessionFamily) -> i64 {
    family.updated_at().unwrap_or_default()
}

// OpenCode prepends the database file itself: member rows are joined out of
// it, so imports and deletions touch it without touching any transcript.
fn family_source_paths(family: &OpenCodeSessionFamily) -> Vec<String> {
    let mut paths = vec![
        db_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "<opencode-db>".to_string()),
    ];

    paths.extend(family.source_paths());
    paths
}

fn cached_messages_for_family(family: &OpenCodeSessionFamily) -> Result<Vec<SessionMessage>> {
    cached_family_messages(
        &OPEN_CODE_TIMELINE_CACHE,
        "OpenCode timeline",
        family.root.id.clone(),
        family_cache_timestamp(family)?,
        || load_messages_for_family(family),
    )
}

fn cached_events_for_family(family: &OpenCodeSessionFamily) -> Result<Vec<SessionEvent>> {
    cached_family_events(
        &OPEN_CODE_TIMELINE_CACHE,
        "OpenCode timeline",
        family.root.id.clone(),
        family_cache_timestamp(family)?,
        || load_events_for_family(family),
    )
}

fn family_cache_timestamp(family: &OpenCodeSessionFamily) -> Result<i64> {
    Ok(opencode_db_timestamp()?.max(family_updated_at(family)))
}

fn count_family_records(member_ids: &[&str]) -> Result<(usize, usize)> {
    let connection = open_connection()?;
    let member_ids = member_ids
        .iter()
        .map(|id| (*id).to_string())
        .collect::<Vec<_>>();
    let placeholders = vec!["?"; member_ids.len()].join(",");

    let message_count: usize = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM message WHERE session_id IN ({placeholders})"),
            params_from_iter(member_ids.iter()),
            |row| row.get::<_, i64>(0).map(|count| count as usize),
        )
        .context("Failed to count OpenCode messages")?;

    // The part table stores its kind inside the data JSON (no `type` column);
    // mirror the timeline's classification: message/control kinds are not
    // events, missing kinds count as "unknown" events.
    let event_count: usize = connection
        .query_row(
        &format!(
            "SELECT COUNT(*) FROM part WHERE session_id IN ({placeholders}) AND COALESCE(json_extract(data, '$.type'), 'unknown') NOT IN ('text','reasoning','tool','patch','file','step-start','step-finish')"
        ),
            params_from_iter(member_ids.iter()),
            |row| row.get::<_, i64>(0).map(|count| count as usize),
        )
        .context("Failed to count OpenCode events")?;

    Ok((message_count, event_count))
}

fn family_member_ids(family: &OpenCodeSessionFamily) -> Vec<String> {
    family.members.iter().map(|row| row.id.clone()).collect()
}

fn load_message_rows(
    connection: &Connection,
    member_ids: &[String],
) -> Result<Vec<OpenCodeMessageRow>> {
    let placeholders = vec!["?"; member_ids.len()].join(",");
    let mut statement = connection.prepare(&format!(
        "SELECT id, session_id, time_created, data FROM message WHERE session_id IN ({placeholders}) ORDER BY time_created ASC, id ASC"
    ))?;
    let rows = statement.query_map(params_from_iter(member_ids.iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut message_rows = Vec::new();

    for row in rows {
        let (message_id, session_id, time_created, raw_data) = row?;
        let value: Value = serde_json::from_str(&raw_data)
            .with_context(|| format!("Invalid OpenCode message JSON for {message_id}"))?;
        message_rows.push(OpenCodeMessageRow {
            id: message_id,
            session_id,
            time_created,
            value,
        });
    }

    Ok(message_rows)
}

fn load_part_rows(connection: &Connection, member_ids: &[String]) -> Result<Vec<OpenCodePartRow>> {
    let placeholders = vec!["?"; member_ids.len()].join(",");
    let mut statement = connection.prepare(&format!(
        "SELECT id, session_id, message_id, time_created, data FROM part WHERE session_id IN ({placeholders}) ORDER BY time_created ASC, id ASC"
    ))?;
    let rows = statement.query_map(params_from_iter(member_ids.iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;

    let mut part_rows = Vec::new();

    for row in rows {
        let (part_id, session_id, message_id, time_created, raw_data) = row?;
        let value: Value = serde_json::from_str(&raw_data)
            .with_context(|| format!("Invalid OpenCode part JSON for {part_id}"))?;
        part_rows.push(OpenCodePartRow {
            id: part_id,
            session_id,
            message_id,
            time_created,
            value,
        });
    }

    Ok(part_rows)
}

fn group_parts_by_message_id(
    part_rows: Vec<OpenCodePartRow>,
) -> HashMap<String, Vec<OpenCodePartRow>> {
    let mut parts_by_message_id = HashMap::new();

    for part_row in part_rows {
        if let Some(message_id) = part_row.message_id.clone() {
            parts_by_message_id
                .entry(message_id)
                .or_insert_with(Vec::new)
                .push(part_row);
        }
    }

    parts_by_message_id
}

fn is_opencode_control_part(kind: &str) -> bool {
    matches!(kind, "step-start" | "step-finish")
}

fn is_opencode_message_part(kind: &str) -> bool {
    matches!(kind, "text" | "reasoning" | "tool" | "patch" | "file")
}

fn load_message_blocks(part_rows: Vec<OpenCodePartRow>) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();

    for part_row in part_rows {
        let value = part_row.value;
        let kind = super::json_string(&value, &["type"]).unwrap_or_else(|| "unknown".to_string());

        if is_opencode_control_part(&kind) {
            continue;
        }

        if kind == "tool" {
            let tool_blocks = tool_blocks(&value);
            if tool_blocks.is_empty() {
                blocks.push(super::empty_tool_block("OpenCode", &value));
            } else {
                blocks.extend(tool_blocks);
            }
            continue;
        }

        let normalized_kind = match kind.as_str() {
            "reasoning" => "thinking".to_string(),
            _ => kind.clone(),
        };

        let text = match kind.as_str() {
            "patch" => value.get("files").and_then(Value::as_array).map(|files| {
                let mut lines = vec!["变更文件：".to_string()];
                lines.extend(
                    files
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|file| format!("- {file}")),
                );
                lines.join("\n")
            }),
            "file" => {
                let filename = super::json_string(&value, &["filename"]);
                let mime = super::json_string(&value, &["mime"]);

                match (filename, mime) {
                    (Some(filename), Some(mime)) => Some(format!("文件：{filename}\n类型：{mime}")),
                    (Some(filename), None) => Some(format!("文件：{filename}")),
                    (None, Some(mime)) => Some(format!("文件类型：{mime}")),
                    (None, None) => None,
                }
            }
            _ => super::json_string(&value, &["text"]),
        };

        if text.is_none() && !is_opencode_message_part(&kind) {
            blocks.push(super::unsupported_block("OpenCode", &value));
        } else {
            blocks.push(ContentBlock {
                kind: normalized_kind,
                text,
                tool_name: super::json_string(&value, &["tool"]),
                tool_call_id: super::json_string(&value, &["callID"]),
                is_error: None,
                payload: Some(value),
            });
        }
    }

    blocks
}

fn load_messages_for_family(family: &OpenCodeSessionFamily) -> Result<Vec<SessionMessage>> {
    let connection = open_connection()?;
    let member_ids = family_member_ids(family);
    let mut parts_by_message_id =
        group_parts_by_message_id(load_part_rows(&connection, &member_ids)?);
    let message_rows = load_message_rows(&connection, &member_ids)?;
    let mut messages = family
        .members
        .iter()
        .filter(|row| row.id != family.root.id)
        .map(|row| session_marker_message(row, "subagent_started"))
        .collect::<Vec<_>>();

    for row in message_rows {
        let role =
            super::json_string(&row.value, &["role"]).unwrap_or_else(|| "unknown".to_string());
        let timestamp = row
            .value
            .get("time")
            .and_then(|time| time.get("created"))
            .and_then(Value::as_i64)
            .or(Some(row.time_created));
        let mut blocks =
            load_message_blocks(parts_by_message_id.remove(&row.id).unwrap_or_default());

        if role == "user" {
            blocks = super::sanitize_user_blocks(blocks);
        }

        if blocks.is_empty() {
            blocks.push(super::empty_message_block(
                "OpenCode",
                "message has no visible parts",
                Some(row.value.clone()),
            ));
        }

        messages.push(SessionMessage {
            id: row.id,
            role,
            timestamp,
            blocks,
            session_id: Some(row.session_id),
        });
    }

    messages.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(messages)
}

fn event_from_part_row(part_row: OpenCodePartRow) -> Option<SessionEvent> {
    let kind =
        super::json_string(&part_row.value, &["type"]).unwrap_or_else(|| "unknown".to_string());

    if is_opencode_message_part(&kind) || is_opencode_control_part(&kind) {
        return None;
    }

    Some(SessionEvent {
        id: part_row.id,
        kind: kind.clone(),
        timestamp: part_row
            .value
            .get("time")
            .and_then(|time| time.get("created"))
            .and_then(Value::as_i64)
            .or(Some(part_row.time_created)),
        summary: super::summarize_event(kind.as_str(), &part_row.value),
        payload: Some(part_row.value),
        session_id: Some(part_row.session_id),
    })
}

fn load_events_for_family(family: &OpenCodeSessionFamily) -> Result<Vec<SessionEvent>> {
    let connection = open_connection()?;
    let member_ids = family_member_ids(family);
    let mut events = family
        .members
        .iter()
        .filter(|row| row.id != family.root.id)
        .map(|row| session_marker_event(row, "subagent_started"))
        .collect::<Vec<_>>();

    events.extend(
        load_part_rows(&connection, &member_ids)?
            .into_iter()
            .filter_map(event_from_part_row),
    );

    events.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(events)
}

fn tool_blocks(value: &Value) -> Vec<ContentBlock> {
    let tool_name = super::json_string(value, &["tool"]);
    let tool_call_id = super::json_string(value, &["callID"]);
    let input = value.get("state").and_then(|state| state.get("input"));
    let output = value.get("state").and_then(|state| state.get("output"));
    let mut blocks = Vec::new();

    if input.is_some_and(|item| !item.is_null()) {
        let mut payload = value.clone();
        if let Some(state) = payload.get_mut("state").and_then(Value::as_object_mut) {
            state.remove("output");
        }

        blocks.push(ContentBlock {
            kind: "function_call".to_string(),
            text: tool_input_text(input),
            tool_name: tool_name.clone(),
            tool_call_id: tool_call_id.clone(),
            is_error: None,
            payload: Some(tool_payload(&payload, input.cloned(), None)),
        });
    }

    if output.is_some_and(|item| !item.is_null()) {
        let mut payload = value.clone();
        if let Some(state) = payload.get_mut("state").and_then(Value::as_object_mut) {
            state.remove("input");
        }

        blocks.push(ContentBlock {
            kind: "function_call_output".to_string(),
            text: tool_output_text(output),
            tool_name,
            tool_call_id,
            is_error: None,
            payload: Some(tool_payload(&payload, input.cloned(), output.cloned())),
        });
    }

    blocks
}

fn tool_payload(base: &Value, input: Option<Value>, output: Option<Value>) -> Value {
    let mut payload = base.clone();

    if let Some(object) = payload.as_object_mut() {
        if let Some(input) = input {
            object.insert("input".to_string(), input);
        }

        if let Some(output) = output {
            object.insert("output".to_string(), output);
        }
    }

    payload
}

fn tool_input_text(input: Option<&Value>) -> Option<String> {
    let Some(input) = input else {
        return None;
    };

    if let Some(text) = input.as_str() {
        return Some(text.to_string());
    }

    super::stringify_json(input)
}

fn tool_output_text(output: Option<&Value>) -> Option<String> {
    let Some(output) = output else {
        return None;
    };

    if let Some(text) = output.as_str() {
        return Some(text.to_string());
    }

    super::stringify_json(output)
}

fn session_marker_message(row: &OpenCodeSessionRow, kind: &str) -> SessionMessage {
    SessionMessage {
        id: format!("opencode-{}-{}", kind, row.id),
        role: "assistant".to_string(),
        timestamp: Some(row.time_created),
        blocks: vec![ContentBlock {
            kind: "output_text".to_string(),
            text: Some(format!("Sub-agent session: {}\n{}", row.title, row.id)),
            tool_name: None,
            tool_call_id: None,
            is_error: None,
            payload: Some(json!({
                "type": kind,
                "session_id": row.id,
                "title": row.title,
                "directory": row.directory,
                "parent_id": row.parent_id,
            })),
        }],
        session_id: Some(row.id.clone()),
    }
}

fn session_marker_event(row: &OpenCodeSessionRow, kind: &str) -> SessionEvent {
    SessionEvent {
        id: format!("opencode-{}-{}", kind, row.id),
        kind: kind.to_string(),
        timestamp: Some(row.time_created),
        summary: format!("Sub-agent session started: {}", row.title),
        payload: Some(json!({
            "session_id": row.id,
            "title": row.title,
            "directory": row.directory,
            "parent_id": row.parent_id,
        })),
        session_id: Some(row.id.clone()),
    }
}

fn write_session(detail: &SessionDetail, new_session_id: &str) -> Result<(String, Vec<String>)> {
    let sessions_before = list_session_rows().unwrap_or_default();
    let payload = import_payload(detail, new_session_id)?;
    let import_file = std::env::temp_dir().join(format!(
        "reins-opencode-import-{}.json",
        Uuid::new_v4().simple()
    ));

    if let Some(parent) = import_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let serialized = serde_json::to_string_pretty(&payload)?;
    fs::write(&import_file, serialized)
        .with_context(|| format!("Failed to write {}", import_file.display()))?;

    let output = Command::new("opencode")
        .arg("import")
        .arg(&import_file)
        .output()
        .context("Failed to execute opencode import")?;

    fs::remove_file(&import_file).ok();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "terminated by signal".to_string());
        bail!(
            "OpenCode import command failed. exit_code: {exit_code}\nstdout:\n{}\nstderr:\n{}",
            stdout.trim(),
            stderr.trim()
        );
    }

    let sessions_after = list_session_rows().unwrap_or_default();
    let created_session_id = resolve_imported_session_id(
        &output.stdout,
        &output.stderr,
        &sessions_before,
        &sessions_after,
        detail,
    )?;

    lock_timeline_cache()?.clear();
    *lock_family_index_cache()? = None;

    Ok((created_session_id, vec![db_path()?.display().to_string()]))
}

fn import_payload(detail: &SessionDetail, session_id: &str) -> Result<Value> {
    let created_at = detail
        .summary
        .created_at
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let updated_at = detail.summary.updated_at.unwrap_or(created_at);
    let cwd = crate::support::fs::effective_cwd(detail.summary.cwd.as_deref())?;
    let project_id = format!("project_{}", Uuid::new_v4().simple());
    let mut stats = OpenCodeImportStats::default();
    let mut messages = Vec::new();
    let mut last_user_message_id: Option<String> = None;
    let mut previous_message_id: Option<String> = None;

    for (message_index, message) in detail.messages.iter().enumerate() {
        let message_id = format!("msg_{}", Uuid::new_v4().simple());
        let message_timestamp = message
            .timestamp
            .unwrap_or(created_at + message_index as i64);
        let parent_message_id = if message.role == "user" {
            None
        } else {
            Some(
                last_user_message_id
                    .clone()
                    .or_else(|| previous_message_id.clone())
                    .unwrap_or_else(|| session_id.to_string()),
            )
        };
        let parts = message_parts(
            session_id,
            &message_id,
            message,
            message_timestamp,
            &mut stats,
        );

        if parts.is_empty() {
            continue;
        }

        messages.push(json!({
            "info": message_info(
                session_id,
                &message_id,
                message,
                message_timestamp,
                &cwd,
                parent_message_id.as_deref(),
            ),
            "parts": parts,
        }));

        if message.role == "user" {
            last_user_message_id = Some(message_id.clone());
        }

        previous_message_id = Some(message_id);
    }

    Ok(json!({
        "info": {
            "id": session_id,
            "slug": slugify_title(&detail.summary.title),
            "projectID": project_id,
            "directory": cwd,
            "title": detail.summary.title,
            "version": "1.4.3",
            "summary": {
                "additions": stats.additions,
                "deletions": stats.deletions,
                "files": stats.files,
            },
            "time": {
                "created": created_at,
                "updated": updated_at,
            }
        },
        "messages": messages,
    }))
}

fn message_info(
    session_id: &str,
    message_id: &str,
    message: &SessionMessage,
    timestamp: i64,
    cwd: &str,
    parent_message_id: Option<&str>,
) -> Value {
    match message.role.as_str() {
        "user" => json!({
            "id": message_id,
            "sessionID": session_id,
            "role": "user",
            "time": {
                "created": timestamp,
            },
            "agent": "build",
            "model": {
                "providerID": "imported",
                "modelID": "imported",
                "variant": "default",
            }
        }),
        _ => json!({
            "id": message_id,
            "sessionID": session_id,
            "parentID": parent_message_id.unwrap_or(session_id),
            "role": "assistant",
            "mode": "build",
            "agent": "build",
            "variant": "default",
            "path": {
                "cwd": cwd,
                "root": cwd,
            },
            "cost": 0,
            "tokens": {
                "total": 0,
                "input": 0,
                "output": 0,
                "reasoning": 0,
                "cache": {
                    "read": 0,
                    "write": 0,
                }
            },
            "modelID": "imported",
            "providerID": "imported",
            "time": {
                "created": timestamp,
                "completed": timestamp,
            },
            "finish": "stop",
        }),
    }
}

fn message_parts(
    session_id: &str,
    message_id: &str,
    message: &SessionMessage,
    timestamp: i64,
    stats: &mut OpenCodeImportStats,
) -> Vec<Value> {
    let mut parts = Vec::new();

    for block in &message.blocks {
        match block.kind.as_str() {
            "text" | "input_text" | "output_text" => {
                if let Some(text) = super::block_text(block) {
                    parts.push(json!({
                        "id": format!("prt_{}", Uuid::new_v4().simple()),
                        "sessionID": session_id,
                        "messageID": message_id,
                        "type": "text",
                        "text": text,
                        "time": {
                            "start": timestamp,
                            "end": timestamp,
                        }
                    }));
                }
            }
            "thinking" | "reasoning" => {
                if let Some(text) = super::block_text(block) {
                    parts.push(json!({
                        "id": format!("prt_{}", Uuid::new_v4().simple()),
                        "sessionID": session_id,
                        "messageID": message_id,
                        "type": "reasoning",
                        "text": text,
                        "time": {
                            "start": timestamp,
                            "end": timestamp,
                        }
                    }));
                }
            }
            "tool_use" | "function_call" => {
                let title = block
                    .tool_name
                    .clone()
                    .unwrap_or_else(|| "imported_tool".to_string());
                parts.push(json!({
                    "id": format!("prt_{}", Uuid::new_v4().simple()),
                    "sessionID": session_id,
                    "messageID": message_id,
                    "type": "tool",
                    "tool": title,
                    "callID": block.tool_call_id.clone().unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple())),
                    "state": {
                        "status": "running",
                        "input": tool_input(block),
                        "title": block.tool_name.clone().unwrap_or_else(|| "imported_tool".to_string()),
                        "metadata": {},
                        "time": {
                            "start": timestamp,
                        }
                    }
                }));
            }
            "tool_result" | "function_call_output" => {
                let title = block
                    .tool_name
                    .clone()
                    .unwrap_or_else(|| "imported_tool".to_string());
                parts.push(json!({
                    "id": format!("prt_{}", Uuid::new_v4().simple()),
                    "sessionID": session_id,
                    "messageID": message_id,
                    "type": "tool",
                    "tool": title,
                    "callID": block.tool_call_id.clone().unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple())),
                    "state": {
                        "status": "completed",
                        "input": tool_input(block),
                        "output": tool_output(block),
                        "title": block.tool_name.clone().unwrap_or_else(|| "imported_tool".to_string()),
                        "metadata": {},
                        "time": {
                            "start": timestamp,
                            "end": timestamp,
                        }
                    }
                }));
            }
            "patch" => {
                if let Some(files) = patch_files(block) {
                    stats.files += files.len();
                    parts.push(json!({
                        "id": format!("prt_{}", Uuid::new_v4().simple()),
                        "sessionID": session_id,
                        "messageID": message_id,
                        "type": "patch",
                        "hash": format!("patch_{}", Uuid::new_v4().simple()),
                        "files": files,
                    }));
                }
            }
            "file" => {
                let mut file_part = json!({
                    "id": format!("prt_{}", Uuid::new_v4().simple()),
                    "sessionID": session_id,
                    "messageID": message_id,
                    "type": "file",
                });

                if let Some(payload) = block.payload.as_ref() {
                    if let Some(filename) = super::json_string(payload, &["filename"]) {
                        file_part["filename"] = Value::String(filename);
                    }
                    if let Some(mime) = super::json_string(payload, &["mime"]) {
                        file_part["mime"] = Value::String(mime);
                    }
                    if let Some(url) = super::json_string(payload, &["url"]) {
                        file_part["url"] = Value::String(url);
                    }
                }

                parts.push(file_part);
            }
            _ => {}
        }
    }

    parts
}

fn tool_input(block: &ContentBlock) -> Value {
    if let Some(payload) = block.payload.as_ref() {
        if let Some(input) = payload.get("input") {
            return input.clone();
        }
    }

    block
        .text
        .as_deref()
        .map(|text| json!({ "raw": text }))
        .unwrap_or_else(|| json!({}))
}

fn tool_output(block: &ContentBlock) -> String {
    if let Some(payload) = block.payload.as_ref() {
        if let Some(output) = payload.get("output") {
            if let Some(text) = output.as_str() {
                return text.to_string();
            }

            return super::stringify_json(output).unwrap_or_default();
        }
    }

    block.text.clone().unwrap_or_default()
}

fn patch_files(block: &ContentBlock) -> Option<Vec<String>> {
    block.payload.as_ref().and_then(|payload| {
        payload
            .get("files")
            .and_then(Value::as_array)
            .map(|files| {
                files
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|files| !files.is_empty())
    })
}

fn parse_imported_session_id(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    );

    combined
        .lines()
        .find_map(|line| line.strip_prefix("Imported session: "))
        .map(|value| value.trim().to_string())
}

fn resolve_imported_session_id(
    stdout: &[u8],
    stderr: &[u8],
    sessions_before: &[OpenCodeSessionRow],
    sessions_after: &[OpenCodeSessionRow],
    detail: &SessionDetail,
) -> Result<String> {
    if let Some(session_id) = parse_imported_session_id(stdout, stderr) {
        return Ok(session_id);
    }

    let known_ids = sessions_before
        .iter()
        .map(|row| row.id.clone())
        .collect::<HashSet<_>>();
    let mut imported_candidates = sessions_after
        .iter()
        .filter(|row| !known_ids.contains(&row.id))
        .collect::<Vec<_>>();

    imported_candidates.sort_by(|left, right| right.time_updated.cmp(&left.time_updated));

    if let Some(candidate) = imported_candidates.into_iter().find(|row| {
        row.title == detail.summary.title
            || detail
                .summary
                .cwd
                .as_deref()
                .is_some_and(|cwd| row.directory == cwd)
    }) {
        return Ok(candidate.id.clone());
    }

    if let Some(candidate) = sessions_after.iter().max_by(|left, right| {
        left.time_updated
            .cmp(&right.time_updated)
            .then_with(|| left.time_created.cmp(&right.time_created))
    }) {
        if !known_ids.contains(&candidate.id) {
            return Ok(candidate.id.clone());
        }
    }

    let stdout_text = String::from_utf8_lossy(stdout);
    let stderr_text = String::from_utf8_lossy(stderr);
    bail!(
        "OpenCode import may have succeeded, but the created session id could not be determined.\nstdout:\n{}\nstderr:\n{}",
        stdout_text.trim(),
        stderr_text.trim()
    )
}

fn slugify_title(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for ch in title.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if ch.is_whitespace() || matches!(ch, '-' | '_' | '/' | '.') {
            Some('-')
        } else {
            None
        };

        match normalized {
            Some('-') if !previous_dash && !slug.is_empty() => {
                slug.push('-');
                previous_dash = true;
            }
            Some(value) if value != '-' => {
                slug.push(value);
                previous_dash = false;
            }
            _ => {}
        }
    }

    let trimmed = slug.trim_matches('-');

    if trimmed.is_empty() {
        return "imported-session".to_string();
    }

    trimmed.to_string()
}
