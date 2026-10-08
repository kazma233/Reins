use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};

use serde::Deserialize;
use serde_json::{Value, json};

use super::family_index::{Family, FamilyRow};
use super::family_timeline::FamilyAgentLabel;
use super::reader_engine::{
    FamilyReader, FamilySpec, Freshness, MarkerShape, MemberTimeline, RowErrorPolicy, scan_files,
};
use super::{
    ContentBlock, SessionEvent, SessionMessage, SessionSummary, SessionTokenUsage, SourceApp,
    SummaryAccumulator, TimelineRecord, UsageHourBuckets,
};

#[derive(Clone, Debug)]
pub(crate) struct ClaudeSessionRow {
    path: PathBuf,
    summary: SessionSummary,
    agent_session_id: String,
    agent_label: Option<String>,
    is_root: bool,
}

type ClaudeSessionFamily = Family<ClaudeSessionRow>;

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

#[derive(Deserialize)]
struct ClaudeAgentMeta {
    #[serde(rename = "agentType")]
    agent_type: Option<String>,
    description: Option<String>,
    name: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ClaudeSpec;

impl FamilySpec for ClaudeSpec {
    type Row = ClaudeSessionRow;

    fn app(&self) -> SourceApp {
        SourceApp::ClaudeCode
    }

    fn label(&self) -> &'static str {
        "Claude"
    }

