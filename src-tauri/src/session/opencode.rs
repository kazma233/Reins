use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::{Connection, params_from_iter};
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
    ContentBlock, DeletePlanAction, SessionEvent, SessionMessage, SessionModel, SessionSummary,
    SessionTokenUsage, SourceApp,
    delete::{DeletePlanInput, run_official_cli},
    usage_stats::SqlUsageHours,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct OpenCodeSessionRow {
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
    // "db路径:id" 组合串:list_rows 时按实例 scan_root 的 db 生成,作为该
    // 记录的稳定展示 key(行来自 SQLite,没有真实转录文件)。
    path: PathBuf,
}

type OpenCodeSessionFamily = Family<OpenCodeSessionRow>;

impl ParentChainRow for OpenCodeSessionRow {
    fn chain_id(&self) -> &str {
        &self.id
    }

    fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }
}

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

    fn list_rows(
        &self,
        scan_root: &Path,
        cache: &super::rows_cache::RowsDb,
    ) -> Result<Vec<OpenCodeSessionRow>> {
        list_session_rows(scan_root, cache)
    }

    fn group_families(&self, rows: Vec<OpenCodeSessionRow>) -> Result<Vec<OpenCodeSessionFamily>> {
        // 孤儿策略与 zcode 同源:认领缺失父 id,root 回退首成员。
        Ok(group_families_by_parent_chain(
            rows,
            OrphanPolicy::ClaimMissingParent,
            member_root,
        ))
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
            models: root.models.clone(),
            archived: root.time_archived.is_some(),
            // OpenCode v2 的转录只存在 sqlite(session_message.data 为明文
            // JSON),没有压缩形态。
            compressed: false,
        }
    }

    // usage 已在索引扫描时按 session_id 预聚合,family 总量交给引擎默认求和。
    fn row_usage(&self, row: &OpenCodeSessionRow) -> Option<SessionTokenUsage> {
        row.token_usage
    }

    fn row_models(&self, row: &OpenCodeSessionRow) -> Vec<SessionModel> {
        row.models.clone()
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

                // assistant 消息的 data.model 是这次调用实际用的模型。
                let model = super::json_string(&row.value["model"], &["id"]).map(|id| {
                    SessionModel::new(
                        super::json_string(&row.value["model"], &["providerID"]).as_deref(),
                        &id,
                    )
                });
                messages_by_session
                    .entry(row.session_id.clone())
                    .or_default()
                    .push(SessionMessage {
                        id: row.id.clone(),
                        role: row.kind.clone(),
                        timestamp: message_row_timestamp(&row),
                        blocks,
                        model,
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

// db 缺失(来源还没有会话)时取 0,让 family_index 走到空列表分支,而不是在
// metadata 读取上失败。
fn opencode_db_timestamp_at(scan_root: &Path) -> Result<i64> {
    let path = db_path_at(scan_root);
    if !path.is_file() {
        return Ok(0);
    }

    crate::support::time::file_modified_timestamp_millis(&path)
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

        run_official_cli(
            &format!("opencode session delete {}", member.id),
            Command::new("opencode")
                .arg("session")
                .arg("delete")
                .arg(&member.id),
        )?;
    }

    BACKEND.engine()?.clear()?;
    Ok(())
}

fn open_connection() -> Result<Connection> {
    Connection::open(db_path()?).context("Failed to open OpenCode sqlite database")
}

// 预演逐 id 列出官方命令;执行侧"已被官方级联删除则跳过"是命令层面的
// 幂等细节,预演仍展示完整意图。
pub(crate) fn delete_plan(input: &DeletePlanInput) -> Result<Vec<DeletePlanAction>> {
    Ok(input
        .session_ids()
        .into_iter()
        .map(|id| DeletePlanAction::RunCli {
            program: "opencode".to_string(),
            args: vec!["session".to_string(), "delete".to_string(), id],
        })
        .collect())
}

fn list_session_rows(
    scan_root: &Path,
    cache: &super::rows_cache::RowsDb,
) -> Result<Vec<OpenCodeSessionRow>> {
    // db 与 session_v2 表都随首次会话写入才落地,缺失只说明该来源还没有会话。
    // 这里用读写模式打开,不先判存在就会凭空创建一个空 db 文件。
    let db_path = db_path_at(scan_root);
    if !db_path.is_file() {
        return Ok(Vec::new());
    }

    let connection =
        Connection::open(&db_path).context("Failed to open OpenCode sqlite database")?;
    if !super::sqlite_table_exists(&connection, "session_v2")? {
        return Ok(Vec::new());
    }

    let contribs = sync_assistant_contribs(&connection, cache, &db_path)?;
    let token_usages = usage_by_session(&contribs);
    let models_by_session = models_by_session(&contribs);
    // time_archived 由 v1→v2 迁移期加入,老库可能没有该列;缺失时列表照常
    // 产出,归档位恒假。
    let archived_column = super::sqlite_column_exists(&connection, "session_v2", "time_archived")?;
    let archived_expr = if archived_column {
        "time_archived"
    } else {
        "NULL"
    };
    // model 同样是迁移期加入的列,老库探测后缺失时按无模型处理。
    let model_column = super::sqlite_column_exists(&connection, "session_v2", "model")?;
    let model_expr = if model_column { "model" } else { "NULL" };
    let mut statement = connection.prepare(&format!(
        "SELECT id, parent_id, directory, title, time_created, time_updated, {archived_expr}, {model_expr} FROM session_v2 ORDER BY time_updated DESC",
    ))?;
    // session_v2.title 允许 NULL（v1 时代 NOT NULL），空标题回退到 id，
    // 避免 family 标题渲染成空白。
    let rows = statement.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: Option<String> = row.get(3)?;
        let token_usage = token_usages.get(&id).copied();
        // v2 会话只存在于 SQLite，没有 transcript 文件；用 "db路径:id" 组合串
        // 作为该记录的稳定 key。整体不是真实路径，path_key 会走原样字符串分支。
        let path = PathBuf::from(format!("{}:{}", db_path.display(), id));
        let models = match models_by_session.get(&id) {
            Some(models) => models.clone(),
            // 还没有 assistant 消息的新会话:session_v2.model 是当前选中的模型,
            // 是这时唯一能取到的模型信息。
            None => row
                .get::<_, Option<String>>(7)?
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .map(|model| {
                    super::json_string(&model, &["id"])
                        .map(|id| {
                            SessionModel::new(
                                super::json_string(&model, &["providerID"]).as_deref(),
                                &id,
                            )
                        })
                        .into_iter()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        };
        Ok(OpenCodeSessionRow {
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
        .context("Failed to read OpenCode sessions")
}

// assistant 消息的行级贡献:usage 求和与模型分组的持久最小单元。行内容
// 可变(opencode 会回写消息行),不能按会话或 rowid 水位做增量合并;失效键
// 用消息行自己的 time_updated——data 变化伴随它推进。轻扫只读 (id,
// time_updated) 两个整数列,不触碰 data 大 JSON;仅变化行做 json_extract。
#[derive(Clone, Serialize, Deserialize)]
struct AssistantContrib {
    session_id: String,
    message_time: i64,
    input: i64,
    output: i64,
    reasoning: i64,
    cache_read: i64,
    cache_write: i64,
    provider: Option<String>,
    model: Option<String>,
}

// 贡献表同步:diff 后只提取变化行,返回当前全量贡献。db 的 session_message
// 表缺失只说明还没有消息,按空贡献处理。
fn sync_assistant_contribs(
    connection: &Connection,
    cache: &super::rows_cache::RowsDb,
    db_path: &Path,
) -> Result<Vec<AssistantContrib>> {
    if !super::sqlite_table_exists(connection, "session_message")? {
        return Ok(Vec::new());
    }

    let mut statement = connection
        .prepare("SELECT id, time_updated FROM session_message WHERE type = 'assistant'")
        .context("Failed to prepare OpenCode assistant scan")?;
    let current = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<std::result::Result<HashMap<String, i64>, _>>()
        .context("Failed to read OpenCode assistant stamps")?;

    let db_key = crate::support::fs::path_key(db_path);
    let cached = cache.load_contribs(SourceApp::OpenCode, &db_key);
    let cached_stamps: HashMap<String, i64> = cached
        .iter()
        .map(|(id, stamp, _)| (id.clone(), *stamp))
        .collect();
    let mut contribs: HashMap<String, AssistantContrib> = cached
        .into_iter()
        .filter_map(|(id, _, payload)| {
            serde_json::from_str(&payload)
                .ok()
                .map(|contrib| (id, contrib))
        })
        .collect();

    let mut changed_ids = Vec::new();
    for (id, stamp) in &current {
        if cached_stamps
            .get(id)
            .is_none_or(|cached_stamp| cached_stamp != stamp)
        {
            changed_ids.push(id.clone());
        }
    }
    let mut removed_ids = Vec::new();
    for id in cached_stamps.keys() {
        if !current.contains_key(id) {
            removed_ids.push(id.clone());
        }
    }

    if !changed_ids.is_empty() || !removed_ids.is_empty() {
        let fresh = extract_assistant_contribs(connection, &changed_ids)?;
        let upserts = fresh
            .iter()
            .map(|(id, stamp, contrib)| {
                serde_json::to_string(contrib)
                    .map(|payload| (id.clone(), *stamp, payload))
                    .context("Failed to serialize OpenCode assistant contribution")
            })
            .collect::<Result<Vec<_>>>()?;
        for (id, _, contrib) in &fresh {
            contribs.insert(id.clone(), contrib.clone());
        }
        cache.sync_contribs(SourceApp::OpenCode, &db_key, &upserts, &removed_ids);
    }

    Ok(contribs.into_values().collect())
}

// 变化行的 json_extract;token 原始列与模型字段一次取齐,message_time 的
// COALESCE 口径与消息时间线一致(data.time.created 回退 time_created)。
fn extract_assistant_contribs(
    connection: &Connection,
    ids: &[String],
) -> Result<Vec<(String, i64, AssistantContrib)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = vec!["?"; ids.len()].join(",");
    let mut statement = connection
        .prepare(&format!(
            "SELECT id,
                    session_id,
                    time_updated,
                    COALESCE(json_extract(data, '$.time.created'), time_created),
                    json_extract(data, '$.model.providerID'),
                    json_extract(data, '$.model.id'),
                    COALESCE(json_extract(data, '$.tokens.input'), 0),
                    COALESCE(json_extract(data, '$.tokens.output'), 0),
                    COALESCE(json_extract(data, '$.tokens.reasoning'), 0),
                    COALESCE(json_extract(data, '$.tokens.cache.read'), 0),
                    COALESCE(json_extract(data, '$.tokens.cache.write'), 0)
             FROM session_message
             WHERE type = 'assistant' AND id IN ({placeholders})"
        ))
        .context("Failed to prepare OpenCode assistant extract")?;

    let rows = statement
        .query_map(params_from_iter(ids.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(2)?,
                AssistantContrib {
                    session_id: row.get(1)?,
                    message_time: row.get(3)?,
                    provider: row.get(4)?,
                    model: row.get(5)?,
                    input: row.get(6)?,
                    output: row.get(7)?,
                    reasoning: row.get(8)?,
                    cache_read: row.get(9)?,
                    cache_write: row.get(10)?,
                },
            ))
        })
        .context("Failed to read OpenCode assistant contributions")?;

    rows.collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to read OpenCode assistant contributions")
}

// assistant 消息的 data.tokens 是每次调用的增量,按会话求和后统一走归一
// 口径(钳制在求和之后,与原 SQL SUM 的顺序一致);reasoning 与 output 分列
// 存储,输出侧相加才与其他来源口径一致。
fn usage_by_session(contribs: &[AssistantContrib]) -> HashMap<String, SessionTokenUsage> {
    let mut sums: HashMap<String, super::usage_stats::UsageColumns> = HashMap::new();
    for contrib in contribs {
        let columns = sums.entry(contrib.session_id.clone()).or_default();
        columns.input += contrib.input;
        columns.output += contrib.output;
        columns.reasoning += contrib.reasoning;
        columns.cache_read += contrib.cache_read;
        columns.cache_write += contrib.cache_write;
    }

    sums.into_iter()
        .map(|(session_id, columns)| {
            (
                session_id,
                super::usage_stats::split_reasoning_usage(columns),
            )
        })
        .collect()
}

// 会话用过的模型:同模型可能换过 provider,一并计入。计数只用于排序
// (主模型排最前),调用次数相同则按模型名稳定排序(原 SQL 口径,provider
// 顺序在 SQL 里未定义,这里取 provider 升序保证确定性)。
fn models_by_session(contribs: &[AssistantContrib]) -> HashMap<String, Vec<SessionModel>> {
    let mut calls: HashMap<(String, Option<String>, String), usize> = HashMap::new();
    for contrib in contribs {
        if let Some(model) = &contrib.model {
            *calls
                .entry((
                    contrib.session_id.clone(),
                    contrib.provider.clone(),
                    model.clone(),
                ))
                .or_default() += 1;
        }
    }

    let mut models = HashMap::<String, Vec<SessionModel>>::new();
    let mut per_session: HashMap<String, Vec<(Option<String>, String, usize)>> = HashMap::new();
    for ((session_id, provider, model), count) in calls {
        per_session
            .entry(session_id)
            .or_default()
            .push((provider, model, count));
    }
    for (session_id, mut entries) in per_session {
        entries.sort_by(|left, right| {
            right
                .2
                .cmp(&left.2)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.0.cmp(&right.0))
        });
        models.insert(
            session_id,
            entries
                .into_iter()
                .map(|(provider, model, _)| SessionModel::new(provider.as_deref(), &model))
                .collect(),
        );
    }
    models
}

// 用量曲线的小时桶:assistant 消息的增量 usage 按消息时间归小时,数据来自
// 贡献表(与列表扫描共享同一份同步,活跃写入时不再全表 json_extract);
// 归一口径与列表侧一致(split_reasoning_usage 单点),消息时间优先
// data.time.created,回退 time_created 列,与消息时间线一致。db 缺失、或
// session_message 表还没建,都表示来源不可用,返回 None。
pub(crate) fn usage_hours() -> Result<Option<SqlUsageHours>> {
    let db_path = db_path()?;
    if !db_path.is_file() {
        return Ok(None);
    }
    let connection = match Connection::open(&db_path) {
        Ok(connection) => connection,
        Err(_) => return Ok(None),
    };
    if !super::sqlite_table_exists(&connection, "session_message")? {
        return Ok(None);
    }

    let cache = super::rows_cache::RowsDb::at_store_dir()?;
    let contribs = sync_assistant_contribs(&connection, &cache, &db_path)?;
    let rows = contribs.iter().map(|contrib| {
        rusqlite::Result::Ok((
            contrib.session_id.clone(),
            contrib.message_time,
            contrib.model.clone(),
            super::usage_stats::split_reasoning_usage(super::usage_stats::UsageColumns {
                input: contrib.input,
                output: contrib.output,
                reasoning: contrib.reasoning,
                cache_read: contrib.cache_read,
                cache_write: contrib.cache_write,
            }),
        ))
    });

    let hours = super::usage_stats::hours_from_usage_rows(rows)
        .context("Failed to read OpenCode usage hours")?;
    Ok(Some(hours))
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
            let tool_blocks = tool_blocks::tool_blocks(item, &TOOL_SHAPE);
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
                return super::truncate_summary(format!("{kind}: {normalized}"));
            }
        }
    }

    super::summarize_event(kind, value)
}

// v2 tool 块：{type,id,name,state:{status,input,content,metadata},time}。
// id 就是调用 id（call_xxx，v1 叫 callID）；输出在 state.content 块数组里
//（v1 是 state.output 字符串）。装配体在 tool_blocks 模块,这里只声明
// 字段差异。
const TOOL_SHAPE: ToolShape = ToolShape {
    tool_name_key: "name",
    call_id_key: "id",
    output_key: "content",
    extract_output: tool_blocks::content_blocks_output,
};
