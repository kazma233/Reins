use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;

use serde::Deserialize;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::{
    ContentBlock, SessionAgent, SessionDetail, SessionEvent, SessionEventPage, SessionExporter,
    SessionFileEntry, SessionMessage, SessionMessagePage, SessionOverview, SessionReader,
    SessionSummary, SourceApp, SummaryAccumulator, TimelineCacheEntry, TimelineRecord,
    family_index::{Family, FamilyIndexCacheEntry, FamilyRow},
    family_timeline::{FamilyAgentLabel, cached_family_events, cached_family_messages},
};

pub(crate) struct ClaudeCodeBackend;

pub(crate) static BACKEND: ClaudeCodeBackend = ClaudeCodeBackend;

#[derive(Clone, Debug)]
struct ClaudeSessionRow {
    path: PathBuf,
    summary: SessionSummary,
    agent_session_id: String,
    agent_label: Option<String>,
    is_root: bool,
}

type ClaudeSessionFamily = Family<ClaudeSessionRow>;
type ClaudeFamilyIndexCacheEntry = FamilyIndexCacheEntry<ClaudeSessionRow>;

impl FamilyRow for ClaudeSessionRow {
    fn member_path(&self) -> std::borrow::Cow<'_, Path> {
        std::borrow::Cow::Borrowed(&self.path)
    }

    fn family_root_id(&self) -> &str {
        &self.summary.source_session_id
    }

    fn member_id(&self) -> &str {
        &self.agent_session_id
    }

    fn member_created_at(&self) -> Option<i64> {
        self.summary.created_at
    }

    fn member_updated_at(&self) -> Option<i64> {
        self.summary.updated_at
    }

    // The transcript path spelling is the member tie-breaker, matching the
    // pre-engine ordering.
    fn member_tie_breaker(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Owned(self.path.display().to_string())
    }
}

#[derive(Clone)]
struct ClaudeSummaryCacheEntry {
    updated_at: i64,
    summary: SessionSummary,
}

#[derive(Deserialize)]
struct ClaudeAgentMeta {
    #[serde(rename = "agentType")]
    agent_type: Option<String>,
    description: Option<String>,
    name: Option<String>,
}

static CLAUDE_TIMELINE_CACHE: LazyLock<Mutex<HashMap<String, TimelineCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static CLAUDE_FAMILY_INDEX_CACHE: LazyLock<Mutex<Option<ClaudeFamilyIndexCacheEntry>>> =
    LazyLock::new(|| Mutex::new(None));
static CLAUDE_SUMMARY_CACHE: LazyLock<Mutex<HashMap<String, ClaudeSummaryCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn lock_timeline_cache()
-> Result<std::sync::MutexGuard<'static, HashMap<String, TimelineCacheEntry>>> {
    CLAUDE_TIMELINE_CACHE
        .lock()
        .map_err(|_| anyhow!("Claude timeline cache lock was poisoned"))
}

fn lock_family_index_cache()
-> Result<std::sync::MutexGuard<'static, Option<ClaudeFamilyIndexCacheEntry>>> {
    CLAUDE_FAMILY_INDEX_CACHE
        .lock()
        .map_err(|_| anyhow!("Claude family index cache lock was poisoned"))
}

fn lock_summary_cache()
-> Result<std::sync::MutexGuard<'static, HashMap<String, ClaudeSummaryCacheEntry>>> {
    CLAUDE_SUMMARY_CACHE
        .lock()
        .map_err(|_| anyhow!("Claude summary cache lock was poisoned"))
}

