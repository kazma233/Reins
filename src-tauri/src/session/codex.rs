use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use serde_json::{Value, json};

use super::family_index::{Family, FamilyRow};
use super::family_timeline::FamilyAgentLabel;
use super::reader_engine::{
    FamilyReader, FamilySpec, MarkerShape, MemberTimeline, OverviewCounts, ReaderEngine,
    RowErrorPolicy, scan_files,
};
use super::{
    ContentBlock, DeletePlanAction, SessionEvent, SessionMessage, SessionOverview, SessionSummary,
    SessionTokenUsage, SourceApp, SummaryAccumulator, TimelineRecord, UsageHourBuckets,
};

#[derive(Clone, Debug)]
pub(crate) struct CodexSessionRow {
    path: PathBuf,
    summary: SessionSummary,
    parent_session_id: Option<String>,
    forked_from_id: Option<String>,
    agent_nickname: Option<String>,
    agent_role: Option<String>,
    // resume 产生的续跑段：与原始段共享 session id，靠 session_meta 里的
    // history_base 区分。family root 必须选原始段，否则标题取自续跑段的
    // 首条消息（通常是"继续"），且选哪一段取决于文件枚举顺序、不确定。
    is_resume_segment: bool,
}

type CodexSessionFamily = Family<CodexSessionRow>;

impl FamilyRow for CodexSessionRow {
    fn member_path(&self) -> std::borrow::Cow<'_, Path> {
        std::borrow::Cow::Borrowed(&self.path)
    }

    fn family_root_id(&self) -> &str {
        &self.summary.source_session_id
    }

    fn member_id(&self) -> &str {
        &self.summary.source_session_id
    }

    fn member_created_at(&self) -> Option<i64> {
        self.summary.created_at
    }

    fn member_updated_at(&self) -> Option<i64> {
        self.summary.updated_at
    }

    fn member_tie_breaker(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Borrowed(&self.summary.source_session_id)
    }
}

#[derive(Clone)]
pub(crate) struct CodexSpec;

impl FamilySpec for CodexSpec {
    type Row = CodexSessionRow;

    fn app(&self) -> SourceApp {
        SourceApp::Codex
    }

    fn label(&self) -> &'static str {
        "Codex"
    }

    fn scan_root(&self, root: &Path) -> PathBuf {
        root.join("sessions")
    }

    fn list_rows(&self, scan_root: &Path) -> Result<Vec<CodexSessionRow>> {
        let files = crate::support::fs::enumerate_jsonl_files(scan_root)?;
        scan_files(
            files,
            parse_session_index_row,
            RowErrorPolicy::Abort,
            "Codex",
        )
    }

    fn group_families(&self, rows: Vec<CodexSessionRow>) -> Result<Vec<CodexSessionFamily>> {
        Ok(build_family_index(rows))
    }

    fn parse_full_summary(&self, path: &Path) -> Result<SessionSummary> {
        parse_full_session_summary(path)
    }

    // 索引行只读到首行 session_meta 即停,成员 usage 只能经路径级缓存取
    // 整文件解析结果;resume 段与子代理线程的消耗并入 family 统计。
    fn family_usage(
        &self,
        engine: &ReaderEngine<Self>,
        family: &CodexSessionFamily,
    ) -> Result<Option<SessionTokenUsage>> {
        let mut total = None;
        for row in &family.members {
            if let Some(usage) = engine.path_summary(&row.path)?.token_usage {
                super::merge_token_usage(&mut total, usage);
            }
        }
        Ok(total)
    }

    // 单次扫描产出双半,替代原先 messages/events 各扫一遍文件。
    fn load_members(
        &self,
        _scan_root: &Path,
        members: &[CodexSessionRow],
    ) -> Result<Vec<MemberTimeline>> {
        members
            .iter()
            .map(|row| {
                let mut messages = Vec::new();
                let mut events = Vec::new();

                for (index, line) in BufReader::new(File::open(&row.path)?).lines().enumerate() {
                    let value = super::parse_json_line(&line?)?;
                    match parse_timeline_record(index, &value) {
                        Some(TimelineRecord::Message(message)) => messages.push(message),
                        Some(TimelineRecord::Event(event)) => events.push(event),
                        None => {}
                    }
                }

                Ok(MemberTimeline {
                    messages: Arc::new(messages),
                    events: Arc::new(events),
                })
            })
            .collect()
    }

    fn agent_name(&self, row: &CodexSessionRow) -> String {
        family_member_display_name(row)
    }

    fn marker(&self, member_id: &str, row: &CodexSessionRow) -> MarkerShape {
        let extras = json!({
            "agent_nickname": row.agent_nickname,
            "agent_role": row.agent_role,
            "parent_session_id": row.parent_session_id,
            "transcript_path": row.path.display().to_string(),
        });

        MarkerShape::labeled("Codex", member_id).extras(extras.clone(), extras)
    }

    fn root_extra_events(&self, family: &CodexSessionFamily) -> Result<Vec<SessionEvent>> {
        Ok(subagent_lifecycle_events(
            &family.root.path,
            &family.members,
        ))
    }

    fn overview_counts(&self) -> OverviewCounts {
        OverviewCounts::Omitted
    }

    fn agent_label(&self, family: &CodexSessionFamily, row: &CodexSessionRow) -> FamilyAgentLabel {
        if row.summary.source_session_id == family.root.summary.source_session_id {
            FamilyAgentLabel::Root
        } else if row.forked_from_id.is_some() {
            FamilyAgentLabel::Derived(family_member_display_name(row))
        } else {
            FamilyAgentLabel::Child(family_member_display_name(row))
        }
    }
}

