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
    // type 列：user/assistant 进消息时间线，其余（system/idle/synthetic/
    // compaction/agent-switched/model-switched）归入事件。
    kind: String,
    time_created: i64,
    value: Value,
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
    // v2 会话只存在于 SQLite，没有 transcript 文件；用 "db路径:id" 组合串
    // 作为该记录的稳定 key。整体不是真实路径，path_key 会走原样字符串分支，
    // 写入与查询两侧同经本函数，key 保持一致。
    let db = db_path().unwrap_or_else(|_| PathBuf::from("/tmp/opencode.db"));
    PathBuf::from(format!("{}:{}", db.display(), session_id))
}

pub(crate) fn delete_session(path: &Path) -> Result<()> {
    let family = session_family_for_path(path)?;
    let connection = open_connection()?;

    for member in &family.members {
        // v2 CLI 删除 root 会级联删掉整条 parent 链；已被级联删除的成员
        // 直接跳过，避免 not found 让整个删除流程报错。
        let exists: Option<i64> = connection
            .query_row(
                "SELECT 1 FROM session_v2 WHERE id = ?1",
                [&member.id],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .context("Failed to check OpenCode session existence")?;
        if exists.is_none() {
            continue;
        }

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
        "SELECT id, parent_id, directory, title, time_created, time_updated FROM session_v2 ORDER BY time_updated DESC",
    )?;
    // session_v2.title 允许 NULL（v1 时代 NOT NULL），空标题回退到 id，
    // 避免 family 标题渲染成空白。
    let rows = statement.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: Option<String> = row.get(3)?;
        Ok(OpenCodeSessionRow {
            title: title
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| id.clone()),
            id,
            parent_id: row.get(1)?,
            directory: row.get(2)?,
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

    // 与时间线加载同一条分类边界：user/assistant 行是消息，其余行是事件。
    let message_count: usize = connection
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM session_message WHERE session_id IN ({placeholders}) AND type IN ('user','assistant')"
            ),
            params_from_iter(member_ids.iter()),
            |row| row.get::<_, i64>(0).map(|count| count as usize),
        )
        .context("Failed to count OpenCode messages")?;

    let event_count: usize = connection
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM session_message WHERE session_id IN ({placeholders}) AND type NOT IN ('user','assistant')"
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
        "SELECT id, session_id, type, time_created, data FROM session_message WHERE session_id IN ({placeholders}) ORDER BY time_created ASC, id ASC"
    ))?;
    let rows = statement.query_map(params_from_iter(member_ids.iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;

    let mut message_rows = Vec::new();

    for row in rows {
        let (message_id, session_id, kind, time_created, raw_data) = row?;
        let value: Value = serde_json::from_str(&raw_data)
            .with_context(|| format!("Invalid OpenCode message JSON for {message_id}"))?;
        message_rows.push(OpenCodeMessageRow {
            id: message_id,
            session_id,
            kind,
            time_created,
            value,
        });
    }

    Ok(message_rows)
}

// 消息时间戳优先取 data.time.created，部分 type（如 compaction）的 data
// 可能没有 time，回退 time_created 列。
fn message_row_timestamp(row: &OpenCodeMessageRow) -> Option<i64> {
    row.value
        .get("time")
        .and_then(|time| time.get("created"))
        .and_then(Value::as_i64)
        .or(Some(row.time_created))
}

fn load_message_blocks(row: &OpenCodeMessageRow) -> Vec<ContentBlock> {
    if row.kind == "user" {
        return user_message_blocks(&row.value);
    }

    assistant_message_blocks(&row.value)
}

fn user_message_blocks(value: &Value) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();

    if let Some(text) = super::json_string(value, &["text"]) {
        // 附件的 base64 数据不进 payload，只保留文字说明
        blocks.push(ContentBlock {
            kind: "text".to_string(),
            text: Some(text),
            tool_name: None,
            tool_call_id: None,
            is_error: None,
            payload: None,
        });
    }

    if let Some(files) = value.get("files").and_then(Value::as_array) {
        for file in files {
            let filename = super::json_string(file, &["name"]);
            let mime = super::json_string(file, &["mime"]);
            let text = match (filename, mime) {
                (Some(filename), Some(mime)) => Some(format!("文件：{filename}\n类型：{mime}")),
                (Some(filename), None) => Some(format!("文件：{filename}")),
                (None, Some(mime)) => Some(format!("文件类型：{mime}")),
                (None, None) => None,
            };

            if let Some(text) = text {
                blocks.push(ContentBlock {
                    kind: "file".to_string(),
                    text: Some(text),
                    tool_name: None,
                    tool_call_id: None,
                    is_error: None,
                    payload: None,
                });
            }
        }
    }

    blocks
}

