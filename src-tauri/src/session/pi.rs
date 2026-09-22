use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    ContentBlock, SessionAgent, SessionEvent, SessionEventPage, SessionFileEntry, SessionMessage,
    SessionMessagePage, SessionOverview, SessionReader, SessionSummary, SourceApp,
    SummaryAccumulator, TimelineCacheEntry,
};

pub(crate) struct PiBackend;

pub(crate) static BACKEND: PiBackend = PiBackend;

#[derive(Clone)]
struct PiSummaryCacheEntry {
    updated_at: i64,
    summary: SessionSummary,
}

#[derive(Clone)]
struct PiIndexCacheEntry {
    source_key: String,
    fingerprint: String,
    entries: Vec<SessionFileEntry>,
}

#[derive(Clone)]
struct PiHeader {
    id: String,
    timestamp: String,
    cwd: String,
    parent_session: Option<String>,
    version: i64,
    value: Value,
}

#[derive(Clone)]
struct PiEntry {
    id: String,
    parent_id: Option<String>,
    timestamp: Option<i64>,
    value: Value,
}

static PI_TIMELINE_CACHE: LazyLock<Mutex<HashMap<String, TimelineCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static PI_INDEX_CACHE: LazyLock<Mutex<Option<PiIndexCacheEntry>>> =
    LazyLock::new(|| Mutex::new(None));
static PI_SUMMARY_CACHE: LazyLock<Mutex<HashMap<String, PiSummaryCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn lock_timeline_cache()
-> Result<std::sync::MutexGuard<'static, HashMap<String, TimelineCacheEntry>>> {
    PI_TIMELINE_CACHE
        .lock()
        .map_err(|_| anyhow!("Pi timeline cache lock was poisoned"))
}

fn lock_index_cache() -> Result<std::sync::MutexGuard<'static, Option<PiIndexCacheEntry>>> {
    PI_INDEX_CACHE
        .lock()
        .map_err(|_| anyhow!("Pi index cache lock was poisoned"))
}

fn lock_summary_cache()
-> Result<std::sync::MutexGuard<'static, HashMap<String, PiSummaryCacheEntry>>> {
    PI_SUMMARY_CACHE
        .lock()
        .map_err(|_| anyhow!("Pi summary cache lock was poisoned"))
}

impl SessionReader for PiBackend {
    fn list_entries(&self) -> Result<Vec<SessionFileEntry>> {
        Ok(index()?.entries)
    }

    fn clear_cache(&self) -> Result<()> {
        lock_timeline_cache()?.clear();
        lock_summary_cache()?.clear();
        *lock_index_cache()? = None;
        Ok(())
    }

    fn resolve_path(&self, source_session_id: &str) -> Result<PathBuf> {
        if let Some(path) = index()?
            .entries
            .into_iter()
            .find(|entry| {
                read_header(&entry.path)
                    .ok()
                    .is_some_and(|header| header.id == source_session_id)
            })
            .map(|entry| entry.path)
        {
            return Ok(path);
        }

        find_session_file(source_session_id)
    }

    fn parse_summary(&self, path: &Path) -> Result<SessionSummary> {
        cached_summary(path)
    }

    fn parse_overview(&self, path: &Path) -> Result<SessionOverview> {
        let summary = cached_summary(path)?;
        let (messages, events) = cached_timeline(path)?;
        let header = read_header(path)?;

        Ok(SessionOverview {
            summary,
            source_paths: vec![path.display().to_string()],
            message_count: Some(messages.len()),
            event_count: Some(events.len()),
            agents: vec![SessionAgent {
                session_id: header.id,
                label: "主 Agent".to_string(),
                is_root: true,
            }],
        })
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
        let (_, events) = cached_timeline(path)?;
        let (events, start, next_offset, total_count) =
            crate::support::paging::slice_page(&events, offset, limit);

        Ok(SessionEventPage {
            events,
            offset: start,
            limit,
            next_offset,
            total_count,
            has_more: next_offset.is_some(),
        })
    }
}

fn parse_messages_page(
    path: &Path,
    offset: usize,
    limit: usize,
) -> Result<SessionMessagePage> {
    let (messages, _) = cached_timeline(path)?;
    let (messages, start, next_offset, total_count) =
        crate::support::paging::slice_page(&messages, offset, limit);

    Ok(SessionMessagePage {
        messages,
        offset: start,
        limit,
        next_offset,
        total_count,
        has_more: next_offset.is_some(),
    })
}

