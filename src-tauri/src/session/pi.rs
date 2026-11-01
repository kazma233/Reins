use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use uuid::Uuid;

use super::family_index::{Family, FamilyIndex, FamilyRow};
use super::family_timeline::{FamilyAgentLabel, family_agents};
use super::reader_engine::{self, MemberTimeline};
use super::{
    ContentBlock, DeletePlanAction, ModelTally, SessionEvent,
    SessionFileEntry, SessionMessage, SessionModel, SessionOverview,
    SessionReader, SessionSummary, SessionTokenUsage, SourceApp, SummaryAccumulator,
    UsageHourBuckets, delete::DeletePlanInput,
};

/// 按 sessions 根构造的读取器：持有该根下的全部缓存。
///
/// 形态与引擎的 `FamilyReader` 一致——static 前置只存「根解析函数」，
/// 实例按 root 缓存在 slot 里，**换根即整实例重建**（测试切 home 不需要
/// 手工清缓存，也不再有进程级可变状态）。
pub(crate) struct PiBackend {
    slot: Mutex<Option<(PathBuf, Arc<PiReader>)>>,
}

pub(crate) static BACKEND: PiBackend = PiBackend {
    slot: Mutex::new(None),
};

impl PiBackend {
    // 每次调用重新解析根（与迁移前一致），根变了就重建实例。
    fn reader(&self) -> Result<Arc<PiReader>> {
        let root = sessions_root()?;
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| anyhow!("Pi reader slot lock was poisoned"))?;

        if let Some((cached_root, reader)) = slot.as_ref()
            && *cached_root == root
        {
            return Ok(reader.clone());
        }

        let reader = Arc::new(PiReader::new(root.clone())?);
        *slot = Some((root, reader.clone()));
        Ok(reader)
    }
}

// 一个根下的缓存与读取逻辑。索引是 family 归族形态（bespoke，不走
// ReaderEngine：列表摘要必须保持按需懒加载，引擎的 list_entries 会强制
// 整族解析）；timeline 与 summary 继续用引擎部件：
//   timeline → DualHalfCache
//   index    → Mutex<Option<CachedIndex>>（root 就是实例本身，键只剩指纹）
//   summary  → SummaryMemCache（内存 → SQLite → 整文件解析三级回填）
pub(crate) struct PiReader {
    sessions_root: PathBuf,
    timeline: reader_engine::DualHalfCache,
    cached_index: Mutex<Option<CachedIndex>>,
    summaries: reader_engine::SummaryMemCache,
}

struct CachedIndex {
    fingerprint: String,
    index: FamilyIndex<PiRow>,
}

#[derive(Clone)]
struct PiRow {
    path: PathBuf,
    id: String,
    created_at: Option<i64>,
    updated_at: i64,
    // header.parentSession:pi 原生 fork/branch 与各代 subagent 扩展派生的
    // 子会话都写它,值是父会话文件的绝对路径。TaskRun 成员不走路径反查,
    // 用 parent_session_id 直连。
    parent_path: Option<PathBuf>,
    parent_session_id: Option<String>,
    // 扩展派生的子代理会话创建后立即 setSessionName;两代扩展的命名不同
    // (新版 "<type>#<id8>"、旧版 "subagent-<agent>-<runId>-1"),但都有名,
    // 而 fork 子会话没有 session_info——「有名字」即扩展派生的通用判据。
    agent_name: Option<String>,
    // 行是否为 family root。扫描时未知,归族构造 family 时标记;删除计划的
    // 成员折叠靠它排除 members 里的 root 行。
    is_root: bool,
    kind: PiMemberKind,
}

// 成员数据形态。pi 会话文件走整文件解析;subagent-tasks runner 是事件流,
// 时间线与摘要都从流合成。
#[derive(Clone)]
enum PiMemberKind {
    SessionFile,
    TaskRun { transcript: PathBuf },
}