fn assistant_message_blocks(value: &Value) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();
    let Some(content) = value.get("content").and_then(Value::as_array) else {
        return blocks;
    };

    for item in content {
        let kind = super::json_string(item, &["type"]).unwrap_or_else(|| "unknown".to_string());

        if kind == "tool" {
            let tool_blocks = tool_blocks(item);
            if tool_blocks.is_empty() {
                blocks.push(super::empty_tool_block("OpenCode", item));
            } else {
                blocks.extend(tool_blocks);
            }
            continue;
        }

        let normalized_kind = match kind.as_str() {
            "reasoning" => "thinking".to_string(),
            _ => kind.clone(),
        };
        let text = super::json_string(item, &["text"]);

        if text.is_none() && normalized_kind != "text" && normalized_kind != "thinking" {
            blocks.push(super::unsupported_block("OpenCode", item));
        } else {
            blocks.push(ContentBlock {
                kind: normalized_kind,
                text,
                tool_name: None,
                tool_call_id: None,
                is_error: None,
                payload: Some(item.clone()),
            });
        }
    }

    blocks
}

fn load_messages_for_family(family: &OpenCodeSessionFamily) -> Result<Vec<SessionMessage>> {
    let connection = open_connection()?;
    let member_ids = family_member_ids(family);
    let message_rows = load_message_rows(&connection, &member_ids)?;
    let mut messages = family
        .members
        .iter()
        .filter(|row| row.id != family.root.id)
        .map(|row| session_marker_message(row, "subagent_started"))
        .collect::<Vec<_>>();

    for row in message_rows {
        if row.kind != "user" && row.kind != "assistant" {
            continue;
        }

        let mut blocks = load_message_blocks(&row);
        if row.kind == "user" {
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
            id: row.id.clone(),
            role: row.kind.clone(),
            timestamp: message_row_timestamp(&row),
            blocks,
            session_id: Some(row.session_id.clone()),
        });
    }

    messages.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(messages)
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
        load_message_rows(&connection, &member_ids)?
            .into_iter()
            .filter(|row| row.kind != "user" && row.kind != "assistant")
            .map(event_from_message_row),
    );

    events.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(events)
}

fn event_from_message_row(row: OpenCodeMessageRow) -> SessionEvent {
    let timestamp = message_row_timestamp(&row);
    SessionEvent {
        id: row.id,
        kind: row.kind.clone(),
        timestamp,
        summary: v2_event_summary(&row.kind, &row.value),
        payload: Some(row.value),
        session_id: Some(row.session_id),
    }
}

// 共享 summarize_event 只认 message/type/name 字段，v2 事件的可读字段因
// type 而异（compaction 的总结、idle 的 outcome、agent-switched 的新
// agent 名），先取这些字段生成摘要，取不到再退回通用逻辑。synthetic 的
// text 是 <system-reminder> 噪音，不作为摘要来源。
fn v2_event_summary(kind: &str, value: &Value) -> String {
    for key in ["description", "summary", "outcome", "agent"] {
        if let Some(text) = super::json_string(value, &[key]) {
            let normalized = super::normalize_title(text);
            if !normalized.is_empty() {
                return format!("{kind}: {normalized}");
            }
        }
    }

    super::summarize_event(kind, value)
}