pub(crate) fn root() -> Result<PathBuf> {
    if let Some(path) = env::var("PI_CODING_AGENT_DIR")
        .ok()
        .filter(|path| !path.is_empty())
    {
        return expand_configured_path(&path);
    }
    let home = crate::support::fs::user_home_dir().context("Unable to determine home directory")?;
    Ok(home.join(".pi").join("agent"))
}

// 官方契约(--session-dir > PI_CODING_AGENT_SESSION_DIR > settings.json.sessionDir):
// 显式配置的 sessionDir 就是最终会话目录;只有未配置时才用
// <agentDir>/sessions/<encoded-cwd>/ 默认布局。
pub(crate) fn sessions_root() -> Result<PathBuf> {
    match configured_session_dir()? {
        Some(dir) => Ok(dir),
        None => Ok(root()?.join("sessions")),
    }
}

fn configured_session_dir() -> Result<Option<PathBuf>> {
    if let Some(path) = env::var("PI_CODING_AGENT_SESSION_DIR")
        .ok()
        .filter(|path| !path.is_empty())
    {
        return Ok(Some(expand_configured_path(&path)?));
    }
    settings_session_dir()
}

fn settings_session_dir() -> Result<Option<PathBuf>> {
    let Ok(content) = fs::read_to_string(root()?.join("settings.json")) else {
        return Ok(None);
    };
    // settings.json 归 Pi 所有,损坏或字段缺失时回退默认目录,不阻断会话浏览。
    let Ok(settings) = serde_json::from_str::<Value>(&content) else {
        return Ok(None);
    };
    settings
        .get("sessionDir")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(expand_configured_path)
        .transpose()
}

fn expand_configured_path(value: &str) -> Result<PathBuf> {
    let home = crate::support::fs::user_home_dir().context("Unable to determine home directory")?;
    let cwd = std::env::current_dir().context("Unable to determine current directory")?;
    Ok(resolve_configured_path(value, &cwd, &home))
}

pub(crate) fn delete_session(path: &Path) -> Result<()> {
    let sessions_root_path = sessions_root()?;
    let canonical_sessions_root =
        fs::canonicalize(&sessions_root_path).unwrap_or(sessions_root_path);
    let target = fs::canonicalize(path)
        .with_context(|| format!("Failed to resolve Pi session {}", path.display()))?;
    if !target.starts_with(&canonical_sessions_root)
        || target.extension().and_then(|ext| ext.to_str()) != Some("jsonl")
    {
        bail!(
            "Refusing to delete a file outside the Pi sessions directory: {}",
            path.display()
        );
    }

    fs::remove_file(path).with_context(|| format!("Failed to delete {}", path.display()))?;
    prune_empty_parents(sessions_root()?, path.parent());
    BACKEND.clear_cache()
}

fn index() -> Result<PiIndexCacheEntry> {
    let sessions_root = sessions_root()?;
    let source_key = sessions_root.display().to_string();
    let files = enumerate_pi_files(&sessions_root)?;
    let fingerprint = sessions_fingerprint(&files)?;

    if let Some(entry) = lock_index_cache()?
        .as_ref()
        .filter(|entry| entry.source_key == source_key && entry.fingerprint == fingerprint)
        .cloned()
    {
        return Ok(entry);
    }

    let mut entries = files
        .into_iter()
        .filter_map(|path| {
            read_header(&path).ok()?;
            let sort_timestamp = file_mtime(&path).ok()?;
            Some(SessionFileEntry {
                path,
                sort_timestamp,
                summary: None,
            })
        })
        .collect::<Vec<_>>();
    super::sort_entries(&mut entries);

    let entry = PiIndexCacheEntry {
        source_key,
        fingerprint,
        entries,
    };
    *lock_index_cache()? = Some(entry.clone());
    Ok(entry)
}

fn enumerate_pi_files(root: &Path) -> Result<Vec<PathBuf>> {
    crate::support::fs::enumerate_jsonl_files(root)
}

fn sessions_fingerprint(files: &[PathBuf]) -> Result<String> {
    let mut parts = Vec::with_capacity(files.len());
    for path in files {
        let metadata = fs::metadata(path)
            .with_context(|| format!("Failed to read metadata for {}", path.display()))?;
        parts.push(format!(
            "{}:{}:{}",
            path.display(),
            file_mtime(path)?,
            metadata.len()
        ));
    }
    parts.sort();
    Ok(parts.join("\n"))
}