impl FamilyRow for PiRow {
    fn member_path(&self) -> Cow<'_, Path> {
        Cow::Borrowed(&self.path)
    }

    fn family_root_id(&self) -> &str {
        &self.id
    }

    fn member_id(&self) -> &str {
        &self.id
    }

    fn member_created_at(&self) -> Option<i64> {
        self.created_at
    }

    fn member_updated_at(&self) -> Option<i64> {
        Some(self.updated_at)
    }

    fn member_tie_breaker(&self) -> Cow<'_, str> {
        Cow::Owned(self.path.display().to_string())
    }

    fn member_is_root(&self) -> bool {
        self.is_root
    }
}

type PiFamily = Family<PiRow>;

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

impl PiReader {
    fn new(sessions_root: PathBuf) -> Result<Self> {
        // 持久缓存目录随实例解析:测试切根即换库，与既有全局层同一份库文件格式。
        let store_dir = crate::support::fs::user_home_dir()
            .map(|home| home.join(".reins"))
            .ok_or_else(|| anyhow!("Unable to determine home directory"))?;
        Ok(Self {
            sessions_root,
            timeline: reader_engine::DualHalfCache::new("Pi timeline".to_string()),
            cached_index: Mutex::new(None),
            summaries: reader_engine::SummaryMemCache::new(
                SourceApp::Pi,
                "Pi summary".to_string(),
                crate::session::summary_cache::SummaryDb::at(store_dir),
            ),
        })
    }

    fn clear_cache(&self) -> Result<()> {
        self.timeline.clear()?;
        self.summaries.clear()?;
        *self
            .cached_index
            .lock()
            .map_err(|_| anyhow!("Pi index cache lock was poisoned"))? = None;
        Ok(())
    }

    // 索引：指纹就是新鲜度（root 由实例本身表达，不再另存 source_key）。
    // 指纹覆盖文件集合（路径+mtime+大小），子会话文件被外部删除时索引能失效;
    // subagent-tasks 在 agent 根下而不在 sessions 根内,它的产物单独进指纹。
    fn family_index(&self) -> Result<FamilyIndex<PiRow>> {
        let files = enumerate_pi_files(&self.sessions_root)?;
        let mut fingerprint_files = files.clone();
        let task_runs = scan_task_runs(&task_runs_root(), &mut fingerprint_files);
        let fingerprint = super::sessions_fingerprint(&fingerprint_files)?;

        if let Some(entry) = self
            .cached_index
            .lock()
            .map_err(|_| anyhow!("Pi index cache lock was poisoned"))?
            .as_ref()
            .filter(|entry| entry.fingerprint == fingerprint)
        {
            return Ok(entry.index.clone());
        }

        let mut rows = files
            .iter()
            .filter_map(|path| scan_row(path))
            .collect::<Vec<_>>();
        rows.extend(task_runs);
        let index = build_family_index(rows);

        *self
            .cached_index
            .lock()
            .map_err(|_| anyhow!("Pi index cache lock was poisoned"))? = Some(CachedIndex {
            fingerprint,
            index: index.clone(),
        });
        Ok(index)
    }

    fn family_for_id(&self, source_session_id: &str) -> Result<PiFamily> {
        self.family_index()?
            .family_for_id(source_session_id)
            .ok_or_else(|| anyhow!("Could not find Pi session {source_session_id}"))
    }

    // 三级摘要：内存 → SQLite 持久层 → 整文件解析，回填逐级进行（引擎部件）。
    // 单成员口径（无 family 聚合），family 级由 family_summary 在其上组装。
    fn member_summary(&self, row: &PiRow) -> Result<SessionSummary> {
        match &row.kind {
            PiMemberKind::SessionFile => self.summaries.load(&row.path, parse_full_summary),
            PiMemberKind::TaskRun { transcript } => {
                // run 转录不是 session 文件,持久层按 (路径, mtime) 键控同样适用。
                self.summaries.load(transcript, |path| parse_run_summary(row, path))
            }
        }
    }

    // 单成员双半时间线。
    fn member_timeline(&self, row: &PiRow) -> Result<MemberTimeline> {
        match &row.kind {
            PiMemberKind::SessionFile => self.timeline.load(
                crate::support::fs::path_key(&row.path),
                file_mtime(&row.path)?,
                || {
                    let document = read_document(&row.path)?;
                    Ok(build_timeline(&document.header, &document.entries))
                },
            ),
            PiMemberKind::TaskRun { transcript } => self.timeline.load(
                crate::support::fs::path_key(transcript),
                file_mtime(transcript)?,
                || build_run_timeline(&row.id, transcript),
            ),
        }
    }

