use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, params_from_iter};
use serde_json::{Value, json};

use super::family_index::{Family, FamilyRow};
use super::family_timeline::FamilyAgentLabel;
use super::reader_engine::{
    FamilyReader, FamilySpec, Freshness, MarkerShape, MemberTimeline, OverviewCounts, SummaryKind,
};
use super::{
    ContentBlock, SessionEvent, SessionMessage, SessionSummary, SessionTokenUsage, SourceApp,
    UsageHourBuckets, usage_stats::SqlUsageHours,
};

#[derive(Clone, Debug)]
pub(crate) struct ZcodeSessionRow {
    id: String,
    parent_id: Option<String>,
    directory: String,
    title: String,
    time_created: i64,
    time_updated: i64,
    // 一次 GROUP BY 查询按 session_id 预聚合,避免列表逐会话全表扫。
    token_usage: Option<SessionTokenUsage>,
    // "db路径:id" 组合串:list_rows 时按实例 db 生成,作为该记录的稳定
    // 展示 key(行来自 SQLite,没有真实转录文件)。
    path: PathBuf,
}

type ZcodeSessionFamily = Family<ZcodeSessionRow>;

impl FamilyRow for ZcodeSessionRow {
    fn member_path(&self) -> std::borrow::Cow<'_, Path> {
        std::borrow::Cow::Borrowed(&self.path)
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
struct ZcodeMessageRow {
    id: String,
    session_id: String,
    // data.role 列:user/assistant。
    role: String,
    // data.semantics.kind 是 zcode 官方的消息语义标记:user_prompt /
    // assistant_response 进消息时间线,todo_reminder / timeline_event 等归事件。
    kind: String,
    time_created: i64,
    value: Value,
}

#[derive(Clone)]
pub(crate) struct ZcodeSpec;

impl FamilySpec for ZcodeSpec {
    type Row = ZcodeSessionRow;

    fn app(&self) -> SourceApp {
        SourceApp::Zcode
    }

    fn label(&self) -> &'static str {
        "ZCode"
    }

    // root() 就是 db 文件本身:可用性以 db 为准,扫描根与其重合。
    fn scan_root(&self, root: &Path) -> PathBuf {
        root.to_path_buf()
    }

    fn list_rows(&self, scan_root: &Path) -> Result<Vec<ZcodeSessionRow>> {
        list_session_rows(scan_root)
    }

    fn group_families(&self, rows: Vec<ZcodeSessionRow>) -> Result<Vec<ZcodeSessionFamily>> {
        Ok(build_session_families(rows))
    }

    // 索引失效只看 db 文件 mtime:全部行都从它 join 出来,行内时间不参与。
    fn index_freshness(&self, scan_root: &Path) -> Result<Freshness> {
        zcode_db_timestamp_at(scan_root).map(Freshness::Stamp)
    }

    fn summary_kind(&self) -> SummaryKind {
        SummaryKind::Rows
    }

    // 原始行摘要;(+N) 标题、transcript_path、族级时间戳与 usage 求和由
    // 引擎统一聚合。cwd/git_branch 引擎不动,在这里给。
    fn summary_from_rows(&self, root: &ZcodeSessionRow) -> SessionSummary {
        SessionSummary {
            source_app: SourceApp::Zcode,
            source_session_id: root.id.clone(),
            title: root.title.clone(),
            cwd: Some(root.directory.clone()),
            git_branch: None,
            transcript_path: root.member_path().display().to_string(),
            created_at: Some(root.time_created),
            updated_at: Some(root.time_updated),
            token_usage: None,
        }
    }

    // usage 已在索引扫描时按 session_id 预聚合,family 总量交给引擎默认求和。
    fn row_usage(&self, row: &ZcodeSessionRow) -> Option<SessionTokenUsage> {
        row.token_usage
    }

    // 时间线失效:max(db mtime, family updated_at)。
    fn family_freshness(&self, scan_root: &Path, family: &ZcodeSessionFamily) -> Result<i64> {
        Ok(zcode_db_timestamp_at(scan_root)?.max(family.updated_at().unwrap_or_default()))
    }

