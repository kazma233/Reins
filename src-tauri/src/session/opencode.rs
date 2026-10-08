use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params_from_iter};
use serde_json::{Value, json};

use super::family_index::{Family, FamilyRow};
use super::family_timeline::FamilyAgentLabel;
use super::reader_engine::{
    FamilyReader, FamilySpec, Freshness, MarkerShape, MemberTimeline, OverviewCounts, SummaryKind,
};
use super::{
    ContentBlock, DeletePlanAction, SessionEvent, SessionMessage, SessionOverview, SessionSummary,
    SessionTokenUsage, SourceApp, UsageHourBuckets, usage_stats::SqlUsageHours,
};

#[derive(Clone, Debug)]
pub(crate) struct OpenCodeSessionRow {
    id: String,
    parent_id: Option<String>,
    directory: String,
    title: String,
    time_created: i64,
    time_updated: i64,
    // 一次 GROUP BY 查询按 session_id 预聚合,避免列表逐会话全表扫。
    token_usage: Option<SessionTokenUsage>,
    // "db路径:id" 组合串:list_rows 时按实例 scan_root 的 db 生成,作为该
    // 记录的稳定展示 key(行来自 SQLite,没有真实转录文件)。
    path: PathBuf,
}

type OpenCodeSessionFamily = Family<OpenCodeSessionRow>;

impl FamilyRow for OpenCodeSessionRow {
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
struct OpenCodeMessageRow {
    id: String,
    session_id: String,
    // type 列：user/assistant 进消息时间线，其余（system/idle/synthetic/
    // compaction/agent-switched/model-switched）归入事件。
    kind: String,
    time_created: i64,
    value: Value,
}

#[derive(Clone)]
pub(crate) struct OpenCodeSpec;

impl FamilySpec for OpenCodeSpec {
    type Row = OpenCodeSessionRow;

    fn app(&self) -> SourceApp {
        SourceApp::OpenCode
    }

    fn label(&self) -> &'static str {
        "OpenCode"
    }

    // 数据全部在 root 下的 opencode.db 里,扫描根就是 root 目录本身。
    fn scan_root(&self, root: &Path) -> PathBuf {
        root.to_path_buf()
    }

    fn list_rows(&self, scan_root: &Path) -> Result<Vec<OpenCodeSessionRow>> {
        list_session_rows(scan_root)
    }

    fn group_families(&self, rows: Vec<OpenCodeSessionRow>) -> Result<Vec<OpenCodeSessionFamily>> {
        Ok(build_session_families(rows))
    }

    // 索引失效只看 db 文件 mtime:全部行都从它 join 出来,行内时间不参与。
    fn index_freshness(&self, scan_root: &Path) -> Result<Freshness> {
        opencode_db_timestamp_at(scan_root).map(Freshness::Stamp)
    }

    fn summary_kind(&self) -> SummaryKind {
        SummaryKind::Rows
    }