fn file_mtime(path: &Path) -> Result<i64> {
    crate::support::time::file_modified_timestamp_millis(path)
}

fn read_header(path: &Path) -> Result<PiHeader> {
    let line = BufReader::new(File::open(path)?)
        .lines()
        .next()
        .ok_or_else(|| anyhow!("Pi session has no header: {}", path.display()))??;
    let value = super::parse_json_line(&line)?;

    if super::json_type(&value) != Some("session") {
        bail!("Pi session header has invalid type: {}", path.display());
    }

    let id = super::json_string(&value, &["id"])
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow!("Pi session header has no id: {}", path.display()))?;
    let timestamp = super::json_string(&value, &["timestamp"])
        .filter(|timestamp| crate::support::time::parse_timestamp(timestamp).is_some())
        .ok_or_else(|| {
            anyhow!(
                "Pi session header has invalid timestamp: {}",
                path.display()
            )
        })?;
    let cwd = super::json_string(&value, &["cwd"]).unwrap_or_default();
    // 官方把缺失/非法的 version 当作 v1 触发迁移;非数字 version 视为已最新。
    let version = value
        .get("version")
        .map(|version| version.as_i64().unwrap_or(CURRENT_SESSION_VERSION))
        .unwrap_or(1);

    Ok(PiHeader {
        id,
        timestamp,
        cwd,
        parent_session: super::json_string(&value, &["parentSession"]),
        version,
        value,
    })
}

fn find_session_file(source_session_id: &str) -> Result<PathBuf> {
    enumerate_pi_files(&sessions_root()?)?
        .into_iter()
        .find(|path| {
            read_header(path)
                .ok()
                .is_some_and(|header| header.id == source_session_id)
        })
        .ok_or_else(|| anyhow!("Could not find Pi session file for {source_session_id}"))
}

fn cached_summary(path: &Path) -> Result<SessionSummary> {
    let key = crate::support::fs::path_key(path);
    let updated_at = file_mtime(path)?;

    if let Some(summary) = lock_summary_cache()?
        .get(&key)
        .filter(|entry| entry.updated_at == updated_at)
        .map(|entry| entry.summary.clone())
    {
        return Ok(summary);
    }

    if let Some(summary) = super::summary_cache::load(SourceApp::Pi, path, updated_at) {
        lock_summary_cache()?.insert(
            key,
            PiSummaryCacheEntry {
                updated_at,
                summary: summary.clone(),
            },
        );
        return Ok(summary);
    }

    let summary = parse_full_summary(path)?;
    lock_summary_cache()?.insert(
        key,
        PiSummaryCacheEntry {
            updated_at,
            summary: summary.clone(),
        },
    );
    super::summary_cache::store(SourceApp::Pi, path, updated_at, &summary);
    Ok(summary)
}

fn parse_full_summary(path: &Path) -> Result<SessionSummary> {
    let document = read_document(path)?;
    let (messages, _) = build_timeline(&document.header, &document.entries);
    let latest_session_name = document
        .entries
        .iter()
        .rev()
        .find(|entry| super::json_type(&entry.value) == Some("session_info"))
        .and_then(|entry| super::json_string(&entry.value, &["name"]))
        .filter(|name| !name.trim().is_empty());
    let title = latest_session_name
        .or_else(|| first_user_title(&messages))
        .unwrap_or_else(|| document.header.id.clone());

    let created_at = crate::support::time::parse_timestamp(&document.header.timestamp);
    let updated_at = file_mtime(path)?;
    let mut summary = SummaryAccumulator {
        session_id: Some(document.header.id.clone()),
        title: Some(super::normalize_title(title)),
        cwd: (!document.header.cwd.is_empty()).then_some(document.header.cwd.clone()),
        git_branch: None,
        created_at,
        updated_at: Some(updated_at),
    };

    // header 决定创建时间，文件 mtime 只负责失效判断，避免旧 entry 时间变化导致列表漂移。
    summary.created_at = created_at;
    super::build_summary(SourceApp::Pi, path, summary)
}

fn first_user_title(messages: &[SessionMessage]) -> Option<String> {
    messages
        .iter()
        .find(|message| message.role == "user")
        .and_then(|message| {
            message
                .blocks
                .iter()
                .filter_map(|block| block.text.as_deref())
                .find_map(super::title_candidate_from_text)
        })
}