// v2 tool 块：{type,id,name,state:{status,input,content,metadata},time}。
// id 就是调用 id（call_xxx，v1 叫 callID）；输出在 state.content 块数组里
//（v1 是 state.output 字符串）。输入/输出拆成两条 UI 块，与前端已有的
// function_call/function_call_output 分组契约保持一致。
fn tool_blocks(value: &Value) -> Vec<ContentBlock> {
    let tool_name = super::json_string(value, &["name"]);
    let tool_call_id = super::json_string(value, &["id"]);
    let state = value.get("state");
    let input = state.and_then(|state| state.get("input"));
    let output_text = state
        .and_then(|state| state.get("content"))
        .and_then(tool_content_text);
    let is_error = state.and_then(|state| state.get("status")).and_then(Value::as_str) == Some("error");
    let mut blocks = Vec::new();

    if input.is_some_and(|item| !item.is_null()) {
        let mut payload = value.clone();
        // 输出内容不重复放进输入块，避免 payload 成倍变大
        if let Some(state) = payload.get_mut("state").and_then(Value::as_object_mut) {
            state.remove("content");
        }
        if let Some(input) = input {
            if let Some(object) = payload.as_object_mut() {
                object.insert("input".to_string(), input.clone());
            }
        }

        blocks.push(ContentBlock {
            kind: "function_call".to_string(),
            text: tool_input_text(input),
            tool_name: tool_name.clone(),
            tool_call_id: tool_call_id.clone(),
            is_error: None,
            payload: Some(payload),
        });
    }

    if let Some(output_text) = output_text {
        let mut payload = value.clone();
        // 输出文本提到顶层，前端 resultOutputText 直接读 payload.output
        if let Some(object) = payload.as_object_mut() {
            object.insert("output".to_string(), Value::String(output_text.clone()));
            if let Some(input) = input {
                object.insert("input".to_string(), input.clone());
            }
        }

        blocks.push(ContentBlock {
            kind: "function_call_output".to_string(),
            text: Some(output_text),
            tool_name,
            tool_call_id,
            is_error: is_error.then_some(true),
            payload: Some(payload),
        });
    }

    blocks
}