    // message/part 两表一次查询装配双半,替代原先 messages/events 各查一遍。
    fn load_members(
        &self,
        scan_root: &Path,
        members: &[ZcodeSessionRow],
    ) -> Result<Vec<MemberTimeline>> {
        let connection = open_connection_at(scan_root)?;
        let member_ids = members.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let message_rows = load_message_rows(&connection, &member_ids)?;
        let parts_by_message = load_parts_by_message(&connection, &member_ids)?;

        let mut messages_by_session = HashMap::<String, Vec<SessionMessage>>::new();
        let mut events_by_session = HashMap::<String, Vec<SessionEvent>>::new();

        for row in message_rows {
            if row.kind == "user_prompt" || row.kind == "assistant_response" {
                let parts = parts_by_message.get(&row.id).cloned().unwrap_or_default();
                let mut blocks = load_message_blocks(&row, &parts);
                if row.kind == "user_prompt" {
                    blocks = super::sanitize_user_blocks(blocks);
                }

                if blocks.is_empty() {
                    blocks.push(super::empty_message_block(
                        "ZCode",
                        "message has no visible parts",
                        Some(row.value.clone()),
                    ));
                }

                messages_by_session
                    .entry(row.session_id.clone())
                    .or_default()
                    .push(SessionMessage {
                        id: row.id.clone(),
                        role: row.role.clone(),
                        timestamp: message_row_timestamp(&row),
                        blocks,
                        session_id: Some(row.session_id.clone()),
                    });
            } else {
                let parts = parts_by_message.get(&row.id).cloned().unwrap_or_default();
                events_by_session
                    .entry(row.session_id.clone())
                    .or_default()
                    .push(event_from_message_row(row, &parts));
            }
        }

        Ok(members
            .iter()
            .map(|row| MemberTimeline {
                messages: Arc::new(messages_by_session.remove(&row.id).unwrap_or_default()),
                events: Arc::new(events_by_session.remove(&row.id).unwrap_or_default()),
            })
            .collect())
    }

    fn agent_name(&self, row: &ZcodeSessionRow) -> String {
        row.title.clone()
    }

    // marker id 沿用历史拼写 "_started"(引擎默认是 "-start-");payload 差异
    // 字段 directory/parent_id 全放 extras,基底由引擎组装。
    fn marker(&self, member_id: &str, row: &ZcodeSessionRow) -> MarkerShape {
        let extras = json!({
            "directory": row.directory,
            "parent_id": row.parent_id,
        });

        MarkerShape {
            message_id: format!("zcode-subagent_started-{member_id}"),
            event_id: format!("zcode-subagent_started-{member_id}"),
            message_extras: extras.clone(),
            event_extras: extras,
        }
    }

    fn overview_counts(&self) -> OverviewCounts {
        OverviewCounts::Declared
    }

    fn count_family_records(
        &self,
        scan_root: &Path,
        family: &ZcodeSessionFamily,
    ) -> Result<(usize, usize)> {
        let member_ids: Vec<&str> = family.members.iter().map(|row| row.id.as_str()).collect();
        count_family_records(scan_root, &member_ids)
    }

    fn agent_label(&self, family: &ZcodeSessionFamily, row: &ZcodeSessionRow) -> FamilyAgentLabel {
        if row.id == family.root.id {
            FamilyAgentLabel::Root
        } else {
            FamilyAgentLabel::Child(row.title.clone())
        }
    }

    // overview.source_paths:db 文件打头,成员行由它 join 出来。
    fn family_source_paths(&self, scan_root: &Path, family: &ZcodeSessionFamily) -> Vec<String> {
        let mut paths = vec![scan_root.display().to_string()];

        paths.extend(family.source_paths());
        paths
    }
}

pub(crate) static BACKEND: FamilyReader<ZcodeSpec> = FamilyReader::new(ZcodeSpec, root);