    // 错误文案用带空格的产品名(claude_code.rs 历史口径)。
    fn display_label(&self) -> &'static str {
        "Claude Code"
    }

    fn scan_root(&self, root: &Path) -> PathBuf {
        root.join("projects")
    }

    // 坏文件静默跳过:一个损坏转录不能让整个列表失败,也不记日志。
    fn list_rows(&self, scan_root: &Path) -> Result<Vec<ClaudeSessionRow>> {
        let files = crate::support::fs::enumerate_jsonl_files(scan_root)?;
        scan_files(
            files,
            parse_session_index_row,
            RowErrorPolicy::SkipSilently,
            "Claude",
        )
    }

    // 按 transcript 内共享 sessionId 分组;root 选取靠 is_root 行标志(非
    // agent-* 命名的子代理文件会回退 family key,id 比较无法区分)。
    fn group_families(&self, rows: Vec<ClaudeSessionRow>) -> Result<Vec<ClaudeSessionFamily>> {
        Ok(build_session_families(rows))
    }

    // 宽失效面:全部转录 mtime + 各自父目录 mtime + 非 root 的 meta.json
    // mtime;不含 projects 根目录本身(子代理改 label 只写 meta.json)。
    fn index_freshness(&self, scan_root: &Path) -> Result<Freshness> {
        claude_projects_timestamp(scan_root).map(Freshness::Stamp)
    }

    // 同上:family 失效把非 root 成员的父目录 mtime 计入。
    fn family_freshness(&self, family: &ClaudeSessionFamily) -> Result<i64> {
        family_timestamp(family)
    }

    fn parse_full_summary(&self, path: &Path) -> Result<SessionSummary> {
        parse_full_session_summary(path)
    }

    // 路径级缓存只含 root 文件;子代理文件的消耗在各自索引行里,行级直取。
    fn row_usage(&self, row: &ClaudeSessionRow) -> Option<SessionTokenUsage> {
        row.summary.token_usage
    }

    // 单次扫描产出双半;记录解析自带 agent_session_id,引擎兜底不生效。
    fn load_members(&self, members: &[ClaudeSessionRow]) -> Result<Vec<MemberTimeline>> {
        members
            .iter()
            .map(|row| {
                let mut messages = Vec::new();
                let mut events = Vec::new();

                for (index, line) in BufReader::new(File::open(&row.path)?).lines().enumerate() {
                    let value = super::parse_json_line(&line?)?;
                    match parse_timeline_record(index, &value, &row.agent_session_id) {
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

    // marker 与名册的 root 判定都用行标志,与 (+N) 计数是两条独立口径。
    fn is_subagent_member(&self, _family: &ClaudeSessionFamily, row: &ClaudeSessionRow) -> bool {
        !row.is_root
    }

    fn agent_name(&self, row: &ClaudeSessionRow) -> String {
        family_member_display_name(row)
    }

    fn marker(&self, member_id: &str, row: &ClaudeSessionRow) -> MarkerShape {
        let extras = json!({
            "transcript_path": row.path.display().to_string(),
            "is_sidechain": true,
        });

        MarkerShape::labeled("Claude", member_id).extras(extras.clone(), extras)
    }

    fn agent_label(
        &self,
        _family: &ClaudeSessionFamily,
        row: &ClaudeSessionRow,
    ) -> FamilyAgentLabel {
        if row.is_root {
            FamilyAgentLabel::Root
        } else {
            FamilyAgentLabel::Child(family_member_display_name(row))
        }
    }
}

pub(crate) static BACKEND: FamilyReader<ClaudeSpec> = FamilyReader::new(ClaudeSpec, root);

// 测试直接按根构造引擎实例:完全脱离进程 env 与全局锁,可并行。
#[cfg(test)]
pub(crate) fn engine_at(
    root: PathBuf,
    store_dir: PathBuf,
) -> super::reader_engine::ReaderEngine<ClaudeSpec> {
    super::reader_engine::ReaderEngine::new(ClaudeSpec, root, store_dir)
}

pub(crate) fn root() -> Result<PathBuf> {
    Ok(crate::support::fs::user_home_dir()
        .context("Unable to determine home directory")?
        .join(".claude"))
}

pub(crate) fn delete_session(path: &Path) -> Result<()> {
    let family = BACKEND.engine()?.family_for_path(path)?;
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
    BACKEND.engine()?.clear()?;
    Ok(())
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

fn parse_session_index_row(path: &Path) -> Result<ClaudeSessionRow> {
    let mut summary = SummaryAccumulator::default();
    let is_root = is_root_transcript(path);

    for line in BufReader::new(File::open(path)?).lines() {
        let value = super::parse_json_line(&line?)?;
        super::update_summary_timestamp(&mut summary, &value);

        if super::json_type(&value) == Some("assistant") {
            accumulate_claude_usage(&mut summary.token_usage, &value["message"]);
        }

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

        if super::json_type(&value) == Some("assistant") {
            accumulate_claude_usage(&mut summary.token_usage, &value["message"]);
        }

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

// 每条 assistant 行携带该次 API 调用的增量 usage,逐条相加;同文件内嵌的
// sidechain 消息与 subagents/ 目录下的子代理文件都是 assistant 行,按
// "会话总消耗"口径一并计入。
fn accumulate_claude_usage(total: &mut Option<SessionTokenUsage>, message: &Value) {
    if let Some(usage) = claude_usage(message) {
        super::merge_token_usage(total, usage);
    }
}

// Claude 的 usage 字段直映射;None 表示该消息没有 usage 记录。
fn claude_usage(message: &Value) -> Option<SessionTokenUsage> {
    let usage = message.get("usage")?;

    Some(SessionTokenUsage {
        input_tokens: super::json_u64(usage, "input_tokens").unwrap_or_default(),
        output_tokens: super::json_u64(usage, "output_tokens").unwrap_or_default(),
        cache_read_tokens: super::json_u64(usage, "cache_read_input_tokens").unwrap_or_default(),
        cache_write_tokens: super::json_u64(usage, "cache_creation_input_tokens")
            .unwrap_or_default(),
    })
}

// 用量曲线的日桶:与 accumulate_claude_usage 同源,但按行级 timestamp 归到
// 本地时区的天,跨天会话由此拆开。
pub(crate) fn usage_hours(path: &Path) -> Result<UsageHourBuckets> {
    let mut buckets = UsageHourBuckets::new();

    for line in BufReader::new(File::open(path)?).lines() {
        let value = super::parse_json_line(&line?)?;
        if super::json_type(&value) != Some("assistant") {
            continue;
        }

        let timestamp = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(crate::support::time::parse_timestamp);

        if let Some(usage) = claude_usage(&value["message"]) {
            super::merge_usage_bucket(&mut buckets, timestamp.and_then(super::hour_key), usage);
        }
    }

    Ok(buckets)
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

fn claude_path_timestamp(path: &Path) -> Result<i64> {
    let mut latest = crate::support::time::file_modified_timestamp_millis(path)?;

    if let Some(parent) = path.parent() {
        latest = latest.max(crate::support::time::file_modified_timestamp_millis(
            parent,
        )?);
    }

    Ok(latest)
}

fn claude_projects_timestamp(projects_root: &Path) -> Result<i64> {
    let mut latest = 0;

    for path in crate::support::fs::enumerate_jsonl_files(projects_root)? {
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
                // 整条都是宿主注入的上下文时保留原文，其余空消息仍退化成原始报文诊断块
                blocks.push(
                    super::injected_context_block(message.get("content"), message).unwrap_or_else(
                        || {
                            super::empty_message_block(
                                "Claude Code",
                                "content was empty after sanitization",
                                Some(message.clone()),
                            )
                        },
                    ),
                );
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

// Claude Code 的本地命令记录：斜杠命令自身与它的输出分别成块，
// 提取可读文本给前端渲染（原始报文在会话文件里，界面不需要再看 JSON）。
fn local_command_block(text: &str) -> Option<ContentBlock> {
    if let Some(name) = tag_text(text, "command-name") {
        let args = tag_text(text, "command-args").unwrap_or_default();
        let summary = if args.is_empty() {
            name
        } else {
            format!("{name} {args}")
        };
        return Some(super::diagnostic_block("local_command", summary, None));
    }

    for tag in ["local-command-stdout", "local-command-stderr"] {
        if let Some(output) = tag_text(text, tag).filter(|output| !output.trim().is_empty()) {
            return Some(super::diagnostic_block(
                "local_command_output",
                output,
                None,
            ));
        }
    }

    None
}

// 取 <tag>…</tag> 之间的文本；标签不成对时返回 None，不猜。
fn tag_text(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = start + text[start..].find(&close)?;
    Some(text[start..end].trim().to_string())
}

fn parse_message_blocks(content: Option<&Value>, role: &str) -> Vec<ContentBlock> {
    match content {
        Some(Value::String(text)) => {
            if role == "user" {
                // 本地命令记录（/exit 这类斜杠命令与其输出）既不是对话，也不该当噪音丢掉
                if let Some(block) = local_command_block(text) {
                    return vec![block];
                }

                if super::is_transport_message(text) {
                    return Vec::new();
                }
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
                // 图片块没有文本：把 source.data / media_type 还原成 data URL 当正文，
                // 前端才能直接渲染
                let text = super::json_string(item, &["text"])
                    .or_else(|| super::json_string(item, &["thinking"]))
                    .or_else(|| super::json_string(item, &["content"]))
                    .or_else(|| {
                        if matches!(kind.as_str(), "image" | "input_image") {
                            super::image_reference(item)
                        } else {
                            None
                        }
                    });

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