    fn family_summary(&self, family: &PiFamily) -> Result<SessionSummary> {
        let mut summary = self.member_summary(&family.root)?;
        family.apply_summary_aggregates(&mut summary);
        summary.token_usage = self.family_usage(family);
        summary.models = self.family_models(family);
        Ok(summary)
    }

    // family 总用量按成员求和;成员摘要走三级缓存,打开会话详情时才解析。
    fn family_usage(&self, family: &PiFamily) -> Option<SessionTokenUsage> {
        family.sum_token_usage(|row| {
            self.member_summary(row)
                .ok()
                .and_then(|summary| summary.token_usage)
        })
    }

    fn family_models(&self, family: &PiFamily) -> Vec<SessionModel> {
        let mut tally = ModelTally::default();
        for row in &family.members {
            if let Ok(summary) = self.member_summary(row) {
                tally.absorb(&summary.models);
            }
        }
        tally.into_models()
    }

    // family 双半时间线：主会话 + 各子代理会话(子代理先注入 marker 入口),
    // 全部按 (timestamp, id) 终排,marker 与成员消息按时间交错。
    fn family_timeline(&self, family: &PiFamily) -> Result<(Vec<SessionMessage>, Vec<SessionEvent>)> {
        let mut messages = Vec::new();
        let mut events = Vec::new();
        for row in &family.members {
            let agent_member = is_agent_member(family, row);
            if agent_member {
                messages.push(self.marker_message(row));
                events.push(self.marker_event(row));
            }
            let timeline = self.member_timeline(row)?;
            messages.extend(timeline.messages.iter().cloned());
            // 归族成员的血缘已由 family 表达;独立 root(fork、父缺失)保留
            // parent_session 事件,那是它唯一的血缘表达。
            events.extend(timeline.events.iter().cloned().filter(|event| {
                !(agent_member && event.kind == "parent_session")
            }));
        }

        messages.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.id.cmp(&right.id))
        });
        events.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok((messages, events))
    }

    fn marker_message(&self, row: &PiRow) -> SessionMessage {
        let title = member_title(row);
        SessionMessage {
            id: format!("pi-subagent-start-{}", row.id),
            role: "assistant".to_string(),
            timestamp: row.created_at,
            model: None,
            blocks: vec![ContentBlock {
                kind: "output_text".to_string(),
                text: Some(format!("Sub-agent session: {title}\n{}", row.id)),
                tool_name: None,
                tool_call_id: None,
                is_error: None,
                payload: Some(json!({
                    "type": "subagent_started",
                    "session_id": row.id,
                    "title": title,
                    "transcript_path": row.path.display().to_string(),
                })),
            }],
            session_id: Some(row.id.clone()),
        }
    }

    fn marker_event(&self, row: &PiRow) -> SessionEvent {
        let title = member_title(row);
        SessionEvent {
            id: format!("pi-subagent-event-{}", row.id),
            kind: "subagent_started".to_string(),
            timestamp: row.created_at,
            summary: format!("Sub-agent session started: {title}"),
            payload: Some(json!({
                "session_id": row.id,
                "title": title,
                "transcript_path": row.path.display().to_string(),
            })),
            session_id: Some(row.id.clone()),
        }
    }
}

// marker 注入条件:family 成员里的子代理会话。members 只收录扩展派生的
// 子会话(fork 不归族),root 自身也在 members 里,按 id 排除。
fn is_agent_member(family: &PiFamily, row: &PiRow) -> bool {
    row.id != family.root.id
}

fn member_title(row: &PiRow) -> String {
    row.agent_name
        .clone()
        .unwrap_or_else(|| "子代理".to_string())
}