// 测试直接按根构造引擎实例:完全脱离进程 env 与全局锁,可并行。
#[cfg(test)]
pub(crate) fn engine_at(
    root: PathBuf,
    store_dir: PathBuf,
) -> super::reader_engine::ReaderEngine<ZcodeSpec> {
    super::reader_engine::ReaderEngine::new(ZcodeSpec, root, store_dir)
}

// 可用性以 db 文件本身为准而不是 ~/.zcode:目录存在但 db 缺失时,只读打开会
// 失败,让 detect 的 available=false 而不是让整个来源检测报错。
pub(crate) fn root() -> Result<PathBuf> {
    db_path()
}

// 模块级 env 版 db 路径仅供生产路径(usage)使用;reader 引擎实例的数据
// 查询一律以实例 scan_root(即 db 文件路径)为准,env 与实例互不串扰。
pub(crate) fn db_path() -> Result<PathBuf> {
    Ok(crate::support::fs::user_home_dir()
        .context("Unable to determine home directory")?
        .join(".zcode")
        .join("cli")
        .join("db")
        .join("db.sqlite"))
}

// db 属主是常驻的 ZCode 进程,只读打开避免与其写入争锁;WAL 模式支持并发读。
fn open_connection() -> Result<Connection> {
    Connection::open_with_flags(
        db_path()?,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("Failed to open ZCode sqlite database")
}

fn open_connection_at(scan_root: &Path) -> Result<Connection> {
    Connection::open_with_flags(
        scan_root,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("Failed to open ZCode sqlite database")
}

fn zcode_db_timestamp_at(scan_root: &Path) -> Result<i64> {
    crate::support::time::file_modified_timestamp_millis(scan_root)
}

fn list_session_rows(scan_root: &Path) -> Result<Vec<ZcodeSessionRow>> {
    let connection = open_connection_at(scan_root)?;
    let token_usages = session_token_usages(&connection)?;
    let mut statement = connection.prepare(
        "SELECT id, parent_id, directory, title, time_created, time_updated FROM session ORDER BY time_updated DESC",
    )?;
    let rows = statement.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: Option<String> = row.get(3)?;
        let token_usage = token_usages.get(&id).copied();
        // zcode 会话只存在于 SQLite,没有 transcript 文件;用 "db路径:id"
        // 组合串作为该记录的稳定 key。整体不是真实路径,path_key 会走
        // 原样字符串分支,写入与查询两侧同源,key 保持一致。
        let path = PathBuf::from(format!("{}:{}", scan_root.display(), id));
        Ok(ZcodeSessionRow {
            title: title
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| id.clone()),
            id,
            parent_id: row.get(1)?,
            directory: row.get(2)?,
            time_created: row.get(4)?,
            time_updated: row.get(5)?,
            token_usage,
            path,
        })
    })?;

    rows.collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to read ZCode sessions")
}