impl SessionReader for ClaudeCodeBackend {
    fn list_entries(&self) -> Result<Vec<SessionFileEntry>> {
        let mut entries = list_session_families()?
            .into_iter()
            .map(|family| -> Result<SessionFileEntry> {
                let sort_timestamp = family.updated_at();
                Ok(SessionFileEntry {
                    path: family.root.path.clone(),
                    sort_timestamp: sort_timestamp.unwrap_or_default(),
                    summary: Some(cached_family_summary(&family)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        super::sort_entries(&mut entries);
        Ok(entries)
    }

    fn clear_cache(&self) -> Result<()> {
        lock_timeline_cache()?.clear();
        lock_summary_cache()?.clear();
        *lock_family_index_cache()? = None;
        Ok(())
    }

    fn resolve_path(&self, source_session_id: &str) -> Result<PathBuf> {
        session_path_for_id(source_session_id)
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

impl SessionExporter for ClaudeCodeBackend {
    fn planned_import_paths(
        &self,
        summary: &SessionSummary,
        session_id: &str,
    ) -> Result<Vec<String>> {
        let cwd = crate::support::fs::effective_cwd(summary.cwd.as_deref())?;
        let path = root()?
            .join("projects")
            .join(project_slug(&cwd))
            .join(format!("{session_id}.jsonl"));
        Ok(vec![path.display().to_string()])
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
        .join(".claude"))
}

pub(crate) fn find_session_file(source_session_id: &str) -> Result<PathBuf> {
    crate::support::fs::find_session_file(&root()?.join("projects"), source_session_id)
}

pub(crate) fn delete_session(path: &Path) -> Result<()> {
    let family = session_family_for_path(path)?;
    let root_session_id = family.root.summary.source_session_id.clone();

    for member in &family.members {
        fs::remove_file(&member.path)
            .with_context(|| format!("Failed to delete {}", member.path.display()))?;

        if !member.is_root {
            let metadata_path = member.path.with_extension("meta.json");
            if metadata_path.exists() {
                fs::remove_file(&metadata_path)
                    .with_context(|| format!("Failed to delete {}", metadata_path.display()))?;
            }
        }
    }

    remove_dir_if_exists(&root()?.join("projects").join(&root_session_id))?;
    remove_dir_if_exists(&root()?.join("session-env").join(&root_session_id))?;
    remove_dir_if_exists(&root()?.join("file-history").join(&root_session_id))?;
    prune_empty_parents(root()?.join("projects"), path.parent());
    lock_timeline_cache()?.clear();
    lock_summary_cache()?.clear();
    *lock_family_index_cache()? = None;
    Ok(())
}

fn list_session_rows() -> Result<Vec<ClaudeSessionRow>> {
    let mut rows = Vec::new();

    for path in crate::support::fs::enumerate_jsonl_files(&root()?.join("projects"))? {
        if let Ok(row) = parse_session_index_row(&path) {
            rows.push(row);
        }
    }

    Ok(rows)
}

fn list_session_families() -> Result<Vec<ClaudeSessionFamily>> {
    Ok(family_index()?.index.families)
}

fn family_index() -> Result<ClaudeFamilyIndexCacheEntry> {
    let projects_root = root()?.join("projects");
    let source_key = projects_root.display().to_string();
    let updated_at = claude_projects_timestamp()?;

    if let Some(entry) = lock_family_index_cache()?
        .as_ref()
        .filter(|entry| entry.is_valid(&source_key, updated_at))
        .cloned()
    {
        return Ok(entry);
    }

    let entry = ClaudeFamilyIndexCacheEntry {
        source_key,
        updated_at,
        index: super::family_index::FamilyIndex::build_with_ids(build_session_families(
            list_session_rows()?,
        )),
    };

    *lock_family_index_cache()? = Some(entry.clone());

    Ok(entry)
}

// Member/family ordering and the id/path maps live in the shared engine; this
// only groups rows by the sessionId shared between a root transcript and its
// subagent transcripts, then picks each family root.
fn build_session_families(rows: Vec<ClaudeSessionRow>) -> Vec<ClaudeSessionFamily> {
    let by_id = rows
        .iter()
        .cloned()
        .map(|row| (row.summary.source_session_id.clone(), row))
        .collect::<HashMap<_, _>>();
    let mut grouped = HashMap::<String, Vec<ClaudeSessionRow>>::new();

    for row in rows {
        grouped
            .entry(row.summary.source_session_id.clone())
            .or_default()
            .push(row);
    }

    grouped
        .into_iter()
        .filter_map(|(root_id, members)| {
            let root = members
                .iter()
                .find(|row| row.is_root)
                .cloned()
                .or_else(|| {
                    by_id
                        .get(&root_id)
                        .cloned()
                        .or_else(|| members.first().cloned())
                })?;

            Some(ClaudeSessionFamily { root, members })
        })
        .collect()
}

fn session_family_for_path(path: &Path) -> Result<ClaudeSessionFamily> {
    let key = crate::support::fs::path_key(path);
    family_index()?
        .index
        .sessions_by_path
        .get(&key)
        .cloned()
        .ok_or_else(|| anyhow!("Could not find Claude Code session for {}", path.display()))
}

fn session_path_for_id(source_session_id: &str) -> Result<PathBuf> {
    if let Some(path) = family_index()?.index.path_for_id(source_session_id) {
        return Ok(path);
    }

    find_session_file(source_session_id)
}

fn parse_session_index_row(path: &Path) -> Result<ClaudeSessionRow> {
    let mut summary = SummaryAccumulator::default();
    let is_root = is_root_transcript(path);

    for line in BufReader::new(File::open(path)?).lines() {
        let value = super::parse_json_line(&line?)?;
        super::update_summary_timestamp(&mut summary, &value);

        if summary.session_id.is_none() {
            summary.session_id = super::json_string(&value, &["sessionId"]);
        }
    }

    let summary = super::build_summary(SourceApp::ClaudeCode, path, summary)?;
    let agent_session_id = if is_root {
        summary.source_session_id.clone()
    } else {
        subagent_session_id_from_path(path).unwrap_or_else(|| summary.source_session_id.clone())
    };
    let agent_label = if is_root {
        None
    } else {
        subagent_label_from_path(path)
    };

    Ok(ClaudeSessionRow {
        path: path.to_path_buf(),
        summary,
        agent_session_id,
        agent_label,
        is_root,
    })
}

fn parse_full_session_summary(path: &Path) -> Result<SessionSummary> {
    let mut summary = SummaryAccumulator::default();

    for line in BufReader::new(File::open(path)?).lines() {
        let value = super::parse_json_line(&line?)?;
        super::update_summary_timestamp(&mut summary, &value);

        if summary.session_id.is_none() {
            summary.session_id = super::json_string(&value, &["sessionId"]);
        }
        if summary.cwd.is_none() {
            summary.cwd = super::json_string(&value, &["cwd"]);
        }
        if summary.git_branch.is_none() {
            summary.git_branch = super::json_string(&value, &["gitBranch"]);
        }

        if super::json_type(&value) == Some("user") && summary.title.is_none() {
            summary.title = extract_title(value.get("message"));
        }
    }

    super::build_summary(SourceApp::ClaudeCode, path, summary)
}

fn is_root_transcript(path: &Path) -> bool {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        != Some("subagents")
}

fn subagent_session_id_from_path(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.strip_prefix("agent-").unwrap_or(stem).to_string())
}

fn subagent_label_from_path(path: &Path) -> Option<String> {
    let metadata_path = path.with_extension("meta.json");
    let metadata = fs::read_to_string(metadata_path).ok()?;
    let value = serde_json::from_str::<Value>(&metadata).ok()?;
    subagent_label_from_metadata(&value)
}

fn subagent_label_from_metadata(value: &Value) -> Option<String> {
    let metadata = serde_json::from_value::<ClaudeAgentMeta>(value.clone()).ok()?;
    metadata
        .description
        .or(metadata.agent_type)
        .or(metadata.name)
        .map(super::normalize_title)
        .filter(|label| !label.is_empty())
}

fn family_member_display_name(row: &ClaudeSessionRow) -> String {
    row.agent_label
        .clone()
        .or_else(|| {
            row.path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.strip_prefix("agent-").unwrap_or(stem).to_string())
        })
        .unwrap_or_else(|| row.summary.title.clone())
}

// is_root is a row flag, not an id comparison: non agent-* subagent files
// fall back to the family key as their member id, which an id comparison
// could not tell apart from the root.
fn family_agents(family: &ClaudeSessionFamily) -> Vec<SessionAgent> {
    super::family_timeline::family_agents(family, |row| {
        if row.is_root {
            FamilyAgentLabel::Root
        } else {
            FamilyAgentLabel::Child(family_member_display_name(row))
        }
    })
}

fn claude_path_timestamp(path: &Path) -> Result<i64> {
    let mut latest = crate::support::time::file_modified_timestamp_millis(path)?;

    if let Some(parent) = path.parent() {
        latest = latest.max(crate::support::time::file_modified_timestamp_millis(
            parent,
        )?);
    }

    Ok(latest)
}

fn claude_projects_timestamp() -> Result<i64> {
    let mut latest = 0;

    for path in crate::support::fs::enumerate_jsonl_files(&root()?.join("projects"))? {
        latest = latest.max(claude_path_timestamp(&path)?);

        if !is_root_transcript(&path) {
            let metadata_path = path.with_extension("meta.json");
            if metadata_path.exists() {
                latest = latest.max(claude_path_timestamp(&metadata_path)?);
            }
        }
    }

    Ok(latest)
}

fn family_timestamp(family: &ClaudeSessionFamily) -> Result<i64> {
    family.members.iter().try_fold(0, |latest, row| {
        let mut row_latest = crate::support::time::file_modified_timestamp_millis(&row.path)?;

        if !row.is_root {
            if let Some(parent) = row.path.parent() {
                row_latest = row_latest.max(crate::support::time::file_modified_timestamp_millis(
                    parent,
                )?);
            }
        }

        Ok(latest.max(row_latest))
    })
}

fn cached_messages_for_family(family: &ClaudeSessionFamily) -> Result<Vec<SessionMessage>> {
    cached_family_messages(
        &CLAUDE_TIMELINE_CACHE,
        "Claude timeline",
        family.root.summary.source_session_id.clone(),
        family_timestamp(family)?,
        || load_messages_for_family(family),
    )
}

fn cached_events_for_family(family: &ClaudeSessionFamily) -> Result<Vec<SessionEvent>> {
    cached_family_events(
        &CLAUDE_TIMELINE_CACHE,
        "Claude timeline",
        family.root.summary.source_session_id.clone(),
        family_timestamp(family)?,
        || load_events_for_family(family),
    )
}

fn subagent_marker_message(row: &ClaudeSessionRow) -> SessionMessage {
    SessionMessage {
        id: format!("claude-subagent-start-{}", row.agent_session_id),
        role: "assistant".to_string(),
        timestamp: row.summary.created_at,
        blocks: vec![ContentBlock {
            kind: "output_text".to_string(),
            text: Some(format!(
                "Sub-agent session: {}\n{}",
                family_member_display_name(row),
                row.agent_session_id
            )),
            tool_name: None,
            tool_call_id: None,
            is_error: None,
            payload: Some(json!({
                "type": "subagent_started",
                "session_id": row.agent_session_id,
                "title": family_member_display_name(row),
                "transcript_path": row.path.display().to_string(),
                "is_sidechain": true,
            })),
        }],
        session_id: Some(row.agent_session_id.clone()),
    }
}

fn subagent_marker_event(row: &ClaudeSessionRow) -> SessionEvent {
    SessionEvent {
        id: format!("claude-subagent-event-{}", row.agent_session_id),
        kind: "subagent_started".to_string(),
        timestamp: row.summary.created_at,
        summary: format!(
            "Sub-agent session started: {}",
            family_member_display_name(row)
        ),
        payload: Some(json!({
            "session_id": row.agent_session_id,
            "title": family_member_display_name(row),
            "transcript_path": row.path.display().to_string(),
            "is_sidechain": true,
        })),
        session_id: Some(row.agent_session_id.clone()),
    }
}

fn parse_summary(path: &Path) -> Result<SessionSummary> {
    let family = session_family_for_path(path)?;
    cached_family_summary(&family)
}

fn parse_overview(path: &Path) -> Result<SessionOverview> {
    let family = session_family_for_path(path)?;
    let summary = cached_family_summary(&family)?;
    let messages = cached_messages_for_family(&family)?;
    let events = cached_events_for_family(&family)?;

    Ok(SessionOverview {
        summary,
        source_paths: family.source_paths(),
        message_count: Some(messages.len()),
        event_count: Some(events.len()),
        agents: family_agents(&family),
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
    let (events, start, next_offset, total_count) =
        crate::support::paging::slice_page(&all_events, offset, limit);

    Ok(SessionEventPage {
        events,
        offset: start,
        limit,
        next_offset,
        total_count,
        has_more: next_offset.is_some(),
    })
}

fn parse_detail(path: &Path) -> Result<SessionDetail> {
    let family = session_family_for_path(path)?;
    let summary = cached_family_summary(&family)?;
    let messages = cached_messages_for_family(&family)?;
    let events = cached_events_for_family(&family)?;

    Ok(SessionDetail {
        summary,
        source_paths: family.source_paths(),
        messages,
        events,
    })
}

fn cached_path_summary(path: &Path) -> Result<SessionSummary> {
    let cache_key = path.display().to_string();
    let updated_at = crate::support::time::file_modified_timestamp_millis(path)?;

    if let Some(summary) = lock_summary_cache()?
        .get(&cache_key)
        .filter(|entry| entry.updated_at == updated_at)
        .map(|entry| entry.summary.clone())
    {
        return Ok(summary);
    }

    // 跨进程的持久缓存：冷启动时未变更的文件跳过整文件解析。
    if let Some(summary) = super::summary_cache::load(SourceApp::ClaudeCode, path, updated_at) {
        lock_summary_cache()?.insert(
            cache_key,
            ClaudeSummaryCacheEntry {
                updated_at,
                summary: summary.clone(),
            },
        );
        return Ok(summary);
    }

    let summary = parse_full_session_summary(path)?;
    lock_summary_cache()?.insert(
        cache_key,
        ClaudeSummaryCacheEntry {
            updated_at,
            summary: summary.clone(),
        },
    );
    super::summary_cache::store(SourceApp::ClaudeCode, path, updated_at, &summary);
    Ok(summary)
}

fn cached_family_summary(family: &ClaudeSessionFamily) -> Result<SessionSummary> {
    let mut summary = cached_path_summary(&family.root.path)?;
    family.apply_summary_aggregates(&mut summary);
    Ok(summary)
}

fn parse_timeline_record(index: usize, value: &Value, session_id: &str) -> Option<TimelineRecord> {
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(crate::support::time::parse_timestamp);

    match super::json_type(value) {
        Some("user") | Some("assistant") => {
            let message = &value["message"];
            let role = super::json_string(message, &["role"])
                .or_else(|| super::json_type(value).map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string());
            let mut blocks = parse_message_blocks(message.get("content"), &role);

            if role == "user" {
                blocks = super::sanitize_user_blocks(blocks);
            }

            if blocks.is_empty() {
                blocks.push(super::empty_message_block(
                    "Claude Code",
                    "content was empty after sanitization",
                    Some(message.clone()),
                ));
            }

            return Some(TimelineRecord::Message(SessionMessage {
                id: super::json_string(value, &["uuid"])
                    .unwrap_or_else(|| format!("claude-message-{index}")),
                role,
                timestamp,
                blocks,
                session_id: Some(session_id.to_string()),
            }));
        }
        _ => {
            let kind = super::json_type(value).unwrap_or("unknown").to_string();
            Some(TimelineRecord::Event(SessionEvent {
                id: format!("claude-event-{index}"),
                kind: kind.clone(),
                timestamp,
                summary: super::summarize_event(kind.as_str(), value),
                payload: Some(value.clone()),
                session_id: Some(session_id.to_string()),
            }))
        }
    }
}

fn load_messages(path: &Path, session_id: &str) -> Result<Vec<SessionMessage>> {
    let mut messages = Vec::new();

    for (index, line) in BufReader::new(File::open(path)?).lines().enumerate() {
        let value = super::parse_json_line(&line?)?;

        if let Some(TimelineRecord::Message(message)) =
            parse_timeline_record(index, &value, session_id)
        {
            messages.push(message);
        }
    }

    Ok(messages)
}

fn load_events(path: &Path, session_id: &str) -> Result<Vec<SessionEvent>> {
    let mut events = Vec::new();

    for (index, line) in BufReader::new(File::open(path)?).lines().enumerate() {
        let value = super::parse_json_line(&line?)?;

        if let Some(TimelineRecord::Event(event)) = parse_timeline_record(index, &value, session_id)
        {
            events.push(event);
        }
    }

    Ok(events)
}

fn load_messages_for_family(family: &ClaudeSessionFamily) -> Result<Vec<SessionMessage>> {
    let mut messages = Vec::new();

    for row in &family.members {
        if !row.is_root {
            messages.push(subagent_marker_message(row));
        }

        messages.extend(load_messages(&row.path, &row.agent_session_id)?);
    }

    messages.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(messages)
}

fn load_events_for_family(family: &ClaudeSessionFamily) -> Result<Vec<SessionEvent>> {
    let mut events = Vec::new();

    for row in &family.members {
        if !row.is_root {
            events.push(subagent_marker_event(row));
        }

        events.extend(load_events(&row.path, &row.agent_session_id)?);
    }

    events.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(events)
}

fn parse_message_blocks(content: Option<&Value>, role: &str) -> Vec<ContentBlock> {
    match content {
        Some(Value::String(text)) => {
            if role == "user" && super::is_transport_message(text) {
                return Vec::new();
            }

            vec![ContentBlock {
                kind: "text".to_string(),
                text: Some(text.clone()),
                tool_name: None,
                tool_call_id: None,
                is_error: None,
                payload: None,
            }]
        }
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                let kind =
                    super::json_string(item, &["type"]).unwrap_or_else(|| "unknown".to_string());
                let text = super::json_string(item, &["text"])
                    .or_else(|| super::json_string(item, &["thinking"]))
                    .or_else(|| super::json_string(item, &["content"]));

                Some(ContentBlock {
                    kind,
                    text,
                    tool_name: super::json_string(item, &["name"]),
                    tool_call_id: super::json_string(item, &["id"])
                        .or_else(|| super::json_string(item, &["tool_use_id"])),
                    is_error: item.get("is_error").and_then(Value::as_bool),
                    payload: Some(item.clone()),
                })
            })
            .collect(),
        value => vec![super::unsupported_content_block("Claude Code", value)],
    }
}

fn extract_title(message: Option<&Value>) -> Option<String> {
    let content = message.and_then(|value| value.get("content"))?;

    parse_message_blocks(Some(content), "user")
        .into_iter()
        .filter(|block| block.kind == "text")
        .filter_map(|block| block.text)
        .find_map(|text| super::title_candidate_from_text(&text))
}

pub(crate) fn write_session(
    detail: &SessionDetail,
    new_session_id: &str,
) -> Result<(String, Vec<String>)> {
    let cwd = crate::support::fs::effective_cwd(detail.summary.cwd.as_deref())?;
    let project_slug = project_slug(&cwd);
    let target_path = root()?
        .join("projects")
        .join(project_slug)
        .join(format!("{new_session_id}.jsonl"));
    let git_branch = detail.summary.git_branch.clone();
    let prompt_id = Uuid::new_v4().to_string();
    let mut previous_uuid: Option<String> = None;
    let mut first_record_uuid: Option<String> = None;
    let mut tool_call_sources: HashMap<String, String> = HashMap::new();
    let mut lines = Vec::new();

    for message in &detail.messages {
        if is_subagent_marker_message(message) {
            continue;
        }

        append_records(
            &mut lines,
            message,
            new_session_id,
            &cwd,
            git_branch.as_deref(),
            &prompt_id,
            &mut previous_uuid,
            &mut first_record_uuid,
            &mut tool_call_sources,
        )?;
    }

    if let Some(first_uuid) = first_record_uuid {
        lines.insert(
            0,
            json!({
              "type": "file-history-snapshot",
              "messageId": first_uuid,
              "snapshot": {
                "messageId": first_uuid,
                "trackedFileBackups": {},
                "timestamp": crate::support::time::iso_timestamp_utc(Utc::now())
              },
              "isSnapshotUpdate": false
            }),
        );
    }

    crate::support::fs::write_jsonl_file(&target_path, &lines)?;
    Ok((
        new_session_id.to_string(),
        vec![target_path.display().to_string()],
    ))
}

#[allow(clippy::too_many_arguments)]
fn append_records(
    lines: &mut Vec<Value>,
    message: &SessionMessage,
    session_id: &str,
    cwd: &str,
    git_branch: Option<&str>,
    prompt_id: &str,
    previous_uuid: &mut Option<String>,
    first_record_uuid: &mut Option<String>,
    tool_call_sources: &mut HashMap<String, String>,
) -> Result<()> {
    let timestamp = message
        .timestamp
        .map(crate::support::time::utc_timestamp_from_millis)
        .unwrap_or_else(|| crate::support::time::iso_timestamp_utc(Utc::now()));
    let assistant_message_id = Uuid::new_v4().simple().to_string();

    for block in &message.blocks {
        match message.role.as_str() {
            "user" => match block.kind.as_str() {
                "text" | "input_text" => {
                    let text = match super::block_text(block) {
                        Some(text) => text,
                        None => continue,
                    };
                    let record_uuid = Uuid::new_v4().to_string();
                    let record = user_text_record(
                        &record_uuid,
                        previous_uuid.as_deref(),
                        &timestamp,
                        session_id,
                        cwd,
                        git_branch,
                        prompt_id,
                        &text,
                    );
                    push_record(
                        lines,
                        record,
                        &record_uuid,
                        previous_uuid,
                        first_record_uuid,
                    );
                }
                "tool_result" | "function_call_output" => {
                    let record_uuid = Uuid::new_v4().to_string();
                    let call_id = block
                        .tool_call_id
                        .clone()
                        .unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()));
                    let tool_text = super::block_text(block).unwrap_or_default();
                    let source_tool_assistant_uuid = tool_call_sources.get(&call_id).cloned();
                    let record = tool_result_record(
                        &record_uuid,
                        previous_uuid.as_deref(),
                        &timestamp,
                        session_id,
                        cwd,
                        git_branch,
                        prompt_id,
                        &call_id,
                        &tool_text,
                        block.is_error.unwrap_or(false),
                        source_tool_assistant_uuid.as_deref(),
                    );
                    push_record(
                        lines,
                        record,
                        &record_uuid,
                        previous_uuid,
                        first_record_uuid,
                    );
                }
                _ => {}
            },
            "assistant" => match block.kind.as_str() {
                "thinking" | "text" | "output_text" => {
                    let record_uuid = Uuid::new_v4().to_string();
                    let assistant_block = assistant_text_block(block);
                    let record = assistant_record(
                        &record_uuid,
                        previous_uuid.as_deref(),
                        &timestamp,
                        session_id,
                        cwd,
                        git_branch,
                        &assistant_message_id,
                        assistant_block,
                        false,
                    );
                    push_record(
                        lines,
                        record,
                        &record_uuid,
                        previous_uuid,
                        first_record_uuid,
                    );
                }
                "tool_use" | "function_call" => {
                    let record_uuid = Uuid::new_v4().to_string();
                    let call_id = block
                        .tool_call_id
                        .clone()
                        .unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()));
                    let assistant_block = tool_use_block(block, &call_id);
                    let record = assistant_record(
                        &record_uuid,
                        previous_uuid.as_deref(),
                        &timestamp,
                        session_id,
                        cwd,
                        git_branch,
                        &assistant_message_id,
                        assistant_block,
                        true,
                    );
                    tool_call_sources.insert(call_id, record_uuid.clone());
                    push_record(
                        lines,
                        record,
                        &record_uuid,
                        previous_uuid,
                        first_record_uuid,
                    );
                }
                _ => {}
            },
            "tool" | "toolResult" => match block.kind.as_str() {
                "function_call" | "tool_use" => {
                    let record_uuid = Uuid::new_v4().to_string();
                    let call_id = block
                        .tool_call_id
                        .clone()
                        .unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()));
                    let assistant_block = tool_use_block(block, &call_id);
                    let record = assistant_record(
                        &record_uuid,
                        previous_uuid.as_deref(),
                        &timestamp,
                        session_id,
                        cwd,
                        git_branch,
                        &assistant_message_id,
                        assistant_block,
                        true,
                    );
                    tool_call_sources.insert(call_id, record_uuid.clone());
                    push_record(
                        lines,
                        record,
                        &record_uuid,
                        previous_uuid,
                        first_record_uuid,
                    );
                }
                "function_call_output" | "tool_result" => {
                    let record_uuid = Uuid::new_v4().to_string();
                    let call_id = block
                        .tool_call_id
                        .clone()
                        .unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()));
                    let tool_text = super::block_text(block).unwrap_or_default();
                    let source_tool_assistant_uuid = tool_call_sources.get(&call_id).cloned();
                    let record = tool_result_record(
                        &record_uuid,
                        previous_uuid.as_deref(),
                        &timestamp,
                        session_id,
                        cwd,
                        git_branch,
                        prompt_id,
                        &call_id,
                        &tool_text,
                        block.is_error.unwrap_or(false),
                        source_tool_assistant_uuid.as_deref(),
                    );
                    push_record(
                        lines,
                        record,
                        &record_uuid,
                        previous_uuid,
                        first_record_uuid,
                    );
                }
                _ => {}
            },
            _ => {}
        }
    }

    Ok(())
}

