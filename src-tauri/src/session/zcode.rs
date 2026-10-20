use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::family_index::{
    Family, FamilyRow, OrphanPolicy, ParentChainRow, group_families_by_parent_chain, member_root,
};
use super::family_timeline::FamilyAgentLabel;
use super::reader_engine::{
    FamilyReader, FamilySpec, Freshness, MarkerShape, MemberTimeline, OverviewCounts, SummaryKind,
};
use super::tool_blocks::{self, ToolShape};
use super::{
    ContentBlock, SessionEvent, SessionMessage, SessionModel, SessionSummary, SessionTokenUsage,
    SourceApp, usage_stats::SqlUsageHours,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ZcodeSessionRow {
    id: String,
    parent_id: Option<String>,
    directory: String,
    title: String,
    time_created: i64,
    time_updated: i64,
    // 归档时间戳,NULL 表示未归档;老库可能没有该列,探测后缺失即视为未归档。
    time_archived: Option<i64>,
    // 一次 GROUP BY 查询按 session_id 预聚合,避免列表逐会话全表扫。
    token_usage: Option<SessionTokenUsage>,
    // 会话用过的模型,同样按 session_id 预聚合;主模型排在最前。
    models: Vec<SessionModel>,
    // "db路径:id" 组合串:list_rows 时按实例 db 生成,作为该记录的稳定
    // 展示 key(行来自 SQLite,没有真实转录文件)。
    path: PathBuf,
}

type ZcodeSessionFamily = Family<ZcodeSessionRow>;

impl ParentChainRow for ZcodeSessionRow {
    fn chain_id(&self) -> &str {
        &self.id
    }

    fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }
}

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
        // 孤儿策略与 opencode 同源:认领缺失父 id,root 回退首成员。
        Ok(group_families_by_parent_chain(
            rows,
            OrphanPolicy::ClaimMissingParent,
            member_root,
        ))
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
            models: root.models.clone(),
            archived: root.time_archived.is_some(),
            // ZCode 的转录只存在 sqlite(part.data 为明文 JSON),没有压缩形态。
            compressed: false,
        }
    }

    // usage 已在索引扫描时按 session_id 预聚合,family 总量交给引擎默认求和。
    fn row_usage(&self, row: &ZcodeSessionRow) -> Option<SessionTokenUsage> {
        row.token_usage
    }

    fn row_models(&self, row: &ZcodeSessionRow) -> Vec<SessionModel> {
        row.models.clone()
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
        // 消息级模型来自 model_usage 的调用明细,按 assistant_message_id 归到
        // 产出该回答的那次调用;同一消息有多次尝试时取最早一次。
        let models_by_message = load_models_by_message(&connection, &member_ids)?;

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

                let model = models_by_message.get(&row.id).cloned();
                messages_by_session
                    .entry(row.session_id.clone())
                    .or_default()
                    .push(SessionMessage {
                        id: row.id.clone(),
                        role: row.role.clone(),
                        timestamp: message_row_timestamp(&row),
                        blocks,
                        model,
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
    // db 文件与数据表都随 zcode 首次写入才落地,表缺失只说明该来源还没有会话。
    if !super::sqlite_table_exists(&connection, "session")? {
        return Ok(Vec::new());
    }

    let token_usages = session_token_usages(&connection)?;
    let models_by_session = session_models(&connection)?;
    // time_archived 是后加的列,老库没有它;缺失时列表照常产出,归档位恒假。
    let archived_column = super::sqlite_column_exists(&connection, "session", "time_archived")?;
    let archived_expr = if archived_column {
        "time_archived"
    } else {
        "NULL"
    };
    let mut statement = connection.prepare(&format!(
        "SELECT id, parent_id, directory, title, time_created, time_updated, {archived_expr} FROM session ORDER BY time_updated DESC",
    ))?;
    let rows = statement.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: Option<String> = row.get(3)?;
        let token_usage = token_usages.get(&id).copied();
        let models = models_by_session.get(&id).cloned().unwrap_or_default();
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
            time_archived: row.get(6)?,
            token_usage,
            models,
            path,
        })
    })?;

    rows.collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to read ZCode sessions")
}

// 会话用过的模型按 session_id 预聚合:model_usage 是调用级明细,provider_id
// 与 model_id 都在行上。计数只用于排序(主模型排最前),同次数按模型名稳定排序。
fn session_models(connection: &Connection) -> Result<HashMap<String, Vec<SessionModel>>> {
    if !super::sqlite_table_exists(connection, "model_usage")? {
        return Ok(HashMap::new());
    }

    let mut statement = connection
        .prepare(
            "SELECT session_id, provider_id, model_id, COUNT(*) AS calls
             FROM model_usage
             GROUP BY session_id, provider_id, model_id
             ORDER BY session_id, calls DESC, model_id",
        )
        .context("Failed to prepare ZCode model query")?;

    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .context("Failed to read ZCode models")?;

    let mut models = HashMap::<String, Vec<SessionModel>>::new();
    for row in rows {
        let (session_id, provider, model) = row?;
        models
            .entry(session_id)
            .or_default()
            .push(SessionModel::new(Some(&provider), &model));
    }
    Ok(models)
}