// 索引行:首行 header 必读;带 parentSession 的文件再在开头少量行内找
// session_info(扩展派生的子代理会话创建后立即写名,实测在第 4 行)。
// 主会话与 fork 子会话只付首行成本。
fn scan_row(path: &Path) -> Option<PiRow> {
    let header = read_header(path).ok()?;
    let updated_at = file_mtime(path).ok()?;
    let agent_name = header
        .parent_session
        .as_ref()
        .and_then(|_| agent_session_name(path));
    Some(PiRow {
        path: path.to_path_buf(),
        id: header.id,
        created_at: crate::support::time::parse_timestamp(&header.timestamp),
        updated_at,
        parent_path: header.parent_session.map(PathBuf::from),
        parent_session_id: None,
        agent_name,
        is_root: false,
        kind: PiMemberKind::SessionFile,
    })
}

// 各代 subagent 扩展的子代理会话都在创建后立即 setSessionName(新版
// "<type>#<id8>"、旧版 "subagent-<agent>-<runId>-1"),而 pi 原生 fork 的
// 子会话没有 session_info——「有名字」即扩展派生的通用判据,不逐家认
// 名字格式。只扫文件开头的少量行:名字写在创建后(实测 @4),更深的
// session_info(用户对旧会话 /rename)不改变归族判定,不值得全文读。
fn agent_session_name(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    for line in BufReader::new(file).lines().skip(1).take(32) {
        let line = line.ok()?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if super::json_type(&value) == Some("session_info") {
            return super::json_string(&value, &["name"]).filter(|name| !name.is_empty());
        }
    }
    None
}

// subagent-tasks 跟 agent 根走:sessions 可能被 sessionDir 配置重定向,
// tasks 目录不会。
fn task_runs_root() -> PathBuf {
    root().unwrap_or_default()
}