fn is_subagent_marker_message(message: &SessionMessage) -> bool {
    if message.role != "assistant" {
        return false;
    }

    message.blocks.iter().any(|block| {
        block
            .payload
            .as_ref()
            .and_then(|payload| payload.get("type"))
            .and_then(Value::as_str)
            == Some("subagent_started")
    })
}

fn push_record(
    lines: &mut Vec<Value>,
    record: Value,
    record_uuid: &str,
    previous_uuid: &mut Option<String>,
    first_record_uuid: &mut Option<String>,
) {
    if first_record_uuid.is_none() {
        *first_record_uuid = Some(record_uuid.to_string());
    }

    lines.push(record);
    *previous_uuid = Some(record_uuid.to_string());
}

fn user_text_record(
    record_uuid: &str,
    parent_uuid: Option<&str>,
    timestamp: &str,
    session_id: &str,
    cwd: &str,
    git_branch: Option<&str>,
    prompt_id: &str,
    text: &str,
) -> Value {
    let mut record = base_record(
        "user",
        record_uuid,
        parent_uuid,
        timestamp,
        session_id,
        cwd,
        git_branch,
    );
    record.insert("promptId".to_string(), Value::String(prompt_id.to_string()));
    record.insert(
        "message".to_string(),
        json!({
          "role": "user",
          "content": text
        }),
    );
    Value::Object(record)
}