// v2 tool 输出是 state.content 块数组（通常为 {type:"text",text}），
// 提取其中文本拼接；数组为空返回 None（running 中的工具）。
fn tool_content_text(content: &Value) -> Option<String> {
    let items = content.as_array()?;
    let texts = items
        .iter()
        .filter_map(|item| super::json_string(item, &["text"]))
        .collect::<Vec<_>>();

    if texts.is_empty() {
        // 非文本输出块降级为 JSON 展示
        return (!items.is_empty()).then(|| super::stringify_json(content)).flatten();
    }

    Some(texts.join("\n"))
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
        .arg("session")
        .arg("import")
        // standalone 起私有 server 完成导入，不依赖后台服务是否在运行
        //（HOME 被重定向或服务未启动时默认连接会超时失败）
        .arg("--standalone")
        .arg(&import_file)
        .output()
        .context("Failed to execute opencode session import")?;

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

// v2 导入 payload 对齐 `opencode session export` 的扁平结构：
// info + messages[]（type 区分 user/assistant，内容内嵌在消息里）。
// 消息 id 必须全新生成——session_message.id 全库唯一，沿用源 id 会撞
// UNIQUE 约束导致整个导入失败（真机验证过）。
fn import_payload(detail: &SessionDetail, session_id: &str) -> Result<Value> {
    let created_at = detail
        .summary
        .created_at
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let updated_at = detail.summary.updated_at.unwrap_or(created_at);
    let cwd = crate::support::fs::effective_cwd(detail.summary.cwd.as_deref())?;

    let mut messages = Vec::new();

    for (message_index, message) in detail.messages.iter().enumerate() {
        let message_timestamp = message
            .timestamp
            .unwrap_or(created_at + message_index as i64);

        if message.role == "user" {
            let Some(text) = user_import_text(message) else {
                continue;
            };

            messages.push(json!({
                "id": format!("msg_{}", Uuid::new_v4().simple()),
                "time": { "created": message_timestamp },
                "type": "user",
                "text": text,
                "files": [],
                "agents": [],
            }));
            continue;
        }

        let content = assistant_import_content(message, message_timestamp);
        if content.is_empty() {
            continue;
        }

        messages.push(json!({
            "id": format!("msg_{}", Uuid::new_v4().simple()),
            "time": { "created": message_timestamp },
            "type": "assistant",
            "agent": "build",
            "model": {
                "id": "imported",
                "providerID": "imported",
                "variant": "default",
            },
            "content": content,
            "finish": "stop",
            "cost": 0,
            "tokens": {
                "input": 0,
                "output": 0,
                "reasoning": 0,
                "cache": { "read": 0, "write": 0 },
            },
        }));
    }

    Ok(json!({
        "info": {
            "id": session_id,
            "projectID": import_project_id(&cwd),
            "title": detail.summary.title,
            "time": {
                "created": created_at,
                "updated": updated_at,
            },
            "location": { "directory": cwd },
            "cost": 0,
            "tokens": {
                "input": 0,
                "output": 0,
                "reasoning": 0,
                "cache": { "read": 0, "write": 0 },
            },
        },
        "messages": messages,
    }))
}

// 用户消息在 v2 只有单个 text 字段；取文本类块拼接。附件块保留文字说明
//（v2 files 需要 base64 数据，导入侧无法还原，files 置空）。
fn user_import_text(message: &SessionMessage) -> Option<String> {
    let parts = message
        .blocks
        .iter()
        .filter(|block| {
            matches!(
                block.kind.as_str(),
                "text" | "input_text" | "output_text" | "file"
            )
        })
        .filter_map(|block| super::block_text(block))
        .collect::<Vec<_>>();

    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

// 归一化块 → v2 content[]。同一消息内的 call+output 按 tool_call_id 合并
// 成一个 completed tool 块（v2 的 tool 本就是单块含输入输出）；跨消息的
// output 单独成块。patch/file 没有 v2 对应类型，降级为文本说明。
fn assistant_import_content(message: &SessionMessage, timestamp: i64) -> Vec<Value> {
    let mut content = Vec::new();
    let mut call_positions = HashMap::new();

    for block in &message.blocks {
        match block.kind.as_str() {
            "text" | "input_text" | "output_text" | "patch" | "file" => {
                let Some(text) = block_import_text(block) else {
                    continue;
                };
                content.push(json!({ "type": "text", "text": text }));
            }
            "thinking" | "reasoning" => {
                let Some(text) = super::block_text(block) else {
                    continue;
                };
                content.push(json!({ "type": "reasoning", "text": text }));
            }
            "tool_use" | "function_call" => {
                let call_id = import_tool_call_id(block);
                content.push(json!({
                    "type": "tool",
                    "id": call_id,
                    "name": block.tool_name.clone().unwrap_or_else(|| "imported_tool".to_string()),
                    "state": {
                        "status": "running",
                        "input": tool_input(block),
                        "content": [],
                        "metadata": {},
                    },
                    "time": { "created": timestamp },
                }));
                call_positions.insert(call_id, content.len() - 1);
            }
            "tool_result" | "function_call_output" => {
                let call_id = import_tool_call_id(block);
                let state = json!({
                    "status": "completed",
                    "input": tool_input(block),
                    "content": [ { "type": "text", "text": tool_output(block) } ],
                    "metadata": {},
                });

                if let Some(&position) = call_positions.get(&call_id) {
                    content[position]["state"] = state;
                } else {
                    content.push(json!({
                        "type": "tool",
                        "id": call_id,
                        "name": block.tool_name.clone().unwrap_or_else(|| "imported_tool".to_string()),
                        "state": state,
                        "time": { "created": timestamp },
                    }));
                }
            }
            _ => {}
        }
    }

    content
}

fn block_import_text(block: &ContentBlock) -> Option<String> {
    match block.kind.as_str() {
        "patch" => block.payload.as_ref().and_then(|payload| {
            let files = payload.get("files").and_then(Value::as_array)?;
            let mut lines = vec!["变更文件：".to_string()];
            lines.extend(
                files
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|file| format!("- {file}")),
            );
            Some(lines.join("\n"))
        }),
        "file" => {
            let payload = block.payload.as_ref();
            let filename = payload.and_then(|payload| super::json_string(payload, &["filename"]));
            let mime = payload.and_then(|payload| super::json_string(payload, &["mime"]));
            Some(match (filename, mime) {
                (Some(filename), Some(mime)) => format!("文件：{filename}\n类型：{mime}"),
                (Some(filename), None) => format!("文件：{filename}"),
                (None, Some(mime)) => format!("文件类型：{mime}"),
                (None, None) => return block.text.clone(),
            })
        }
        _ => super::block_text(block),
    }
}

fn import_tool_call_id(block: &ContentBlock) -> String {
    block
        .tool_call_id
        .clone()
        .unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()))
}

// v2 session 归属 project 表（worktree 即目录）；目录已有项目时复用其 id，
// 否则生成独立 id——导入接口不校验 project 行存在（真机验证），FK 也未开启。
fn import_project_id(cwd: &str) -> String {
    open_connection()
        .ok()
        .and_then(|connection| {
            connection
                .query_row(
                    "SELECT id FROM project WHERE worktree = ?1 ORDER BY time_created DESC LIMIT 1",
                    [cwd],
                    |row| row.get::<_, String>(0),
                )
                .ok()
        })
        .unwrap_or_else(|| format!("project_{}", Uuid::new_v4().simple()))
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