// turn_usage 表按 turn 预聚合了四类用量列,直接 SUM。其 input_tokens 是全口径
// (未命中 + 缓存读):model_usage.raw_usage_json 实测 inputTokens = provider 的
// input_tokens + cache_read_input_tokens,computed_total = input + output 同为全
// 口径。这里拆掉 cache_read,对齐 SessionTokenUsage 的 input 口径(不含缓存命中)。
fn session_token_usages(connection: &Connection) -> Result<HashMap<String, SessionTokenUsage>> {
    let mut statement = connection
        .prepare(
            "SELECT session_id,
                    COALESCE(SUM(input_tokens), 0)
                        - COALESCE(SUM(cache_read_input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0)
                        + COALESCE(SUM(reasoning_tokens), 0),
                    COALESCE(SUM(cache_read_input_tokens), 0),
                    COALESCE(SUM(cache_creation_input_tokens), 0)
             FROM turn_usage
             GROUP BY session_id",
        )
        .context("Failed to prepare ZCode token usage query")?;

    let rows = statement
        .query_map([], |row| {
            let session_id: String = row.get(0)?;
            let usage_column = |index: usize| -> rusqlite::Result<u64> {
                Ok(row.get::<_, Option<i64>>(index)?.unwrap_or_default().max(0) as u64)
            };
            Ok((
                session_id,
                SessionTokenUsage {
                    input_tokens: usage_column(1)?,
                    output_tokens: usage_column(2)?,
                    cache_read_tokens: usage_column(3)?,
                    cache_write_tokens: usage_column(4)?,
                },
            ))
        })
        .context("Failed to read ZCode token usages")?;

    rows.collect::<std::result::Result<HashMap<_, _>, _>>()
        .context("Failed to read ZCode token usages")
}

// 用量曲线的小时桶:turn_usage 每 turn 预聚合且自带 started_at,归一口径与
// session_token_usages 一致(input 拆掉 cache_read,output 并入 reasoning)。
// db 缺失表示来源不可用,返回 None。
pub(crate) fn usage_hours() -> Result<Option<SqlUsageHours>> {
    let connection = match open_connection() {
        Ok(connection) => connection,
        Err(_) => return Ok(None),
    };

    let mut statement = connection
        .prepare(
            "SELECT session_id, started_at, input_tokens, output_tokens, reasoning_tokens,
                    cache_read_input_tokens, cache_creation_input_tokens
             FROM turn_usage",
        )
        .context("Failed to prepare Zcode usage hours query")?;

    let rows = statement
        .query_map([], |row| {
            let session_id: String = row.get(0)?;
            let started_at: i64 = row.get(1)?;
            let column = |index: usize| -> rusqlite::Result<i64> {
                Ok(row.get::<_, Option<i64>>(index)?.unwrap_or_default().max(0))
            };
            Ok((
                session_id,
                started_at,
                SessionTokenUsage {
                    input_tokens: (column(2)? - column(5)?).max(0) as u64,
                    output_tokens: (column(3)? + column(4)?) as u64,
                    cache_read_tokens: column(5)? as u64,
                    cache_write_tokens: column(6)? as u64,
                },
            ))
        })
        .context("Failed to read Zcode usage hours")?;

    let today = super::usage_stats::today_prefix();
    let mut buckets = UsageHourBuckets::new();
    let mut sessions = std::collections::HashSet::new();
    let mut today_sessions = std::collections::HashSet::new();

    for row in rows {
        let (session_id, started_at, usage) = row?;
        let hour_key = super::hour_key(started_at);
        if let Some(key) = &hour_key {
            if super::usage_stats::today_hour(key, &today).is_some() {
                today_sessions.insert(session_id.clone());
            }
        }
        sessions.insert(session_id);
        super::merge_usage_bucket(&mut buckets, hour_key, usage);
    }

    Ok(Some(SqlUsageHours {
        buckets,
        session_count: sessions.len(),
        today_session_count: today_sessions.len(),
    }))
}

// Member/family ordering and the path map live in the shared engine; this
// only groups rows along the SQL parent_id chain and picks each family root.
fn build_session_families(rows: Vec<ZcodeSessionRow>) -> Vec<ZcodeSessionFamily> {
    let by_id = rows
        .iter()
        .cloned()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    let mut grouped = HashMap::<String, Vec<ZcodeSessionRow>>::new();

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

            Some(ZcodeSessionFamily { root, members })
        })
        .collect()
}

fn root_session_id(row: &ZcodeSessionRow, by_id: &HashMap<String, ZcodeSessionRow>) -> String {
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

fn count_family_records(scan_root: &Path, member_ids: &[&str]) -> Result<(usize, usize)> {
    let connection = open_connection_at(scan_root)?;
    let member_ids = member_ids
        .iter()
        .map(|id| (*id).to_string())
        .collect::<Vec<_>>();
    let placeholders = vec!["?"; member_ids.len()].join(",");

    // 与时间线加载同一条分类边界:semantics.kind 是 zcode 官方的消息语义标记。
    let message_count: usize = connection
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM message WHERE session_id IN ({placeholders}) AND json_extract(data, '$.semantics.kind') IN ('user_prompt', 'assistant_response')"
            ),
            params_from_iter(member_ids.iter()),
            |row| row.get::<_, i64>(0).map(|count| count as usize),
        )
        .context("Failed to count ZCode messages")?;

    let event_count: usize = connection
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM message WHERE session_id IN ({placeholders}) AND (json_extract(data, '$.semantics.kind') IS NULL OR json_extract(data, '$.semantics.kind') NOT IN ('user_prompt', 'assistant_response'))"
            ),
            params_from_iter(member_ids.iter()),
            |row| row.get::<_, i64>(0).map(|count| count as usize),
        )
        .context("Failed to count ZCode events")?;

    Ok((message_count, event_count))
}