fn tool_result_record(
    record_uuid: &str,
    parent_uuid: Option<&str>,
    timestamp: &str,
    session_id: &str,
    cwd: &str,
    git_branch: Option<&str>,
    prompt_id: &str,
    call_id: &str,
    content: &str,
    is_error: bool,
    source_tool_assistant_uuid: Option<&str>,
) -> Value {
    let mut record = base_record(
        "user",
        record_uuid,
        parent_uuid,
        timestamp,
        session_id,
        cwd,
        git_branch,
    );
    record.insert("promptId".to_string(), Value::String(prompt_id.to_string()));
    record.insert(
        "message".to_string(),
        json!({
          "role": "user",
          "content": [
            {
              "tool_use_id": call_id,
              "type": "tool_result",
              "content": content,
              "is_error": is_error
            }
          ]
        }),
    );
    record.insert(
        "toolUseResult".to_string(),
        json!({
          "type": "text",
          "content": content
        }),
    );

    if let Some(uuid) = source_tool_assistant_uuid {
        record.insert(
            "sourceToolAssistantUUID".to_string(),
            Value::String(uuid.to_string()),
        );
    }

    Value::Object(record)
}

fn assistant_record(
    record_uuid: &str,
    parent_uuid: Option<&str>,
    timestamp: &str,
    session_id: &str,
    cwd: &str,
    git_branch: Option<&str>,
    message_id: &str,
    block: Value,
    tool_use: bool,
) -> Value {
    let mut record = base_record(
        "assistant",
        record_uuid,
        parent_uuid,
        timestamp,
        session_id,
        cwd,
        git_branch,
    );
    record.insert(
        "message".to_string(),
        json!({
          "id": message_id,
          "type": "message",
          "role": "assistant",
          "content": [block],
          "model": "imported",
          "stop_reason": if tool_use { Value::String("tool_use".to_string()) } else { Value::Null },
          "stop_sequence": Value::Null,
          "service_tier": "imported"
        }),
    );
    Value::Object(record)
}