// subagent-tasks runner(旧版扩展的后台 run):task.json 携带父会话 id 与
// agent 名,过程在 output.jsonl(pi --mode json 事件流)。它不是会话文件,
// 不能独立成条目——父缺失(已删)的 run 直接跳过,数据仍在磁盘。
fn scan_task_runs(agent_root: &Path, fingerprint_files: &mut Vec<PathBuf>) -> Vec<PiRow> {
    let tasks_dir = agent_root.join("subagent-tasks");
    let Ok(entries) = fs::read_dir(&tasks_dir) else {
        return Vec::new();
    };

    let mut rows = Vec::new();
    for entry in entries.flatten() {
        let task_path = entry.path().join("task.json");
        let transcript = entry.path().join("output.jsonl");
        let Ok(content) = fs::read_to_string(&task_path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        let (Some(id), Some(parent_id)) = (
            super::json_string(&value, &["id"]),
            super::json_string(&value, &["sessionId"]),
        ) else {
            continue;
        };
        if !transcript.exists() {
            continue;
        }
        fingerprint_files.push(task_path.clone());
        fingerprint_files.push(transcript.clone());
        rows.push(PiRow {
            path: task_path,
            id,
            created_at: value.get("createdAt").and_then(Value::as_i64),
            updated_at: file_mtime(&transcript).unwrap_or_default(),
            parent_path: None,
            parent_session_id: Some(parent_id),
            agent_name: super::json_string(&value, &["agent"]),
            is_root: false,
            kind: PiMemberKind::TaskRun { transcript },
        });
    }
    rows
}

// 归族:parentSession 指向本根内会话的扩展派生子代理挂到父;fork 子会话
// (无 session_info 名)保持独立条目——fork 复制了父会话的历史,折叠进
// 时间线会整段重复。父缺失(已删)的子会话同样保持独立。
fn build_family_index(rows: Vec<PiRow>) -> FamilyIndex<PiRow> {
    // parentSession 是 pi 写下的绝对路径,与文件枚举可能隔着 symlink,
    // 两边都规范后再对表。
    let by_path = rows
        .iter()
        .map(|row| (canonical_path(&row.path), row.id.clone()))
        .collect::<HashMap<_, _>>();
    let by_id = rows
        .iter()
        .map(|row| (row.id.clone(), row.id.clone()))
        .collect::<HashMap<_, _>>();

    // 直接归属:子代理 id -> 父会话 id。只收扩展派生的子代理(有 agent 名);
    // fork 子会话保持独立。自指忽略。
    let mut owner: HashMap<String, String> = HashMap::new();
    for row in &rows {
        // TaskRun 的父关联在 task.json 里,按 id 直连;父缺失的 run 已被
        // 扫描过滤,不会出现在这里。
        if let Some(parent_id) = row.parent_session_id.as_ref() {
            if by_id.contains_key(parent_id) && parent_id != &row.id {
                owner.insert(row.id.clone(), parent_id.clone());
            }
            continue;
        }
        let (Some(parent_path), Some(_)) = (row.parent_path.as_ref(), row.agent_name.as_ref())
        else {
            continue;
        };
        if let Some(parent_id) = by_path.get(&canonical_path(parent_path))
            && parent_id != &row.id
        {
            owner.insert(row.id.clone(), parent_id.clone());
        }
    }

    // 嵌套 spawn 的子代理沿链上溯到最近的顶层主会话;环按到达者收束。
    let top_owner = |id: &str| -> Option<String> {
        let mut current = id.to_string();
        let mut visited = HashSet::from([current.clone()]);
        while let Some(parent) = owner.get(&current) {
            if !visited.insert(parent.clone()) {
                break;
            }
            current = parent.clone();
        }
        (current != id).then_some(current)
    };

    let mut children_of: HashMap<String, Vec<PiRow>> = HashMap::new();
    let mut families = Vec::new();
    for row in rows {
        match top_owner(&row.id) {
            Some(root_id) => children_of.entry(root_id).or_default().push(row),
            None => {
                let mut member = row.clone();
                member.is_root = true;
                let mut root = row;
                root.is_root = true;
                families.push(Family {
                    members: vec![member],
                    root,
                });
            }
        }
    }
    for family in &mut families {
        if let Some(children) = children_of.remove(&family.root.id) {
            family.members.extend(children);
        }
    }

    FamilyIndex::build(families)
}

fn canonical_path(path: &Path) -> PathBuf {
    crate::support::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// subagent-tasks 的事件流转录(pi --mode json)不是 session 文件:只取
// message_end 的最终消息合成线性时间线,message_start/update 是流式增量。
fn build_run_timeline(
    run_id: &str,
    transcript: &Path,
) -> Result<(Vec<SessionMessage>, Vec<SessionEvent>)> {
    let file = File::open(transcript)?;
    let mut messages = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if super::json_type(&value) != Some("message_end") {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        let entry = PiEntry {
            id: format!("pi-run-{index}"),
            parent_id: None,
            timestamp: message.get("timestamp").and_then(Value::as_i64),
            value: json!({ "type": "message", "message": message }),
        };
        messages.push(parse_message_entry(&entry, run_id));
    }
    super::fill_user_message_models(&mut messages);
    Ok((messages, Vec::new()))
}

// run 摘要从事件流合成:usage 与模型取全部 assistant 消息(与子会话文件
// 的 assistant 求和口径一致),标题用 task.json 的 agent 名。
fn parse_run_summary(row: &PiRow, transcript: &Path) -> Result<SessionSummary> {
    let file = File::open(transcript)?;
    let mut token_usage = None;
    let mut models = ModelTally::default();
    let mut created_at = row.created_at;
    let mut updated_at = row.updated_at;
    for line in BufReader::new(file).lines() {
        let line = line?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if super::json_type(&value) != Some("message_end") {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        if super::json_string(message, &["role"]).as_deref() != Some("assistant") {
            continue;
        }
        if let Some(usage) = message.get("usage") {
            super::merge_token_usage(&mut token_usage, pi_usage(usage));
        }
        if let Some(model) = super::json_string(message, &["model"]) {
            models.record(super::json_string(message, &["provider"]).as_deref(), &model);
        }
        if let Some(timestamp) = message.get("timestamp").and_then(Value::as_i64) {
            created_at = Some(created_at.map_or(timestamp, |current| current.min(timestamp)));
            updated_at = updated_at.max(timestamp);
        }
    }

    super::build_summary(
        SourceApp::Pi,
        transcript,
        SummaryAccumulator {
            session_id: Some(row.id.clone()),
            title: Some(member_title(row)),
            cwd: None,
            git_branch: None,
            created_at,
            updated_at: Some(updated_at),
            token_usage,
            models,
            archived: false,
            compressed: false,
        },
    )
}

impl SessionReader for PiBackend {
    fn list_entries(&self) -> Result<Vec<SessionFileEntry>> {
        let reader = self.reader()?;
        let mut entries = reader
            .family_index()?
            .families
            .iter()
            .map(|family| SessionFileEntry {
                source_session_id: family.root.id.clone(),
                path: family.root.path.clone(),
                // 列表排序口径:family 最近活动(全部成员 mtime 最大值)。
                sort_timestamp: family.updated_at().unwrap_or_default(),
                summary: None,
            })
            .collect::<Vec<_>>();
        super::sort_entries(&mut entries);
        Ok(entries)
    }

    fn clear_cache(&self) -> Result<()> {
        self.reader()?.clear_cache()
    }

    fn parse_summary(&self, source_session_id: &str) -> Result<SessionSummary> {
        let reader = self.reader()?;
        reader.family_summary(&reader.family_for_id(source_session_id)?)
    }

    // 删除单位是 family:主会话与全部子代理会话文件一起清。
    fn delete_plan_input(&self, source_session_id: &str) -> Result<super::delete::DeletePlanInput> {
        Ok(DeletePlanInput::from_family(
            &self.reader()?.family_for_id(source_session_id)?,
        ))
    }

    fn parse_overview(&self, source_session_id: &str) -> Result<SessionOverview> {
        let reader = self.reader()?;
        let family = reader.family_for_id(source_session_id)?;
        let summary = reader.family_summary(&family)?;
        let (messages, events) = reader.family_timeline(&family)?;

        Ok(SessionOverview {
            summary,
            source_paths: family.source_paths(),
            message_count: Some(messages.len()),
            event_count: Some(events.len()),
            agents: family_agents(&family, |row| {
                if is_agent_member(&family, row) {
                    FamilyAgentLabel::Child(member_title(row))
                } else {
                    FamilyAgentLabel::Root
                }
            }),
        })
    }

    fn parse_messages(&self, source_session_id: &str) -> Result<Vec<SessionMessage>> {
        let reader = self.reader()?;
        let (messages, _) = reader.family_timeline(&reader.family_for_id(source_session_id)?)?;
        Ok(messages)
    }

    fn parse_events(&self, source_session_id: &str) -> Result<Vec<SessionEvent>> {
        let reader = self.reader()?;
        let (_, events) = reader.family_timeline(&reader.family_for_id(source_session_id)?)?;
        Ok(events)
    }

    // 子代理过程弹窗:marker(分组锚点) + 该子代理会话的完整消息。
    fn parse_agent_messages(
        &self,
        source_session_id: &str,
        agent_session_id: &str,
    ) -> Result<Vec<SessionMessage>> {
        let reader = self.reader()?;
        let family = reader.family_for_id(source_session_id)?;
        let row = family
            .members
            .iter()
            .find(|row| is_agent_member(&family, row) && row.id == agent_session_id)
            .ok_or_else(|| anyhow!("Pi subagent {agent_session_id} does not belong to this session"))?;
        let mut messages = vec![reader.marker_message(row)];
        messages.extend(reader.member_timeline(row)?.messages.iter().cloned());
        Ok(messages)
    }
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

impl PiReader {
    fn delete_session(&self, source_session_id: &str) -> Result<()> {
        let family = self.family_for_id(source_session_id)?;
        let canonical_sessions_root = crate::support::fs::canonicalize(&self.sessions_root)
            .unwrap_or_else(|_| self.sessions_root.clone());
        for member in &family.members {
            let target = crate::support::fs::canonicalize(&member.path)
                .with_context(|| format!("Failed to resolve Pi session {}", member.path.display()))?;
            if !target.starts_with(&canonical_sessions_root)
                || target.extension().and_then(|ext| ext.to_str()) != Some("jsonl")
            {
                bail!(
                    "Refusing to delete a file outside the Pi sessions directory: {}",
                    member.path.display()
                );
            }
        }

        for member in &family.members {
            fs::remove_file(&member.path)
                .with_context(|| format!("Failed to delete {}", member.path.display()))?;
            // 显式配置的会话根不能被 prune 掉（保护在 prune_empty_parents 内）。
            super::delete::prune_empty_parents(self.sessions_root.clone(), member.path.parent());
        }
        self.clear_cache()
    }
}

// sources 注册表要的函数指针：经 static 前置取实例后删除。
pub(crate) fn delete_session(source_session_id: &str) -> Result<()> {
    BACKEND.reader()?.delete_session(source_session_id)
}

// 删除预演与执行一致:family 全部成员文件。
pub(crate) fn delete_plan(input: &DeletePlanInput) -> Result<Vec<DeletePlanAction>> {
    Ok(input
        .members
        .iter()
        .map(|member| DeletePlanAction::RemoveFile {
            path: member.path.display().to_string(),
        })
        .collect())
}

fn enumerate_pi_files(root: &Path) -> Result<Vec<PathBuf>> {
    crate::support::fs::enumerate_jsonl_files(root)
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
        token_usage: None,
        models: Default::default(),
        // pi 无归档也无压缩:会话列表就是 sessions 根下的全部 .jsonl,
        // 上下文压缩只追加 compaction 条目,不动文件。
        archived: false,
        compressed: false,
    };
    for entry in &document.entries {
        accumulate_pi_usage(&mut summary.token_usage, &entry.value);
        if let Some(message) = entry.value.get("message") {
            if super::json_string(message, &["role"]).as_deref() == Some("assistant") {
                if let Some(model) = super::json_string(message, &["model"]) {
                    summary.models.record(
                        super::json_string(message, &["provider"]).as_deref(),
                        &model,
                    );
                }
            }
        }
    }

    // header 决定创建时间，文件 mtime 只负责失效判断，避免旧 entry 时间变化导致列表漂移。
    summary.created_at = created_at;
    super::build_summary(SourceApp::Pi, path, summary)
}

// 主线程 assistant 消息带每次调用的增量 usage;子代理 run 的消耗记录在
// toolResult 的扩展元数据里,按"会话总消耗"口径一并计入。官方 usage 的
// output 已含 reasoning(totalTokens = input+output+cacheRead+cacheWrite)。
fn accumulate_pi_usage(total: &mut Option<SessionTokenUsage>, entry: &Value) {
    for (_, usage) in pi_entry_usages(entry) {
        super::merge_token_usage(total, usage);
    }
}

// 单个 entry 产生的全部增量 usage(主线程回复 0 或 1 条,subagent toolResult
// 可能带多个 run),附带模型名:主线 assistant 消息带 message.model,subagent
// run 的转录没有模型字段,为 None。
fn pi_entry_usages(entry: &Value) -> Vec<(Option<String>, SessionTokenUsage)> {
    let Some(message) = entry.get("message") else {
        return Vec::new();
    };

    match message.get("role").and_then(Value::as_str) {
        Some("assistant") => message.get("usage").map(|usage| {
            vec![(super::json_string(message, &["model"]), pi_usage(usage))]
        }).unwrap_or_default(),
        Some("toolResult")
            if super::json_string(message, &["toolName"]).as_deref() == Some("subagent") =>
        {
            message
                .get("details")
                .and_then(|details| details.get("results"))
                .and_then(Value::as_array)
                .map(|runs| {
                    runs.iter()
                        .filter_map(|run| run.get("usage").map(pi_usage))
                        .map(|usage| (None, usage))
                        .collect()
                })
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

// 用量曲线的日桶:与 accumulate_pi_usage 同源,按 entry.timestamp 归到本地
// 时区的天,跨天会话由此拆开。
pub(crate) fn usage_hours(path: &Path) -> Result<UsageHourBuckets> {
    let document = read_document(path)?;
    let mut buckets = UsageHourBuckets::new();

    for entry in &document.entries {
        for (model, usage) in pi_entry_usages(&entry.value) {
            super::merge_usage_event(
                &mut buckets,
                entry.timestamp.and_then(super::hour_key),
                model.as_deref(),
                usage,
            );
        }
    }

    Ok(buckets)
}

fn pi_usage(usage: &Value) -> SessionTokenUsage {
    SessionTokenUsage {
        input_tokens: super::json_u64(usage, "input").unwrap_or_default(),
        output_tokens: super::json_u64(usage, "output").unwrap_or_default(),
        cache_read_tokens: super::json_u64(usage, "cacheRead").unwrap_or_default(),
        cache_write_tokens: super::json_u64(usage, "cacheWrite").unwrap_or_default(),
    }
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

    // 用户输入自身不带模型,由这一轮的回答补归属。
    super::fill_user_message_models(&mut messages);

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
        model: None,
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
            model: None,
            session_id: Some(session_id.to_string()),
        };
    };
    let role = super::json_string(message, &["role"]).unwrap_or_else(|| "unknown".to_string());
    let mut blocks = match role.as_str() {
        // 系统提示：正文为空，提示词按 sections 分块存
        "system" => system_prompt_block(message)
            .map(|block| vec![block])
            .unwrap_or_else(|| vec![super::unsupported_content_block("Pi", Some(message))]),
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
        // 无可见内容的消息按语义分流：助手侧错误显示错误文案，整条都是宿主注入的
        // 上下文则保留原文，其余才退化成一行原始报文诊断块。
        let error_message =
            super::json_string(message, &["errorMessage"]).filter(|text| !text.trim().is_empty());
        blocks.push(
            error_message
                .as_deref()
                .map(|error| super::message_error_block(error, Some(message.clone())))
                .or_else(|| super::injected_context_block(message.get("content"), message))
                .unwrap_or_else(|| {
                    super::empty_message_block(
                        "Pi",
                        "content was empty after sanitization",
                        Some(message.clone()),
                    )
                }),
        );
    }

    SessionMessage {
        id: entry.id.clone(),
        role,
        timestamp: message
            .get("timestamp")
            .and_then(Value::as_i64)
            .or(entry.timestamp),
        blocks,
        // Pi 的 assistant 消息直接带这次调用的模型与 provider。
        model: super::json_string(message, &["model"]).map(|model| {
            SessionModel::new(super::json_string(message, &["provider"]).as_deref(), &model)
        }),
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
                    serde_json::to_value(nested_messages).expect("SessionMessage 序列化不会失败"),
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
        model: None,
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

// 系统提示消息：正文为空，提示词按 sections 分块存，另带本次新增/移除的工具。
// 拼回一段可读文本（段名 + 正文 + 工具名与说明）；工具的 parameters 是机械的
// JSON schema，不进正文，完整报文仍留在 payload 里。
fn system_prompt_block(message: &Value) -> Option<ContentBlock> {
    let sections = message.get("sections").and_then(Value::as_object);
    let added = tool_entries(message.get("toolsAdded"));
    let removed = tool_names(message.get("toolsRemoved"));

    if sections.is_none() && added.is_empty() && removed.is_empty() {
        return None;
    }

    let mut parts = Vec::new();

    if let Some(sections) = sections {
        // 首行当折叠行的摘要：这段提示词包含哪些段
        let names = sections
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("、");
        parts.push(format!("{} 段：{}", sections.len(), names));
        parts.extend(
            sections
                .iter()
                .map(|(name, value)| format!("## {name}\n{}", value.as_str().unwrap_or_default())),
        );
    }

    if !added.is_empty() {
        parts.push(format!("## 本次加入的工具\n{}", added.join("\n")));
    }

    if !removed.is_empty() {
        let lines = removed
            .iter()
            .map(|name| format!("- {name}"))
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!("## 本次移除的工具\n{lines}"));
    }

    Some(super::diagnostic_block(
        "system_prompt",
        parts.join("\n\n"),
        Some(message.clone()),
    ))
}

fn tool_entries(tools: Option<&Value>) -> Vec<String> {
    tools
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let name = super::json_string(item, &["name"])?;
                    let description =
                        super::json_string(item, &["description"]).unwrap_or_default();
                    Some(format!("- {name}: {description}"))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn tool_names(tools: Option<&Value>) -> Vec<String> {
    tools
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| super::json_string(item, &["name"]))
                .collect()
        })
        .unwrap_or_default()
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

    // 图片项没有 text 字段：把 data + mimeType 还原成 data URL 当正文，
    // 前端才能直接渲染，而不是退成一行 JSON
    let text = text.or_else(|| {
        if kind == "image" {
            super::image_reference(value)
        } else {
            None
        }
    });

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