// turn_usage 表按 turn 预聚合了四类用量列,直接 SUM。其 input_tokens 是全口径
// (未命中 + 缓存读):model_usage.raw_usage_json 实测 inputTokens = provider 的
// input_tokens + cache_read_input_tokens,computed_total = input + output 同为全
// 口径。SQL 只产原始列,拆合与钳制由 normalized_usage 的归一口径统一处理。
fn session_token_usages(connection: &Connection) -> Result<HashMap<String, SessionTokenUsage>> {
    let mut statement = connection
        .prepare(
            "SELECT session_id,
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(reasoning_tokens), 0),
                    COALESCE(SUM(cache_read_input_tokens), 0),
                    COALESCE(SUM(cache_creation_input_tokens), 0)
             FROM turn_usage
             GROUP BY session_id",
        )
        .context("Failed to prepare ZCode token usage query")?;

    let rows = statement
        .query_map([], |row| {
            let session_id: String = row.get(0)?;
            let column = |index: usize| -> rusqlite::Result<i64> {
                Ok(row.get::<_, Option<i64>>(index)?.unwrap_or_default())
            };
            Ok((
                session_id,
                super::usage_stats::full_scope_usage(super::usage_stats::UsageColumns {
                    input: column(1)?,
                    output: column(2)?,
                    reasoning: column(3)?,
                    cache_read: column(4)?,
                    cache_write: column(5)?,
                }),
            ))
        })
        .context("Failed to read ZCode token usages")?;

    rows.collect::<std::result::Result<HashMap<_, _>, _>>()
        .context("Failed to read ZCode token usages")
}

// 用量曲线的小时桶:model_usage 是调用级明细,自带 model_id 与官方
// (started_at, provider_id, model_id) 索引,总量与模型切片同源;它比
// turn_usage(只记主 turn)多计入 subagent 调用,与 claude/pi 把子代理计入
// 的口径一致,列表 summary 仍走 turn_usage。归一口径与
// session_token_usages 一致(normalized_usage 单点)。db 缺失、或表还没
// 建,都表示来源不可用,返回 None。status 只取 completed,error/cancelled
// 的部分用量不计。
pub(crate) fn usage_hours() -> Result<Option<SqlUsageHours>> {
    let connection = match open_connection() {
        Ok(connection) => connection,
        Err(_) => return Ok(None),
    };
    if !super::sqlite_table_exists(&connection, "model_usage")? {
        return Ok(None);
    }

    let mut statement = connection
        .prepare(
            "SELECT session_id, started_at, model_id, input_tokens, output_tokens,
                    reasoning_tokens, cache_read_input_tokens, cache_creation_input_tokens
             FROM model_usage
             WHERE status = 'completed'",
        )
        .context("Failed to prepare Zcode usage hours query")?;

    let rows = statement
        .query_map([], |row| {
            let session_id: String = row.get(0)?;
            let started_at: i64 = row.get(1)?;
            let column = |index: usize| -> rusqlite::Result<i64> {
                Ok(row.get::<_, Option<i64>>(index)?.unwrap_or_default())
            };
            Ok((
                session_id,
                started_at,
                row.get::<_, Option<String>>(2)?,
                super::usage_stats::full_scope_usage(super::usage_stats::UsageColumns {
                    input: column(3)?,
                    output: column(4)?,
                    reasoning: column(5)?,
                    cache_read: column(6)?,
                    cache_write: column(7)?,
                }),
            ))
        })
        .context("Failed to read Zcode usage hours")?;

    let hours = super::usage_stats::hours_from_usage_rows(rows)
        .context("Failed to read Zcode usage hours")?;
    Ok(Some(hours))
}

// Member/family ordering and the path map live in the shared engine; this
// only counts message/event rows along the same classification boundary the
// timeline loading uses.
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

// 消息级的模型归属:model_usage 记录每次调用的 assistant_message_id,按它把
// (provider, model) 映射回具体回答。同一消息有多次尝试(重试)时取最早一次,
// 与实际展示的那次回答对齐。
fn load_models_by_message(
    connection: &Connection,
    member_ids: &[String],
) -> Result<HashMap<String, SessionModel>> {
    if !super::sqlite_table_exists(connection, "model_usage")? {
        return Ok(HashMap::new());
    }

    let placeholders = vec!["?"; member_ids.len()].join(",");
    let mut statement = connection.prepare(&format!(
        "SELECT assistant_message_id, provider_id, model_id
         FROM model_usage
         WHERE session_id IN ({placeholders}) AND assistant_message_id IS NOT NULL
         ORDER BY started_at ASC, attempt_index ASC"
    ))?;
    let rows = statement.query_map(params_from_iter(member_ids.iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let mut models = HashMap::new();
    for row in rows {
        let (message_id, provider, model) = row?;
        models
            .entry(message_id)
            .or_insert_with(|| SessionModel::new(Some(&provider), &model));
    }
    Ok(models)
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
                let tool_blocks = tool_blocks::tool_blocks(part, &TOOL_SHAPE);
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

// zcode tool part 的形态:{type,callID,tool,state:{status,input,output}};
// 装配体在 tool_blocks 模块,这里只声明字段差异。
const TOOL_SHAPE: ToolShape = ToolShape {
    tool_name_key: "tool",
    call_id_key: "callID",
    output_key: "output",
    extract_output: tool_blocks::text_output,
};

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
                summary = super::truncate_summary(format!(
                    "todo reminder: {}",
                    super::normalize_title(text)
                ));
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