fn base_record(
    record_type: &str,
    record_uuid: &str,
    parent_uuid: Option<&str>,
    timestamp: &str,
    session_id: &str,
    cwd: &str,
    git_branch: Option<&str>,
) -> Map<String, Value> {
    let mut record = Map::new();
    record.insert(
        "parentUuid".to_string(),
        parent_uuid
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
    );
    record.insert("isSidechain".to_string(), Value::Bool(false));
    record.insert("type".to_string(), Value::String(record_type.to_string()));
    record.insert("uuid".to_string(), Value::String(record_uuid.to_string()));
    record.insert(
        "timestamp".to_string(),
        Value::String(timestamp.to_string()),
    );
    record.insert(
        "userType".to_string(),
        Value::String("external".to_string()),
    );
    record.insert("entrypoint".to_string(), Value::String("reins".to_string()));
    record.insert("cwd".to_string(), Value::String(cwd.to_string()));
    record.insert(
        "sessionId".to_string(),
        Value::String(session_id.to_string()),
    );
    record.insert(
        "version".to_string(),
        Value::String(super::IMPORTER_VERSION.to_string()),
    );

    if let Some(branch) = git_branch {
        record.insert("gitBranch".to_string(), Value::String(branch.to_string()));
    }

    record
}

fn assistant_text_block(block: &ContentBlock) -> Value {
    match block.kind.as_str() {
        "thinking" => json!({
          "type": "thinking",
          "thinking": super::block_text(block).unwrap_or_default()
        }),
        _ => json!({
          "type": "text",
          "text": super::block_text(block).unwrap_or_default()
        }),
    }
}