struct PiDocument {
    header: PiHeader,
    entries: Vec<PiEntry>,
}

const CURRENT_SESSION_VERSION: i64 = 3;

fn read_document(path: &Path) -> Result<PiDocument> {
    let header = read_header(path)?;
    let file = File::open(path)?;
    let mut entries = Vec::new();

    for (index, line) in BufReader::new(file).lines().enumerate().skip(1) {
        let line = line?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = super::json_string(&value, &["id"]).unwrap_or_else(|| format!("pi-entry-{index}"));
        let parent_id = value
            .get("parentId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(crate::support::time::parse_timestamp);
        entries.push(PiEntry {
            id,
            parent_id,
            timestamp,
            value,
        });
    }

    if header.version < CURRENT_SESSION_VERSION {
        migrate_legacy_entries(header.version, &mut entries);
    }

    Ok(PiDocument { header, entries })
}

// 对齐官方 session-manager 的加载迁移;Reins 只读浏览,仅在内存迁移,不写回磁盘。
fn migrate_legacy_entries(version: i64, entries: &mut [PiEntry]) {
    if version < 2 {
        migrate_v1_entries(entries);
    }
    if version < 3 {
        migrate_v2_entries(entries);
    }
}

// v1 文件没有 id/parentId,按官方 migrateV1ToV2 补成线性链,
// 否则 build_timeline 从叶子回溯时整段历史只剩最后一条。
fn migrate_v1_entries(entries: &mut [PiEntry]) {
    let mut used_ids = HashSet::new();
    let mut previous_id = None;
    for entry in entries.iter_mut() {
        let id = new_entry_id(&mut used_ids);
        if let Some(object) = entry.value.as_object_mut() {
            object.insert("id".to_string(), json!(id.clone()));
            object.insert(
                "parentId".to_string(),
                previous_id
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
        }
        entry.id = id.clone();
        entry.parent_id = previous_id;
        previous_id = Some(id);
    }
    migrate_compaction_indices(entries);
}

// v1 的 compaction 用文件行索引(含 header 行,故减一)指向保留 entry,迁移成 entry id。
fn migrate_compaction_indices(entries: &mut [PiEntry]) {
    let fixes = entries
        .iter()
        .enumerate()
        .filter_map(|(position, entry)| {
            let object = entry.value.as_object()?;
            if object.get("type").and_then(Value::as_str) != Some("compaction") {
                return None;
            }
            let kept_index = object.get("firstKeptEntryIndex").and_then(Value::as_i64)?;
            let kept_id = kept_index
                .checked_sub(1)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| entries.get(index))
                .map(|target| target.id.clone());
            Some((position, kept_id))
        })
        .collect::<Vec<_>>();

    for (position, kept_id) in fixes {
        let Some(object) = entries[position].value.as_object_mut() else {
            continue;
        };
        object.remove("firstKeptEntryIndex");
        if let Some(kept_id) = kept_id {
            object.insert("firstKeptEntryId".to_string(), Value::String(kept_id));
        }
    }
}

// v2 → v3:hookMessage 角色改名 custom。
fn migrate_v2_entries(entries: &mut [PiEntry]) {
    for entry in entries.iter_mut() {
        let Some(message) = entry
            .value
            .get_mut("message")
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        if message.get("role").and_then(Value::as_str) == Some("hookMessage") {
            message.insert("role".to_string(), json!("custom"));
        }
    }
}

fn build_timeline(
    header: &PiHeader,
    entries: &[PiEntry],
) -> (Vec<SessionMessage>, Vec<SessionEvent>) {
    let by_id = entries
        .iter()
        .cloned()
        .map(|entry| (entry.id.clone(), entry))
        .collect::<HashMap<_, _>>();
    let Some(mut current) = entries.last().cloned() else {
        return (Vec::new(), parent_session_events(header));
    };
    let mut chain = Vec::new();
    let mut visited = HashSet::new();

    loop {
        if !visited.insert(current.id.clone()) {
            break;
        }
        chain.push(current.clone());

        let Some(parent_id) = current.parent_id.clone() else {
            break;
        };
        let Some(parent) = by_id.get(&parent_id) else {
            break;
        };
        current = parent.clone();
    }

    chain.reverse();
    let mut messages = Vec::new();
    let mut events = parent_session_events(header);

    for entry in chain {
        match super::json_type(&entry.value) {
            Some("message") => {
                messages.push(parse_message_entry(&entry, &header.id));
            }
            Some("custom_message") => {
                messages.push(parse_custom_message_entry(&entry, &header.id));
            }
            Some("compaction") => {
                events.push(parse_event(&entry, "compaction", &header.id));
                expand_compaction_messages(&entry, &header.id, &mut messages);
            }
            Some(kind) => events.push(parse_event(&entry, kind, &header.id)),
            None => events.push(parse_event(&entry, "unknown", &header.id)),
        }
    }

    (messages, events)
}

// 官方把 compaction entry 还原为 compactionSummary 消息 + retainedTail 中的保留消息;
// 这里是浏览器口径,全量展示,不过滤 stopReason 为 error/aborted/deferred 的助手消息。
fn expand_compaction_messages(
    entry: &PiEntry,
    session_id: &str,
    messages: &mut Vec<SessionMessage>,
) {
    messages.push(SessionMessage {
        id: format!("{}-summary", entry.id),
        role: "compactionSummary".to_string(),
        timestamp: entry.timestamp,
        blocks: vec![ContentBlock {
            kind: "compactionSummary".to_string(),
            text: super::json_string(&entry.value, &["summary"]),
            tool_name: None,
            tool_call_id: None,
            is_error: None,
            payload: Some(entry.value.clone()),
        }],
        session_id: Some(session_id.to_string()),
    });

    let Some(tail) = entry.value.get("retainedTail").and_then(Value::as_array) else {
        return;
    };
    for (index, message) in tail.iter().enumerate() {
        let tail_entry = PiEntry {
            id: format!("{}-tail-{index}", entry.id),
            parent_id: None,
            timestamp: message
                .get("timestamp")
                .and_then(Value::as_i64)
                .or(entry.timestamp),
            value: json!({
                "type": "message",
                "message": message,
            }),
        };
        messages.push(parse_message_entry(&tail_entry, session_id));
    }
}

fn parent_session_events(header: &PiHeader) -> Vec<SessionEvent> {
    header
        .parent_session
        .as_ref()
        .map(|parent_session| {
            vec![SessionEvent {
                id: "pi-parent-session".to_string(),
                kind: "parent_session".to_string(),
                timestamp: crate::support::time::parse_timestamp(&header.timestamp),
                summary: "Pi session has a parent session".to_string(),
                payload: Some(json!({
                    "type": "session",
                    "id": header.id,
                    "parentSession": parent_session,
                    "header": header.value,
                })),
                session_id: Some(header.id.clone()),
            }]
        })
        .unwrap_or_default()
}

fn parse_event(entry: &PiEntry, kind: &str, session_id: &str) -> SessionEvent {
    SessionEvent {
        id: entry.id.clone(),
        kind: kind.to_string(),
        timestamp: entry.timestamp,
        summary: super::summarize_event(kind, &entry.value),
        payload: Some(entry.value.clone()),
        session_id: Some(session_id.to_string()),
    }
}

fn parse_message_entry(entry: &PiEntry, session_id: &str) -> SessionMessage {
    let Some(message) = entry.value.get("message") else {
        return SessionMessage {
            id: entry.id.clone(),
            role: "unknown".to_string(),
            timestamp: entry.timestamp,
            blocks: vec![super::unsupported_content_block("Pi", Some(&entry.value))],
            session_id: Some(session_id.to_string()),
        };
    };
    let role = super::json_string(message, &["role"]).unwrap_or_else(|| "unknown".to_string());
    let mut blocks = match role.as_str() {
        "bashExecution" => vec![ContentBlock {
            kind: "bash_execution".to_string(),
            text: Some(
                [
                    super::json_string(message, &["command"]),
                    super::json_string(message, &["output"]),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("\n"),
            ),
            tool_name: Some("bash".to_string()),
            tool_call_id: None,
            is_error: None,
            payload: Some(message.clone()),
        }],
        "branchSummary" | "compactionSummary" => vec![ContentBlock {
            kind: role.clone(),
            text: super::json_string(message, &["summary"]),
            tool_name: None,
            tool_call_id: None,
            is_error: None,
            payload: Some(message.clone()),
        }],
        "user" | "assistant" | "toolResult" | "custom" => {
            parse_message_content(&role, message.get("content"))
        }
        _ => vec![super::unsupported_content_block("Pi", Some(message))],
    };

    if role == "toolResult" {
        let is_error = message.get("isError").and_then(Value::as_bool);
        // subagent 扩展(pi --no-session)把子代理完整对话内嵌在 toolResult.details 里,
        // 这是官方留给扩展的元数据通道;存在时用结构化块替代普通 tool_result 块。
        if super::json_string(message, &["toolName"]).as_deref() == Some("subagent") {
            if let Some(block) = parse_subagent_run_block(&entry.id, message, session_id) {
                blocks = vec![block];
            }
        }
        for block in &mut blocks {
            block.tool_name = block
                .tool_name
                .clone()
                .or_else(|| super::json_string(message, &["toolName"]));
            block.tool_call_id = block
                .tool_call_id
                .clone()
                .or_else(|| super::json_string(message, &["toolCallId"]));
            block.is_error = block.is_error.or(is_error);
        }
    }

    if role == "user" {
        blocks = sanitize_pi_user_blocks(blocks);
    }

    if blocks.is_empty() {
        blocks.push(super::empty_message_block(
            "Pi",
            "content was empty after sanitization",
            Some(message.clone()),
        ));
    }

    SessionMessage {
        id: entry.id.clone(),
        role,
        timestamp: message
            .get("timestamp")
            .and_then(Value::as_i64)
            .or(entry.timestamp),
        blocks,
        session_id: Some(session_id.to_string()),
    }
}

// 子代理运行结果的结构化块：details.results[].messages 是扩展捕获的标准
// AgentMessage 线性流,复用 parse_message_entry 解析成嵌套消息;原始 messages
// 数组不进 payload,由解析结果替代,其余 run 元数据原样保留。
fn parse_subagent_run_block(
    entry_id: &str,
    message: &Value,
    session_id: &str,
) -> Option<ContentBlock> {
    let details = message.get("details")?;
    let results = details.get("results")?.as_array()?;
    if results.is_empty() {
        return None;
    }

    let runs = results
        .iter()
        .enumerate()
        .map(|(run_index, run)| {
            let mut run = run.clone();
            let nested_messages = run
                .get("messages")
                .and_then(Value::as_array)
                .map(|messages| {
                    messages
                        .iter()
                        .enumerate()
                        .map(|(message_index, nested_message)| {
                            let nested_entry = PiEntry {
                                id: format!("{entry_id}-r{run_index}-m{message_index}"),
                                parent_id: None,
                                timestamp: None,
                                value: json!({ "type": "message", "message": nested_message }),
                            };
                            parse_message_entry(&nested_entry, session_id)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if let Some(object) = run.as_object_mut() {
                object.remove("messages");
                object.insert(
                    "nestedMessages".to_string(),
                    serde_json::to_value(nested_messages)
                        .expect("SessionMessage 序列化不会失败"),
                );
            }
            run
        })
        .collect::<Vec<_>>();

    let report = parse_message_content("toolResult", message.get("content"))
        .into_iter()
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("\n");

    Some(ContentBlock {
        kind: "subagent_run".to_string(),
        text: (!report.is_empty()).then_some(report),
        tool_name: Some("subagent".to_string()),
        tool_call_id: super::json_string(message, &["toolCallId"]),
        is_error: None,
        payload: Some(json!({
            "runs": runs,
        })),
    })
}

fn parse_custom_message_entry(entry: &PiEntry, session_id: &str) -> SessionMessage {
    let content = entry.value.get("content");
    let mut blocks = parse_message_content("custom", content);
    if blocks.is_empty() {
        blocks.push(super::empty_message_block(
            "Pi",
            "custom message content was empty",
            Some(entry.value.clone()),
        ));
    }

    SessionMessage {
        id: entry.id.clone(),
        role: "custom".to_string(),
        timestamp: entry.timestamp,
        blocks,
        session_id: Some(session_id.to_string()),
    }
}

fn parse_message_content(role: &str, content: Option<&Value>) -> Vec<ContentBlock> {
    match content {
        Some(Value::String(text)) => vec![ContentBlock {
            kind: if role == "assistant" {
                "output_text".to_string()
            } else if role == "toolResult" {
                "tool_result".to_string()
            } else {
                "text".to_string()
            },
            text: Some(text.clone()),
            tool_name: None,
            tool_call_id: None,
            is_error: None,
            payload: None,
        }],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| parse_content_block(role, item))
            .collect(),
        Some(value) => vec![super::unsupported_content_block("Pi", Some(value))],
        None => Vec::new(),
    }
}

fn parse_content_block(role: &str, value: &Value) -> Option<ContentBlock> {
    let raw_kind = super::json_string(value, &["type"]).unwrap_or_else(|| "unknown".to_string());
    let kind = match (role, raw_kind.as_str()) {
        ("assistant", "text") => "output_text",
        ("toolResult", "text") => "tool_result",
        (_, "text") => "text",
        (_, "toolCall") => "tool_use",
        (_, "toolResult") => "tool_result",
        (_, "image") => "image",
        _ => raw_kind.as_str(),
    }
    .to_string();
    let text = super::json_string(value, &["text"])
        .or_else(|| super::json_string(value, &["thinking"]))
        .or_else(|| super::json_string(value, &["content"]));
    if role == "assistant"
        && raw_kind == "text"
        && text
            .as_deref()
            .is_some_and(is_pi_zero_width_text_placeholder)
    {
        return None;
    }
    let kind = if matches!(
        kind.as_str(),
        "text" | "output_text" | "thinking" | "image" | "tool_use" | "tool_result"
    ) {
        kind
    } else {
        "unsupported_block".to_string()
    };

    Some(ContentBlock {
        kind,
        text,
        tool_name: super::json_string(value, &["name"])
            .or_else(|| super::json_string(value, &["toolName"])),
        tool_call_id: super::json_string(value, &["id"])
            .or_else(|| super::json_string(value, &["toolCallId"])),
        is_error: None,
        payload: Some(value.clone()),
    })
}

fn is_pi_zero_width_text_placeholder(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|character| character == '\u{200B}')
}

fn sanitize_pi_user_blocks(blocks: Vec<ContentBlock>) -> Vec<ContentBlock> {
    blocks
        .into_iter()
        .filter_map(|mut block| {
            block.text = block
                .text
                .take()
                .and_then(|text| super::sanitize_user_message_text(&text));
            (block.text.is_some()
                || matches!(
                    block.kind.as_str(),
                    "image"
                        | "tool_use"
                        | "tool_result"
                        | "function_call"
                        | "function_call_output"
                        | "unsupported_block"
                ))
            .then_some(block)
        })
        .collect()
}

fn cached_timeline(path: &Path) -> Result<(Vec<SessionMessage>, Vec<SessionEvent>)> {
    let key = crate::support::fs::path_key(path);
    let updated_at = file_mtime(path)?;

    {
        let cache = lock_timeline_cache()?;
        let cached = cache
            .get(&key)
            .filter(|entry| entry.updated_at == updated_at)
            .and_then(|entry| {
                let (Some(messages), Some(events)) = (&entry.messages, &entry.events) else {
                    return None;
                };
                Some((messages.clone(), events.clone()))
            });
        if let Some((messages, events)) = cached {
            return Ok((messages, events));
        }
    }

    let document = read_document(path)?;
    let (messages, events) = build_timeline(&document.header, &document.entries);
    super::family_timeline::store_timeline_items(
        &PI_TIMELINE_CACHE,
        "Pi timeline",
        key,
        updated_at,
        |entry| {
            entry.messages = Some(messages.clone());
            entry.events = Some(events.clone());
        },
    )?;
    Ok((messages, events))
}

fn resolve_configured_path(value: &str, cwd: &Path, home: &Path) -> PathBuf {
    let expanded = if value == "~" {
        home.to_path_buf()
    } else if let Some(relative) = value.strip_prefix("~/") {
        home.join(relative)
    } else {
        PathBuf::from(value)
    };

    if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(expanded)
    }
}

fn new_entry_id(used_ids: &mut HashSet<String>) -> String {
    loop {
        let id = Uuid::new_v4().simple().to_string()[..8].to_string();
        if used_ids.insert(id.clone()) {
            return id;
        }
    }
}

fn prune_empty_parents(root: PathBuf, start: Option<&Path>) {
    let Some(mut current) = start.map(Path::to_path_buf) else {
        return;
    };

    while current.starts_with(&root) && current != root {
        let is_empty = fs::read_dir(&current)
            .ok()
            .and_then(|mut entries| entries.next())
            .is_none();
        if !is_empty || fs::remove_dir(&current).is_err() {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
}