pub(crate) static BACKEND: FamilyReader<CodexSpec> = FamilyReader::new(CodexSpec, root);

// 测试直接按根构造引擎实例:完全脱离进程 env 与全局锁,可并行。
#[cfg(test)]
pub(crate) fn engine_at(root: PathBuf, store_dir: PathBuf) -> ReaderEngine<CodexSpec> {
    ReaderEngine::new(CodexSpec, root, store_dir)
}

pub(crate) fn root() -> Result<PathBuf> {
    Ok(crate::support::fs::user_home_dir()
        .context("Unable to determine home directory")?
        .join(".codex"))
}

pub(crate) fn delete_session(source_session_id: &str) -> Result<()> {
    let family = BACKEND.engine()?.family_for_id(source_session_id)?;

    for member in &family.members {
        // 官方删除 root 会级联整条 family；已被级联删除的成员（state 库
        // threads 行已不存在）直接跳过，避免 not found 让整个删除流程报错。
        if !thread_exists(&member.summary.source_session_id)? {
            continue;
        }

        let output = Command::new("codex")
            .arg("delete")
            .arg("--force")
            .arg(&member.summary.source_session_id)
            .output()
            .with_context(|| {
                format!(
                    "Failed to execute codex delete {}",
                    member.summary.source_session_id
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let exit_code = output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string());
            bail!(
                "Codex delete command failed for {}. exit_code: {exit_code}\nstdout:\n{}\nstderr:\n{}",
                member.summary.source_session_id,
                stdout.trim(),
                stderr.trim()
            );
        }
    }

    BACKEND.engine()?.clear()?;
    Ok(())
}

// 预演逐 id 列出官方命令;执行侧"已不在 state 库则跳过"是命令层面的幂等
// 细节,预演仍展示完整意图。
pub(crate) fn delete_plan(overview: &SessionOverview) -> Result<Vec<DeletePlanAction>> {
    Ok(super::delete::delete_target_session_ids(overview)
        .into_iter()
        .map(|id| DeletePlanAction::RunCli {
            program: "codex".to_string(),
            args: vec!["delete".to_string(), "--force".to_string(), id],
        })
        .collect())
}

// codex delete --force 非交互执行且只认 UUID；会话存在性以 state 库 threads 行为准。
fn thread_exists(session_id: &str) -> Result<bool> {
    for state_db in state_dbs()? {
        let connection = Connection::open(&state_db)
            .with_context(|| format!("Failed to open {}", state_db.display()))?;
        let exists: Option<i64> = connection
            .query_row("SELECT 1 FROM threads WHERE id = ?1", [session_id], |row| {
                row.get(0)
            })
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .with_context(|| format!("Failed to check Codex thread {session_id}"))?;
        if exists.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn parse_session_index_row(path: &Path) -> Result<CodexSessionRow> {
    let mut summary = SummaryAccumulator::default();
    let mut parent_session_id = None;
    let mut forked_from_id = None;
    let mut agent_nickname = None;
    let mut agent_role = None;
    let mut is_resume_segment = false;

    for line in BufReader::new(File::open(path)?).lines() {
        let value = super::parse_json_line(&line?)?;

        if super::json_type(&value) != Some("session_meta") {
            continue;
        }

        let payload = &value["payload"];
        summary.session_id = summary
            .session_id
            .or_else(|| super::json_string(payload, &["id"]));
        summary.cwd = summary
            .cwd
            .or_else(|| super::json_string(payload, &["cwd"]));
        summary.created_at = summary.created_at.or_else(|| {
            super::json_string(payload, &["timestamp"])
                .and_then(|value| crate::support::time::parse_timestamp(&value))
        });
        summary.git_branch = summary
            .git_branch
            .or_else(|| super::json_string(payload, &["git", "branch"]))
            .or_else(|| super::json_string(payload, &["git_branch"]))
            .or_else(|| super::json_string(payload, &["gitBranch"]));
        forked_from_id =
            forked_from_id.or_else(|| super::json_string(payload, &["forked_from_id"]));
        is_resume_segment = is_resume_segment || !payload["history_base"].is_null();
        parent_session_id = parent_session_id.or_else(|| {
            super::json_string(
                payload,
                &["source", "subagent", "thread_spawn", "parent_thread_id"],
            )
        });
        agent_nickname = agent_nickname
            .or_else(|| super::json_string(payload, &["agent_nickname"]))
            .or_else(|| {
                super::json_string(
                    payload,
                    &["source", "subagent", "thread_spawn", "agent_nickname"],
                )
            });
        agent_role = agent_role
            .or_else(|| super::json_string(payload, &["agent_role"]))
            .or_else(|| {
                super::json_string(
                    payload,
                    &["source", "subagent", "thread_spawn", "agent_role"],
                )
            });
        break;
    }

    let mut summary = super::build_summary(SourceApp::Codex, path, summary)?;
    summary.updated_at = Some(crate::support::time::file_modified_timestamp_millis(path)?);

    Ok(CodexSessionRow {
        path: path.to_path_buf(),
        summary,
        parent_session_id,
        forked_from_id,
        agent_nickname,
        agent_role,
        is_resume_segment,
    })
}

fn parse_full_session_summary(path: &Path) -> Result<SessionSummary> {
    let mut summary = SummaryAccumulator::default();

    for line in BufReader::new(File::open(path)?).lines() {
        let value = super::parse_json_line(&line?)?;
        super::update_summary_timestamp(&mut summary, &value);

        match super::json_type(&value) {
            Some("session_meta") => {
                let payload = &value["payload"];
                summary.session_id = summary
                    .session_id
                    .or_else(|| super::json_string(payload, &["id"]));
                summary.cwd = summary
                    .cwd
                    .or_else(|| super::json_string(payload, &["cwd"]));
                summary.created_at = summary.created_at.or_else(|| {
                    super::json_string(payload, &["timestamp"])
                        .and_then(|value| crate::support::time::parse_timestamp(&value))
                });
                summary.git_branch = summary
                    .git_branch
                    .or_else(|| super::json_string(payload, &["git", "branch"]))
                    .or_else(|| super::json_string(payload, &["git_branch"]))
                    .or_else(|| super::json_string(payload, &["gitBranch"]));
            }
            Some("response_item") => {
                let payload = &value["payload"];

                if super::json_string(payload, &["type"]).as_deref() == Some("message") {
                    let role = super::json_string(payload, &["role"]).unwrap_or_default();

                    if role == "user" && summary.title.is_none() {
                        summary.title = extract_title(payload.get("content"));
                    }
                }
            }
            Some("turn_context") => {
                let payload = &value["payload"];
                summary.cwd = summary
                    .cwd
                    .or_else(|| super::json_string(payload, &["cwd"]));
            }
            Some("event_msg") => {
                let payload = &value["payload"];

                if super::json_string(payload, &["type"]).as_deref() == Some("token_count") {
                    // total_token_usage 是本文件开跑以来的累计值而非增量,
                    // 只认最后一条,按增量求和会把同一消耗重复计入。
                    if let Some(usage) = payload
                        .get("info")
                        .and_then(|info| info.get("total_token_usage"))
                        .and_then(codex_cumulative_usage)
                    {
                        summary.token_usage = Some(usage);
                    }
                }
            }
            _ => {}
        }
    }

    super::build_summary(SourceApp::Codex, path, summary)
}

// Codex 的 input_tokens 含缓存命中部分,扣除后与其他来源的"新输入"口径
// 对齐;output_tokens 本身已含 reasoning。
pub(crate) fn codex_cumulative_usage(usage: &Value) -> Option<SessionTokenUsage> {
    let input = super::json_u64(usage, "input_tokens")?;
    let cached = super::json_u64(usage, "cached_input_tokens").unwrap_or_default();

    Some(SessionTokenUsage {
        input_tokens: input.saturating_sub(cached),
        output_tokens: super::json_u64(usage, "output_tokens").unwrap_or_default(),
        cache_read_tokens: cached,
        cache_write_tokens: super::json_u64(usage, "cache_write_input_tokens").unwrap_or_default(),
    })
}

// 用量曲线的小时桶:token_count 的 total_token_usage 是本文件开跑以来的
// 累计值,相邻事件差分得到增量,归到事件时间戳所在小时。首事件视作从 0 起
// 步;累计回退(compact 等重置)时差分钳 0,宁可少计也不重复计。
pub(crate) fn usage_hours(path: &Path) -> Result<UsageHourBuckets> {
    let mut buckets = UsageHourBuckets::new();
    let mut previous = SessionTokenUsage::default();

    for line in BufReader::new(File::open(path)?).lines() {
        let value = super::parse_json_line(&line?)?;
        if super::json_type(&value) != Some("event_msg") {
            continue;
        }

        let payload = &value["payload"];
        if super::json_string(payload, &["type"]).as_deref() != Some("token_count") {
            continue;
        }

        let Some(current) = payload
            .get("info")
            .and_then(|info| info.get("total_token_usage"))
            .and_then(codex_cumulative_usage)
        else {
            continue;
        };

        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(crate::support::time::parse_timestamp);

        super::merge_usage_bucket(
            &mut buckets,
            timestamp.and_then(super::hour_key),
            SessionTokenUsage {
                input_tokens: current.input_tokens.saturating_sub(previous.input_tokens),
                output_tokens: current.output_tokens.saturating_sub(previous.output_tokens),
                cache_read_tokens: current
                    .cache_read_tokens
                    .saturating_sub(previous.cache_read_tokens),
                cache_write_tokens: current
                    .cache_write_tokens
                    .saturating_sub(previous.cache_write_tokens),
            },
        );
        previous = current;
    }

    Ok(buckets)
}

// Member/family ordering and the id/path maps live in the shared engine; this
// only groups rows along the parent-thread chain and picks each family root.
fn build_family_index(rows: Vec<CodexSessionRow>) -> Vec<CodexSessionFamily> {
    let by_id = rows
        .iter()
        .cloned()
        .map(|row| (row.summary.source_session_id.clone(), row))
        .collect::<HashMap<_, _>>();
    let mut grouped = HashMap::<String, Vec<CodexSessionRow>>::new();

    for row in rows {
        let family_root_id = root_session_id(&row, &by_id);
        grouped.entry(family_root_id).or_default().push(row);
    }

    grouped
        .into_iter()
        .filter_map(|(root_id, members)| {
            // 同一 session id 可能有多个 rollout 文件（resume 续跑段），members
            // 此时还是文件枚举顺序，不能直接 find/first：root 选取必须是输入
            // 顺序无关的，否则标题/路径随枚举序漂移。优先选原始段（无
            // history_base），其余按 created_at、路径取最小保证确定。
            let by_segment_age = |row: &CodexSessionRow| {
                (
                    row.is_resume_segment,
                    row.summary.created_at.unwrap_or(i64::MAX),
                    row.path.display().to_string(),
                )
            };
            let root = members
                .iter()
                .filter(|row| row.summary.source_session_id == root_id)
                .min_by_key(|row| by_segment_age(row))
                .cloned()
                .or_else(|| {
                    members
                        .iter()
                        .min_by_key(|row| by_segment_age(row))
                        .cloned()
                })?;

            Some(CodexSessionFamily { root, members })
        })
        .collect()
}

fn root_session_id(row: &CodexSessionRow, by_id: &HashMap<String, CodexSessionRow>) -> String {
    let mut current_id = row.summary.source_session_id.clone();
    let mut parent_id = row.parent_session_id.clone();
    let mut visited = HashSet::from([current_id.clone()]);

    while let Some(next_parent_id) = parent_id {
        if !visited.insert(next_parent_id.clone()) {
            break;
        }

        let Some(parent) = by_id.get(&next_parent_id) else {
            break;
        };

        current_id = parent.summary.source_session_id.clone();
        parent_id = parent.parent_session_id.clone();
    }

    current_id
}

fn family_member_display_name(row: &CodexSessionRow) -> String {
    row.agent_nickname
        .clone()
        .or_else(|| row.agent_role.clone())
        .unwrap_or_else(|| row.summary.title.clone())
}

fn subagent_lifecycle_events(root_path: &Path, members: &[CodexSessionRow]) -> Vec<SessionEvent> {
    let by_id = members
        .iter()
        .map(|row| (row.summary.source_session_id.clone(), row))
        .collect::<HashMap<_, _>>();
    let mut events = Vec::new();
    let Ok(file) = File::open(root_path) else {
        return events;
    };

    for (index, line) in BufReader::new(file).lines().enumerate() {
        let Ok(line) = line else {
            continue;
        };
        let Ok(value) = super::parse_json_line(&line) else {
            continue;
        };
        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(crate::support::time::parse_timestamp);

        match super::json_type(&value) {
            Some("event_msg") => {
                let payload = &value["payload"];
                let event_type = super::json_string(payload, &["type"]).unwrap_or_default();

                if event_type == "collab_agent_spawn_end" {
                    if let Some(child_id) = super::json_string(payload, &["new_thread_id"]) {
                        if let Some(row) = by_id.get(&child_id) {
                            events.push(SessionEvent {
                                id: format!("codex-subagent-spawn-{index}"),
                                kind: "subagent_spawned".to_string(),
                                timestamp,
                                summary: format!(
                                    "Sub-agent spawned: {}",
                                    family_member_display_name(row)
                                ),
                                payload: Some(super::record_payload(&value)),
                                session_id: Some(row.summary.source_session_id.clone()),
                            });
                        }
                    }
                }

                if event_type == "collab_close_end" {
                    if let Some(child_id) = super::json_string(payload, &["receiver_thread_id"]) {
                        if let Some(row) = by_id.get(&child_id) {
                            events.push(SessionEvent {
                                id: format!("codex-subagent-close-{index}"),
                                kind: "subagent_closed".to_string(),
                                timestamp,
                                summary: format!(
                                    "Sub-agent closed: {}",
                                    family_member_display_name(row)
                                ),
                                payload: Some(super::record_payload(&value)),
                                session_id: Some(row.summary.source_session_id.clone()),
                            });
                        }
                    }
                }
            }
            Some("response_item") => {
                let payload = &value["payload"];
                if super::json_string(payload, &["type"]).as_deref() != Some("message") {
                    continue;
                }
                if super::json_string(payload, &["role"]).as_deref() != Some("user") {
                    continue;
                }

                let Some(Value::Array(content)) = payload.get("content") else {
                    continue;
                };

                for block in content {
                    let Some(text) = super::json_string(block, &["text"]) else {
                        continue;
                    };
                    let trimmed = text.trim();
                    if !trimmed.starts_with("<subagent_notification>") {
                        continue;
                    }

                    let Some(json_start) = trimmed.find('{') else {
                        continue;
                    };
                    let Some(json_end) = trimmed.rfind('}') else {
                        continue;
                    };

                    let Ok(notification) =
                        serde_json::from_str::<Value>(&trimmed[json_start..=json_end])
                    else {
                        continue;
                    };
                    let Some(child_id) = super::json_string(&notification, &["agent_path"]) else {
                        continue;
                    };
                    let Some(row) = by_id.get(&child_id) else {
                        continue;
                    };

                    events.push(SessionEvent {
                        id: format!("codex-subagent-notification-{index}"),
                        kind: "subagent_notification".to_string(),
                        timestamp,
                        summary: format!(
                            "Sub-agent notification: {}",
                            family_member_display_name(row)
                        ),
                        payload: Some(super::record_payload(&value)),
                        session_id: Some(row.summary.source_session_id.clone()),
                    });
                }
            }
            _ => {}
        }
    }

    events
}

// raw 页展示的是原始记录：事件的 payload 一律给整条记录，摘要仍按内层字段生成。
fn parse_timeline_record(index: usize, value: &Value) -> Option<TimelineRecord> {
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(crate::support::time::parse_timestamp);

    match super::json_type(value) {
        Some("response_item") => {
            let payload = &value["payload"];
            let payload_type = super::json_string(payload, &["type"]).unwrap_or_default();

            if payload_type == "message" {
                let role =
                    super::json_string(payload, &["role"]).unwrap_or_else(|| "unknown".to_string());
                let mut blocks = parse_message_blocks(payload.get("content"), &role);

                if role == "user" {
                    blocks = super::sanitize_user_blocks(blocks);
                }

                if role == "developer" {
                    return Some(TimelineRecord::Event(SessionEvent {
                        id: format!("codex-event-{index}"),
                        kind: "developer_message".to_string(),
                        timestamp,
                        summary: super::summarize_event("developer_message", payload),
                        payload: Some(super::record_payload(value)),
                        session_id: super::json_string(value, &["session_id"]),
                    }));
                }

                if blocks.is_empty() {
                    // 整条消息都是宿主注入的上下文（AGENTS.md 指令、environment_context）
                    // 时保留原文：这类消息没有对话内容，但内容本身要能看；其余空消息
                    // 仍退化成一行原始报文诊断块。
                    blocks.push(
                        super::injected_context_block(payload.get("content"), payload)
                            .unwrap_or_else(|| {
                                super::empty_message_block(
                                    "Codex",
                                    "content was empty after sanitization",
                                    Some(payload.clone()),
                                )
                            }),
                    );
                }

                return Some(TimelineRecord::Message(SessionMessage {
                    id: format!("codex-message-{index}"),
                    role,
                    timestamp,
                    blocks,
                    session_id: super::json_string(value, &["session_id"]),
                }));
            } else if payload_type == "function_call" || payload_type == "function_call_output" {
                return Some(TimelineRecord::Message(SessionMessage {
                    id: format!("codex-tool-{index}"),
                    role: "tool".to_string(),
                    timestamp,
                    blocks: vec![ContentBlock {
                        kind: payload_type,
                        text: super::tool_text(payload),
                        tool_name: super::json_string(payload, &["name"]),
                        tool_call_id: super::json_string(payload, &["call_id"])
                            .or_else(|| super::json_string(payload, &["callId"])),
                        is_error: None,
                        payload: Some(payload.clone()),
                    }],
                    session_id: super::json_string(value, &["session_id"]),
                }));
            } else {
                return Some(TimelineRecord::Event(SessionEvent {
                    id: format!("codex-event-{index}"),
                    kind: payload_type.clone(),
                    timestamp,
                    summary: super::summarize_event(payload_type.as_str(), payload),
                    payload: Some(super::record_payload(value)),
                    session_id: super::json_string(value, &["session_id"]),
                }));
            }
        }
        Some("event_msg") => {
            let payload = &value["payload"];
            let event_kind =
                super::json_string(payload, &["type"]).unwrap_or_else(|| "event_msg".to_string());
            return Some(TimelineRecord::Event(SessionEvent {
                id: format!("codex-event-{index}"),
                kind: event_kind.clone(),
                timestamp,
                summary: super::summarize_event(event_kind.as_str(), payload),
                payload: Some(super::record_payload(value)),
                session_id: super::json_string(value, &["session_id"]),
            }));
        }
        Some("turn_context") | Some("session_meta") => {
            let kind = super::json_type(value).unwrap_or("unknown").to_string();
            return Some(TimelineRecord::Event(SessionEvent {
                id: format!("codex-event-{index}"),
                kind: kind.clone(),
                timestamp,
                summary: super::summarize_event(kind.as_str(), value),
                payload: Some(super::record_payload(value)),
                session_id: super::json_string(value, &["session_id"]),
            }));
        }
        Some("task_complete") | Some("turn_aborted") | Some("task_started") => {
            let kind = super::json_type(value).unwrap_or("unknown").to_string();
            return Some(TimelineRecord::Event(SessionEvent {
                id: format!("codex-event-{index}"),
                kind: kind.clone(),
                timestamp,
                summary: super::summarize_event(kind.as_str(), value),
                payload: Some(super::record_payload(value)),
                session_id: super::json_string(value, &["session_id"]),
            }));
        }
        _ => {
            let kind = super::json_type(value).unwrap_or("unknown").to_string();
            return Some(TimelineRecord::Event(SessionEvent {
                id: format!("codex-event-{index}"),
                kind,
                timestamp,
                summary: super::summarize_event("unknown", value),
                payload: Some(super::record_payload(value)),
                session_id: super::json_string(value, &["session_id"]),
            }));
        }
    }
}

fn parse_message_blocks(content: Option<&Value>, role: &str) -> Vec<ContentBlock> {
    match content {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                let kind =
                    super::json_string(item, &["type"]).unwrap_or_else(|| "unknown".to_string());
                let text = super::json_string(item, &["text"])
                    .or_else(|| super::json_string(item, &["content"]))
                    .or_else(|| super::json_string(item, &["thinking"]));

                if role == "user" && text.as_deref().is_some_and(super::is_transport_message) {
                    return None;
                }

                // 粘贴进来的图片（input_image / image）没有文本字段：把引用提成正文，
                // 否则会被当成未支持块，只能看 JSON
                if matches!(kind.as_str(), "input_image" | "image") {
                    if let Some(reference) = super::image_reference(item) {
                        return Some(ContentBlock {
                            kind: "image".to_string(),
                            text: Some(reference),
                            tool_name: None,
                            tool_call_id: None,
                            is_error: None,
                            payload: Some(item.clone()),
                        });
                    }
                }

                if text.is_none() && !matches!(kind.as_str(), "tool_use" | "tool_result") {
                    return Some(super::unsupported_block("Codex", item));
                }

                Some(ContentBlock {
                    kind,
                    text,
                    tool_name: super::json_string(item, &["name"]),
                    tool_call_id: super::json_string(item, &["id"])
                        .or_else(|| super::json_string(item, &["tool_use_id"])),
                    is_error: None,
                    payload: Some(item.clone()),
                })
            })
            .collect(),
        value => vec![super::unsupported_content_block("Codex", value)],
    }
}

fn extract_title(content: Option<&Value>) -> Option<String> {
    parse_message_blocks(content, "user")
        .into_iter()
        .filter(|block| block.kind != "unsupported_content" && block.kind != "unsupported_block")
        .filter_map(|block| block.text)
        .find_map(|text| super::title_candidate_from_text(&text))
}

fn state_dbs() -> Result<Vec<PathBuf>> {
    let mut candidates = fs::read_dir(root()?)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("state_") && name.ends_with(".sqlite"))
        })
        .collect::<Vec<_>>();

    candidates.sort();
    Ok(candidates)
}