fn load_message_rows(
    connection: &Connection,
    member_ids: &[String],
) -> Result<Vec<ZcodeMessageRow>> {
    let placeholders = vec!["?"; member_ids.len()].join(",");
    let mut statement = connection.prepare(&format!(
        "SELECT id, session_id, time_created, data FROM message WHERE session_id IN ({placeholders}) ORDER BY time_created ASC, sequence ASC, id ASC"
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
            .with_context(|| format!("Invalid ZCode message JSON for {message_id}"))?;
        message_rows.push(ZcodeMessageRow {
            id: message_id,
            session_id,
            role: super::json_string(&value, &["role"]).unwrap_or_else(|| "unknown".to_string()),
            kind: super::json_string(&value, &["semantics", "kind"])
                .unwrap_or_else(|| "unknown".to_string()),
            time_created,
            value,
        });
    }

    Ok(message_rows)
}

// parts 按 message_id 分组;sequence 是 zcode 写入时的自增序,决定同一条消息
// 内内容块的展示顺序。
fn load_parts_by_message(
    connection: &Connection,
    member_ids: &[String],
) -> Result<HashMap<String, Vec<Value>>> {
    let placeholders = vec!["?"; member_ids.len()].join(",");
    let mut statement = connection.prepare(&format!(
        "SELECT message_id, data FROM part WHERE session_id IN ({placeholders}) ORDER BY message_id, sequence, time_created, id"
    ))?;
    let rows = statement.query_map(params_from_iter(member_ids.iter()), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut parts_by_message = HashMap::<String, Vec<Value>>::new();

    for row in rows {
        let (message_id, raw_data) = row?;
        let value: Value = serde_json::from_str(&raw_data)
            .with_context(|| format!("Invalid ZCode part JSON for {message_id}"))?;
        parts_by_message.entry(message_id).or_default().push(value);
    }

    Ok(parts_by_message)
}

// 消息时间戳优先取 data.time.created,部分系统消息(如 timeline_event)的
// data 可能没有 time,回退 time_created 列。
fn message_row_timestamp(row: &ZcodeMessageRow) -> Option<i64> {
    row.value
        .get("time")
        .and_then(|time| time.get("created"))
        .and_then(Value::as_i64)
        .or(Some(row.time_created))
}

fn load_message_blocks(row: &ZcodeMessageRow, parts: &[Value]) -> Vec<ContentBlock> {
    if row.kind == "user_prompt" {
        return user_message_blocks(parts);
    }

    assistant_message_blocks(parts)
}

fn user_message_blocks(parts: &[Value]) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();

    for part in parts {
        match super::json_string(part, &["type"]).as_deref() {
            Some("text") => {
                if let Some(text) = super::json_string(part, &["text"]) {
                    blocks.push(ContentBlock {
                        kind: "text".to_string(),
                        text: Some(text),
                        tool_name: None,
                        tool_call_id: None,
                        is_error: None,
                        payload: None,
                    });
                }
            }
            // file part 只有 mime 与 artifact URL,URL 无法脱离 zcode 进程解析,
            // 展示 mime 即可。
            Some("file") => {
                let mime = super::json_string(part, &["mime"]);
                if let Some(text) = mime.map(|mime| format!("文件类型:{mime}")) {
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
            _ => {}
        }
    }

    blocks
}

fn assistant_message_blocks(parts: &[Value]) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();

    for part in parts {
        match super::json_string(part, &["type"]).as_deref() {
            Some("tool") => {
                let tool_blocks = tool_blocks(part);
                if tool_blocks.is_empty() {
                    blocks.push(super::empty_tool_block("ZCode", part));
                } else {
                    blocks.extend(tool_blocks);
                }
            }
            Some("reasoning") => {
                let text = super::json_string(part, &["text"]);
                blocks.push(ContentBlock {
                    kind: "thinking".to_string(),
                    text,
                    tool_name: None,
                    tool_call_id: None,
                    is_error: None,
                    payload: Some(part.clone()),
                });
            }
            Some("text") => {
                let text = super::json_string(part, &["text"]);
                blocks.push(ContentBlock {
                    kind: "text".to_string(),
                    text,
                    tool_name: None,
                    tool_call_id: None,
                    is_error: None,
                    payload: Some(part.clone()),
                });
            }
            // step-start/step-finish 是每步的边界与统计标记(空对象或纯计数),
            // 量大且无展示价值,不进时间线。
            Some("step-start") | Some("step-finish") => {}
            _ => blocks.push(super::unsupported_block("ZCode", part)),
        }
    }

    blocks
}

// zcode tool part:{type,callID,tool,state:{status,input,output}};input 是参数
// 对象或字符串,output 是字符串。与 OpenCode v2 的 tool 块同构,输入/输出
// 拆成两条 UI 块,保持与 function_call/function_call_output 的前端契约一致。
fn tool_blocks(value: &Value) -> Vec<ContentBlock> {
    let tool_name = super::json_string(value, &["tool"]);
    let tool_call_id = super::json_string(value, &["callID"]);
    let state = value.get("state");
    let input = state.and_then(|state| state.get("input"));
    let output_text = state
        .and_then(|state| state.get("output"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let is_error = state
        .and_then(|state| state.get("status"))
        .and_then(Value::as_str)
        == Some("error");
    let mut blocks = Vec::new();

    if input.is_some_and(|item| !item.is_null()) {
        let mut payload = value.clone();
        // 输出内容不重复放进输入块，避免 payload 成倍变大
        if let Some(state) = payload.get_mut("state").and_then(Value::as_object_mut) {
            state.remove("output");
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

fn tool_input_text(input: Option<&Value>) -> Option<String> {
    let Some(input) = input else {
        return None;
    };

    if let Some(text) = input.as_str() {
        return Some(text.to_string());
    }

    super::stringify_json(input)
}

// 非 timeline 语义的消息(todo_reminder、timeline_event 等)转事件;正文不
// 在 message.data 里而在 part 表,summary 与 payload 都要借 part 文本补全。
// raw 页要看到原始记录：message 行 + part 表内容一起给出去（事件类型的正文
// 在 part 表里）。摘要仍按 part 文本生成可读文案。
fn event_from_message_row(row: ZcodeMessageRow, parts: &[Value]) -> SessionEvent {
    let timestamp = message_row_timestamp(&row);
    let mut summary = super::summarize_event(&row.kind, &row.value);

    match row.kind.as_str() {
        "todo_reminder" => {
            let text = parts
                .iter()
                .filter(|part| super::json_string(part, &["type"]).as_deref() == Some("text"))
                .filter_map(|part| super::json_string(part, &["text"]))
                .collect::<Vec<_>>()
                .join("\n");

            if !text.is_empty() {
                summary = format!("todo reminder: {}", super::normalize_title(text));
            }
        }
        "timeline_event" => {
            let timeline_type = parts
                .iter()
                .find(|part| super::json_string(part, &["type"]).as_deref() == Some("timeline"))
                .and_then(|part| super::json_string(part, &["timelineType"]));

            if let Some(timeline_type) = timeline_type {
                summary = format!("timeline: {timeline_type}");
            }
        }
        _ => {}
    }

    let payload = json!({
        "id": row.id,
        "session_id": row.session_id,
        "time_created": row.time_created,
        "data": row.value,
        "parts": parts,
    });

    SessionEvent {
        id: row.id,
        kind: row.kind.clone(),
        timestamp,
        summary,
        payload: Some(super::record_payload(&payload)),
        session_id: Some(row.session_id),
    }
}