fn tool_use_block(block: &ContentBlock, call_id: &str) -> Value {
    json!({
      "type": "tool_use",
      "id": call_id,
      "name": block.tool_name.clone().unwrap_or_else(|| "imported_tool".to_string()),
      "input": tool_input(block)
    })
}

fn tool_input(block: &ContentBlock) -> Value {
    if let Some(payload) = block.payload.as_ref() {
        if let Some(input) = payload.get("input") {
            return input.clone();
        }

        if let Some(arguments) = payload.get("arguments") {
            if arguments.is_object() {
                return arguments.clone();
            }

            if let Some(text) = arguments.as_str() {
                if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                    if parsed.is_object() {
                        return parsed;
                    }
                }

                return json!({ "raw": text });
            }
        }
    }

    if let Some(text) = block.text.as_deref() {
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            if parsed.is_object() {
                return parsed;
            }
        }

        return json!({ "raw": text });
    }

    json!({})
}

fn project_slug(cwd: &str) -> String {
    cwd.replace('/', "-")
}

fn remove_dir_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("Failed to delete {}", path.display()))?;
    }

    Ok(())
}

fn prune_empty_parents(root: PathBuf, start: Option<&Path>) {
    let Some(mut current) = start.map(Path::to_path_buf) else {
        return;
    };

    while current.starts_with(&root) {
        let is_empty = fs::read_dir(&current)
            .ok()
            .and_then(|mut entries| entries.next())
            .is_none();

        if !is_empty {
            break;
        }

        if fs::remove_dir(&current).is_err() {
            break;
        }

        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
}