    // 原始行摘要;(+N) 标题、transcript_path、族级时间戳与 usage 求和由
    // 引擎统一聚合。cwd/git_branch 引擎不动,在这里给。
    fn summary_from_rows(&self, root: &OpenCodeSessionRow) -> SessionSummary {
        SessionSummary {
            source_app: SourceApp::OpenCode,
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
    fn row_usage(&self, row: &OpenCodeSessionRow) -> Option<SessionTokenUsage> {
        row.token_usage
    }

    // 时间线失效:max(db mtime, family updated_at)。
    fn family_freshness(&self, scan_root: &Path, family: &OpenCodeSessionFamily) -> Result<i64> {
        Ok(opencode_db_timestamp_at(scan_root)?.max(family.updated_at().unwrap_or_default()))
    }

    // 单次 SQL 查询装配双半,替代原先 messages/events 各查一遍数据库。
    fn load_members(
        &self,
        scan_root: &Path,
        members: &[OpenCodeSessionRow],
    ) -> Result<Vec<MemberTimeline>> {
        let connection = open_connection_at(scan_root)?;
        let member_ids = members.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let message_rows = load_message_rows(&connection, &member_ids)?;

        let mut messages_by_session = HashMap::<String, Vec<SessionMessage>>::new();
        let mut events_by_session = HashMap::<String, Vec<SessionEvent>>::new();

        for row in message_rows {
            if row.kind == "user" || row.kind == "assistant" {
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

                messages_by_session
                    .entry(row.session_id.clone())
                    .or_default()
                    .push(SessionMessage {
                        id: row.id.clone(),
                        role: row.kind.clone(),
                        timestamp: message_row_timestamp(&row),
                        blocks,
                        session_id: Some(row.session_id.clone()),
                    });
            } else {
                events_by_session
                    .entry(row.session_id.clone())
                    .or_default()
                    .push(event_from_message_row(row));
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

    fn agent_name(&self, row: &OpenCodeSessionRow) -> String {
        row.title.clone()
    }

    // marker id 沿用历史拼写 "_started"(引擎默认是 "-start-");payload 差异
    // 字段 directory/parent_id 全放 extras,基底由引擎组装。
    fn marker(&self, member_id: &str, row: &OpenCodeSessionRow) -> MarkerShape {
        let extras = json!({
            "directory": row.directory,
            "parent_id": row.parent_id,
        });

        MarkerShape {
            message_id: format!("opencode-subagent_started-{member_id}"),
            event_id: format!("opencode-subagent_started-{member_id}"),
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
        family: &OpenCodeSessionFamily,
    ) -> Result<(usize, usize)> {
        let member_ids: Vec<&str> = family.members.iter().map(|row| row.id.as_str()).collect();
        count_family_records(scan_root, &member_ids)
    }

    fn agent_label(
        &self,
        family: &OpenCodeSessionFamily,
        row: &OpenCodeSessionRow,
    ) -> FamilyAgentLabel {
        if row.id == family.root.id {
            FamilyAgentLabel::Root
        } else {
            FamilyAgentLabel::Child(row.title.clone())
        }
    }

    // overview.source_paths:db 文件打头,成员行由它 join 出来,删除只动 db。
    fn family_source_paths(&self, scan_root: &Path, family: &OpenCodeSessionFamily) -> Vec<String> {
        let mut paths = vec![db_path_at(scan_root).display().to_string()];

        paths.extend(family.source_paths());
        paths
    }
}

pub(crate) static BACKEND: FamilyReader<OpenCodeSpec> = FamilyReader::new(OpenCodeSpec, root);

// 测试直接按根构造引擎实例:完全脱离进程 env 与全局锁,可并行。
#[cfg(test)]
pub(crate) fn engine_at(
    root: PathBuf,
    store_dir: PathBuf,
) -> super::reader_engine::ReaderEngine<OpenCodeSpec> {
    super::reader_engine::ReaderEngine::new(OpenCodeSpec, root, store_dir)
}

pub(crate) fn root() -> Result<PathBuf> {
    Ok(crate::support::fs::user_home_dir()
        .context("Unable to determine home directory")?
        .join(".local")
        .join("share")
        .join("opencode"))
}

// 模块级 env 版 db 路径仅供生产路径(delete/usage)使用;reader 引擎实例的
// 数据查询一律从实例 scan_root 派生,env 与实例互不串扰。
pub(crate) fn db_path() -> Result<PathBuf> {
    Ok(root()?.join("opencode.db"))
}

fn db_path_at(scan_root: &Path) -> PathBuf {
    scan_root.join("opencode.db")
}

fn open_connection_at(scan_root: &Path) -> Result<Connection> {
    Connection::open(db_path_at(scan_root)).context("Failed to open OpenCode sqlite database")
}

fn opencode_db_timestamp_at(scan_root: &Path) -> Result<i64> {
    crate::support::time::file_modified_timestamp_millis(&db_path_at(scan_root))
}

pub(crate) fn delete_session(source_session_id: &str) -> Result<()> {
    let family = BACKEND.engine()?.family_for_id(source_session_id)?;
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

    BACKEND.engine()?.clear()?;
    Ok(())
}

fn open_connection() -> Result<Connection> {
    Connection::open(db_path()?).context("Failed to open OpenCode sqlite database")
}

// 预演逐 id 列出官方命令;执行侧"已被官方级联删除则跳过"是命令层面的
// 幂等细节,预演仍展示完整意图。
pub(crate) fn delete_plan(overview: &SessionOverview) -> Result<Vec<DeletePlanAction>> {
    Ok(super::delete::delete_target_session_ids(overview)
        .into_iter()
        .map(|id| DeletePlanAction::RunCli {
            program: "opencode".to_string(),
            args: vec!["session".to_string(), "delete".to_string(), id],
        })
        .collect())
}

fn list_session_rows(scan_root: &Path) -> Result<Vec<OpenCodeSessionRow>> {
    let connection = open_connection_at(scan_root)?;
    let db_path = db_path_at(scan_root);
    let token_usages = session_token_usages(&connection)?;
    let mut statement = connection.prepare(
        "SELECT id, parent_id, directory, title, time_created, time_updated FROM session_v2 ORDER BY time_updated DESC",
    )?;
    // session_v2.title 允许 NULL（v1 时代 NOT NULL），空标题回退到 id，
    // 避免 family 标题渲染成空白。
    let rows = statement.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: Option<String> = row.get(3)?;
        let token_usage = token_usages.get(&id).copied();
        // v2 会话只存在于 SQLite，没有 transcript 文件；用 "db路径:id" 组合串
        // 作为该记录的稳定 key。整体不是真实路径，path_key 会走原样字符串分支。
        let path = PathBuf::from(format!("{}:{}", db_path.display(), id));
        Ok(OpenCodeSessionRow {
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
        .context("Failed to read OpenCode sessions")
}

// assistant 消息的 data.tokens 是每次调用的增量,按会话求和;reasoning 与
// output 在 OpenCode 里分列存储,输出侧相加才与其他来源口径一致。SQLite 的
// NULL 会传染加法,reasoning 缺失的行经 COALESCE 按 0 计。
fn session_token_usages(connection: &Connection) -> Result<HashMap<String, SessionTokenUsage>> {
    let mut statement = connection
        .prepare(
            "SELECT session_id,
                    COALESCE(SUM(json_extract(data, '$.tokens.input')), 0),
                    COALESCE(SUM(json_extract(data, '$.tokens.output')), 0)
                        + COALESCE(SUM(json_extract(data, '$.tokens.reasoning')), 0),
                    COALESCE(SUM(json_extract(data, '$.tokens.cache.read')), 0),
                    COALESCE(SUM(json_extract(data, '$.tokens.cache.write')), 0)
             FROM session_message
             WHERE type = 'assistant'
             GROUP BY session_id",
        )
        .context("Failed to prepare OpenCode token usage query")?;

    let rows = statement
        .query_map([], |row| {
            let session_id: String = row.get(0)?;
            // SUM 对无匹配行为返回 NULL,按 0 处理;负值理论上不出现,钳到 0。
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
        .context("Failed to read OpenCode token usages")?;

    rows.collect::<std::result::Result<HashMap<_, _>, _>>()
        .context("Failed to read OpenCode token usages")
}

// 用量曲线的小时桶:assistant 消息的增量 usage 按消息时间归小时,归一口径与
// session_token_usages 一致(output 并入 reasoning);消息时间优先
// data.time.created,回退 time_created 列,与消息时间线一致。db 缺失表示
// 来源不可用,返回 None。
pub(crate) fn usage_hours() -> Result<Option<SqlUsageHours>> {
    let connection = match open_connection() {
        Ok(connection) => connection,
        Err(_) => return Ok(None),
    };

    let mut statement = connection
        .prepare(
            "SELECT session_id,
                    COALESCE(json_extract(data, '$.time.created'), time_created),
                    COALESCE(json_extract(data, '$.tokens.input'), 0),
                    COALESCE(json_extract(data, '$.tokens.output'), 0)
                        + COALESCE(json_extract(data, '$.tokens.reasoning'), 0),
                    COALESCE(json_extract(data, '$.tokens.cache.read'), 0),
                    COALESCE(json_extract(data, '$.tokens.cache.write'), 0)
             FROM session_message
             WHERE type = 'assistant'",
        )
        .context("Failed to prepare OpenCode usage hours query")?;

    let rows = statement
        .query_map([], |row| {
            let timestamp: i64 = row.get(1)?;
            let usage_column = |index: usize| -> rusqlite::Result<u64> {
                Ok(row.get::<_, Option<i64>>(index)?.unwrap_or_default().max(0) as u64)
            };
            Ok((
                row.get::<_, String>(0)?,
                timestamp,
                SessionTokenUsage {
                    input_tokens: usage_column(2)?,
                    output_tokens: usage_column(3)?,
                    cache_read_tokens: usage_column(4)?,
                    cache_write_tokens: usage_column(5)?,
                },
            ))
        })
        .context("Failed to read OpenCode usage hours")?;

    let today = super::usage_stats::today_prefix();
    let mut buckets = UsageHourBuckets::new();
    let mut sessions = std::collections::HashSet::new();
    let mut today_sessions = std::collections::HashSet::new();

    for row in rows {
        let (session_id, timestamp, usage) = row?;
        let hour_key = super::hour_key(timestamp);
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

fn count_family_records(scan_root: &Path, member_ids: &[&str]) -> Result<(usize, usize)> {
    let connection = open_connection_at(scan_root)?;
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

// raw 页要看到原始记录：DB 行整行给出去（data 列是应用写入的原始 JSON）。
fn event_from_message_row(row: OpenCodeMessageRow) -> SessionEvent {
    let timestamp = message_row_timestamp(&row);
    let payload = json!({
        "id": row.id,
        "session_id": row.session_id,
        "type": row.kind,
        "time_created": row.time_created,
        "data": row.value,
    });

    SessionEvent {
        id: row.id,
        kind: row.kind.clone(),
        timestamp,
        summary: v2_event_summary(&row.kind, &row.value),
        payload: Some(super::record_payload(&payload)),
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
    let is_error = state
        .and_then(|state| state.get("status"))
        .and_then(Value::as_str)
        == Some("error");
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
        return (!items.is_empty())
            .then(|| super::stringify_json(content))
            .flatten();
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
