use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use ruzstd::decoding::{BlockDecodingStrategy, FrameDecoder};
use serde_json::{Value, json};

use super::{
    ContentBlock, SessionEvent, SessionEventPage, SessionFileEntry, SessionMessage,
    SessionMessagePage, SessionOverview, SessionReader, SessionSummary, SessionTokenUsage,
    SourceApp, TimelineCacheEntry, UsageHourBuckets,
    family_index::{Family, FamilyIndex, FamilyRow},
    family_timeline::{
        FamilyAgentLabel, cached_family_events, cached_family_messages, family_agents,
    },
};

pub(crate) struct DshBackend;

pub(crate) static BACKEND: DshBackend = DshBackend;

static DSH_TIMELINE_CACHE: LazyLock<Mutex<HashMap<String, TimelineCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static DSH_INDEX_CACHE: LazyLock<Mutex<Option<DshIndexCacheEntry>>> =
    LazyLock::new(|| Mutex::new(None));

// 可用性看 ~/.dsh/sessions 而不是 ~/.dsh 本身;不读 .credentials.yaml 等其他内容。
pub(crate) fn root() -> Result<PathBuf> {
    Ok(crate::support::fs::user_home_dir()
        .context("Unable to determine home directory")?
        .join(".dsh"))
}

pub(crate) fn sessions_root() -> Result<PathBuf> {
    Ok(root()?.join("sessions"))
}

fn lock_timeline_cache()
-> Result<std::sync::MutexGuard<'static, HashMap<String, TimelineCacheEntry>>> {
    DSH_TIMELINE_CACHE
        .lock()
        .map_err(|_| anyhow!("DSH timeline cache lock was poisoned"))
}

fn lock_index_cache() -> Result<std::sync::MutexGuard<'static, Option<DshIndexCacheEntry>>> {
    DSH_INDEX_CACHE
        .lock()
        .map_err(|_| anyhow!("DSH index cache lock was poisoned"))
}

// dsh 转录是按写入批次追加的独立 zstd 帧拼接;ruzstd 的 StreamingDecoder 只解
// 首帧,必须逐帧解码。live 文件尾帧可能截断,解到能解的位置为止。
fn decode_zstd_frames(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = FrameDecoder::new();
    let mut input = data;
    let mut output = Vec::new();
    let mut frames = 0usize;

    while !input.is_empty() {
        match decode_one_frame(&mut decoder, &mut input) {
            Ok(bytes) => {
                frames += 1;
                output.extend_from_slice(&bytes);
            }
            Err(error) => {
                // 首帧就失败说明文件损坏而不是 live 截断,向上报错。
                if frames == 0 {
                    bail!("Failed to decode DSH zstd transcript: {error}");
                }
                break;
            }
        }
    }

    Ok(output)
}

// 帧边界由解码器自己消费的字节决定,不靠扫 magic,避免压缩数据里偶然出现
// magic 字节造成错切。
fn decode_one_frame(decoder: &mut FrameDecoder, input: &mut &[u8]) -> Result<Vec<u8>> {
    decoder.init(&mut *input)?;
    decoder.decode_blocks(&mut *input, BlockDecodingStrategy::All)?;
    // reset 会丢弃缓冲,先收集;帧解完时 collect 清空全部输出。
    decoder.collect().context("Failed to collect DSH zstd frame")
}

struct DshHeader {
    id: String,
    created_at: i64,
    cwd: Option<String>,
    delegation_depth: u32,
}

#[derive(Clone)]
struct CatalogChild {
    child_id: String,
    label: Option<String>,
}

// 单个转录解析出的全部展示数据;summary 与时间线共用这一份扫描结果。
struct DshTranscript {
    header: DshHeader,
    messages: Vec<SessionMessage>,
    events: Vec<SessionEvent>,
    title: Option<String>,
    first_user_title: Option<String>,
    last_event_time: i64,
    usage_total: Option<SessionTokenUsage>,
    usage_buckets: UsageHourBuckets,
    catalog_children: Vec<CatalogChild>,
}

fn read_transcript(path: &Path) -> Result<DshTranscript> {
    let raw = fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let bytes = if path.extension().and_then(|ext| ext.to_str()) == Some("zstd") {
        decode_zstd_frames(&raw)?
    } else {
        raw
    };
    // 尾帧截断可能切在多字节字符中间,损失地转成字符串而不是报错。
    let text = String::from_utf8_lossy(&bytes);

    let mut values = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str(line)
            .with_context(|| format!("Invalid DSH JSON at {} line {}", path.display(), index + 1))?;
        values.push(value);
    }

    let header_value = values
        .first()
        .filter(|value| super::json_type(value) == Some("session"))
        .ok_or_else(|| anyhow!("DSH transcript has no session header: {}", path.display()))?;
    let header = parse_header(header_value)?;
    let timeline = build_timeline(header, &values[1..]);
    Ok(timeline)
}

fn parse_header(value: &Value) -> Result<DshHeader> {
    let id = super::json_string(value, &["id"])
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow!("DSH session header has no id"))?;
    Ok(DshHeader {
        id,
        created_at: value.get("createdAt").and_then(Value::as_i64).unwrap_or_default(),
        cwd: super::json_string(value, &["cwd"]).filter(|cwd| !cwd.is_empty()),
        delegation_depth: value
            .get("delegationDepth")
            .and_then(Value::as_u64)
            .unwrap_or_default() as u32,
    })
}

// surface 节点带有可折叠的 seq 覆盖区间;replace 折叠 [startSeq,endSeq] 内的
// 已有 surface 节点(官方 compaction 语义),非 surface 事件不受影响。
struct TimelineItems {
    messages: Vec<(Option<(i64, i64)>, SessionMessage)>,
    events: Vec<(Option<(i64, i64)>, SessionEvent)>,
}

impl TimelineItems {
    fn apply_replace(&mut self, start: i64, end: i64) {
        let outside = |range: &Option<(i64, i64)>| match range {
            Some((low, high)) => *high < start || *low > end,
            None => true,
        };
        self.messages.retain(|(range, _)| outside(range));
        self.events.retain(|(range, _)| outside(range));
    }
}

fn build_timeline(header: DshHeader, events: &[Value]) -> DshTranscript {
    let session_id = header.id.clone();
    let mut items = TimelineItems {
        messages: Vec::new(),
        events: Vec::new(),
    };
    let mut title: Option<String> = None;
    let mut first_user_title: Option<String> = None;
    let mut last_event_time = header.created_at;
    let mut usage_total = None;
    let mut usage_buckets = UsageHourBuckets::new();
    let mut catalog_children = Vec::new();

    for (index, value) in events.iter().enumerate() {
        let kind = super::json_type(value).unwrap_or("unknown");
        let seq = value.get("seq").and_then(Value::as_i64);
        let time = value.get("time").and_then(Value::as_i64).unwrap_or(last_event_time);
        last_event_time = last_event_time.max(time);
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        let item_id = |prefix: &str| {
            format!(
                "dsh-{session_id}-{prefix}-{}",
                seq.map(|seq| seq.to_string()).unwrap_or_else(|| index.to_string())
            )
        };
        let surface_op = value.get("surfaceOp");

        match kind {
            "user/message" => {
                let source_kind = super::json_string(&data, &["source", "kind"])
                    .unwrap_or_else(|| "unknown".to_string());
                if source_kind == "user" {
                    let blocks = super::sanitize_user_blocks(content_text_blocks(&data, "text"));
                    if first_user_title.is_none() {
                        first_user_title = blocks
                            .iter()
                            .filter_map(|block| block.text.as_deref())
                            .find_map(super::title_candidate_from_text);
                    }
                    if !blocks.is_empty() {
                        let message = SessionMessage {
                            id: item_id("message"),
                            role: "user".to_string(),
                            timestamp: Some(time),
                            blocks,
                            session_id: Some(session_id.clone()),
                        };
                        push_surface_item(&mut items, seq, surface_op, Some(message), None);
                    }
                } else {
                    // 合成注入(AGENTS.md/runtime-context/skill-catalog 等)不是人类输入,
                    // 转事件展示,不进消息时间线。
                    let summary = first_content_text(&data)
                        .map(super::normalize_title)
                        .unwrap_or_else(|| source_kind.clone());
                    let event = surface_event(
                        &item_id("event"),
                        &source_kind,
                        time,
                        summary,
                        data,
                        &session_id,
                    );
                    push_surface_item(&mut items, seq, surface_op, None, Some(event));
                }
            }
            "assistant/message" => {
                if let Some(usage) = data.get("usage").map(parse_usage) {
                    super::merge_token_usage(&mut usage_total, usage);
                    super::merge_usage_bucket(&mut usage_buckets, super::hour_key(time), usage);
                }
                let blocks = assistant_blocks(&data);
                if !blocks.is_empty() {
                    let message = SessionMessage {
                        id: item_id("message"),
                        role: "assistant".to_string(),
                        timestamp: Some(time),
                        blocks,
                        session_id: Some(session_id.clone()),
                    };
                    push_surface_item(&mut items, seq, surface_op, Some(message), None);
                }
            }
            "tool/result" => {
                let message = data.get("message").cloned().unwrap_or(Value::Null);
                let text = message
                    .get("content")
                    .and_then(Value::as_array)
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter(|block| {
                                super::json_string(block, &["type"]).as_deref() == Some("text")
                            })
                            .filter_map(|block| super::json_string(block, &["text"]))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                let block = ContentBlock {
                    kind: "function_call_output".to_string(),
                    text: (!text.is_empty()).then_some(text),
                    tool_name: None,
                    tool_call_id: super::json_string(&message, &["toolCallId"])
                        .or_else(|| super::json_string(&message, &["source", "callId"])),
                    is_error: message
                        .get("isError")
                        .and_then(Value::as_bool),
                    payload: Some(message),
                };
                let result = SessionMessage {
                    id: item_id("message"),
                    role: "tool".to_string(),
                    timestamp: Some(time),
                    blocks: vec![block],
                    session_id: Some(session_id.clone()),
                };
                push_surface_item(&mut items, seq, surface_op, Some(result), None);
            }
            "system/message" | "developer/message" => {
                let summary = message_first_text(&data)
                    .map(super::normalize_title)
                    .unwrap_or_else(|| kind.to_string());
                let event =
                    surface_event(&item_id("event"), kind, time, summary, data, &session_id);
                push_surface_item(&mut items, seq, surface_op, None, Some(event));
            }
            "session/title" => {
                if let Some(value) = super::json_string(&data, &["title"]) {
                    if !value.trim().is_empty() {
                        title = Some(value);
                    }
                }
            }
            "subagent/catalog" => {
                if let Some(child_id) = super::json_string(&data, &["childId"]) {
                    catalog_children.push(CatalogChild {
                        child_id,
                        label: super::json_string(&data, &["label"]),
                    });
                }
            }
            _ if is_ignored_type(kind) => {}
            _ if is_generic_event_type(kind) => {
                let event = surface_event(
                    &item_id("event"),
                    kind,
                    time,
                    super::summarize_event(kind, &data),
                    data,
                    &session_id,
                );
                items.events.push((None, event));
            }
            _ => {
                // 官方语义:未知 type 无 ignorable 标记应拒读;Reins 降级为事件展示
                // 而不是让整个来源不可用。
                if value.get("ignorable").and_then(Value::as_bool) != Some(true) {
                    let event = surface_event(
                        &item_id("event"),
                        kind,
                        time,
                        super::summarize_event(kind, &data),
                        data,
                        &session_id,
                    );
                    items.events.push((None, event));
                }
            }
        }
    }

    DshTranscript {
        header,
        messages: items.messages.into_iter().map(|(_, message)| message).collect(),
        events: items.events.into_iter().map(|(_, event)| event).collect(),
        title,
        first_user_title,
        last_event_time,
        usage_total,
        usage_buckets,
        catalog_children,
    }
}

// surfaceOp 为 replace 时先折叠 [startSeq,endSeq] 内已有 surface 节点(官方
// compaction 语义)再以新区间入列;append/缺省按自身 seq 入列,缺 seq 的节点
// 无法参与折叠。
fn push_surface_item(
    items: &mut TimelineItems,
    seq: Option<i64>,
    surface_op: Option<&Value>,
    message: Option<SessionMessage>,
    event: Option<SessionEvent>,
) {
    let mut range = seq.map(|seq| (seq, seq));
    if let Some(op) = surface_op {
        if op.get("op").and_then(Value::as_str) == Some("replace") {
            if let (Some(start), Some(end)) = (
                op.get("startSeq").and_then(Value::as_i64),
                op.get("endSeq").and_then(Value::as_i64),
            ) {
                let folded = (start.min(end), end.max(start));
                items.apply_replace(folded.0, folded.1);
                range = Some(folded);
            }
        }
    }
    if let Some(message) = message {
        items.messages.push((range, message));
    }
    if let Some(event) = event {
        items.events.push((range, event));
    }
}

fn surface_event(
    id: &str,
    kind: &str,
    time: i64,
    summary: String,
    data: Value,
    session_id: &str,
) -> SessionEvent {
    SessionEvent {
        id: id.to_string(),
        kind: kind.to_string(),
        timestamp: Some(time),
        summary,
        payload: Some(data),
        session_id: Some(session_id.to_string()),
    }
}

// 纯边界/噪音/重复内容:tool/call 与 assistant/message 内的 tool-call 重复,
// step 是步边界计数,delivery-accepted 是云同步投递确认。
fn is_ignored_type(kind: &str) -> bool {
    matches!(
        kind,
        "step/start"
            | "step/end"
            | "turn/start"
            | "tool/call"
            | "agent/inbox/spliced"
            | "session/title-llm-request"
            | "request/header"
    ) || kind.starts_with("session-log-deepseek/")
        || kind.starts_with("llm/retry")
        || kind.starts_with("subagent/")
}

fn is_generic_event_type(kind: &str) -> bool {
    matches!(
        kind,
        "turn/end"
            | "permission/preset"
            | "sandbox/mode"
            | "approval/policy"
            | "model/selection"
            | "request/context"
    ) || kind.starts_with("plan/")
        || kind.starts_with("todo/")
        || kind.starts_with("compaction/")
}

// usage 字段与 SessionTokenUsage 官方口径 1:1(input=未命中输入),无换算。
fn parse_usage(usage: &Value) -> SessionTokenUsage {
    SessionTokenUsage {
        input_tokens: super::json_u64(usage, "inputTokens").unwrap_or_default(),
        output_tokens: super::json_u64(usage, "outputTokens").unwrap_or_default(),
        cache_read_tokens: super::json_u64(usage, "cacheReadTokens").unwrap_or_default(),
        cache_write_tokens: super::json_u64(usage, "cacheWriteTokens").unwrap_or_default(),
    }
}

// user/message 的 content 块:本机只见 text;其余类型走 unsupported 降级。
fn content_text_blocks(data: &Value, kind: &str) -> Vec<ContentBlock> {
    data.get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .map(|block| match super::json_string(block, &["type"]).as_deref() {
                    Some("text") => ContentBlock {
                        kind: kind.to_string(),
                        text: super::json_string(block, &["text"]),
                        tool_name: None,
                        tool_call_id: None,
                        is_error: None,
                        payload: None,
                    },
                    _ => super::unsupported_block("DSH", block),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn first_content_text(data: &Value) -> Option<String> {
    data.get("content")
        .and_then(Value::as_array)?
        .iter()
        .find(|block| super::json_string(block, &["type"]).as_deref() == Some("text"))
        .and_then(|block| super::json_string(block, &["text"]))
}

fn message_first_text(data: &Value) -> Option<String> {
    let content = data.get("message")?.get("content")?;
    content
        .as_array()?
        .iter()
        .find(|block| super::json_string(block, &["type"]).as_deref() == Some("text"))
        .and_then(|block| super::json_string(block, &["text"]))
}

// assistant 消息:reasoning→thinking、text→text、tool-call→function_call
// (arguments 是 JSON 字符串,原样作 text,payload.input 放解析结果);
// 空 reasoning 是流式占位,没有展示价值,跳过。
fn assistant_blocks(data: &Value) -> Vec<ContentBlock> {
    let Some(blocks) = data.get("message").and_then(|message| message.get("content")).and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for block in blocks {
        match super::json_string(block, &["type"]).as_deref() {
            Some("reasoning") => {
                if let Some(text) = super::json_string(block, &["text"]) {
                    if !text.is_empty() {
                        result.push(ContentBlock {
                            kind: "thinking".to_string(),
                            text: Some(text),
                            tool_name: None,
                            tool_call_id: None,
                            is_error: None,
                            payload: None,
                        });
                    }
                }
            }
            Some("text") => {
                if let Some(text) = super::json_string(block, &["text"]) {
                    if !text.is_empty() {
                        result.push(ContentBlock {
                            kind: "text".to_string(),
                            text: Some(text),
                            tool_name: None,
                            tool_call_id: None,
                            is_error: None,
                            payload: None,
                        });
                    }
                }
            }
            Some("tool-call") => {
                let raw = super::json_string(block, &["arguments"]).unwrap_or_default();
                // 模型中断会留下截断的 arguments;按 {raw} 降级,不让单条坏数据
                // 毁掉整个会话。
                let input =
                    serde_json::from_str::<Value>(&raw).unwrap_or_else(|_| json!({"raw": raw}));
                result.push(ContentBlock {
                    kind: "function_call".to_string(),
                    text: (!raw.is_empty()).then_some(raw),
                    tool_name: super::json_string(block, &["name"]),
                    tool_call_id: super::json_string(block, &["id"]),
                    is_error: None,
                    payload: Some(json!({"input": input})),
                });
            }
            _ => result.push(super::unsupported_block("DSH", block)),
        }
    }
    result
}

// --- 目录扫描 ---

// 会话文件名 session.v{N}.jsonl[.zstd],同目录多代共存取最高代;压缩可选。
fn transcript_generation(file_name: &str) -> Option<(u32, bool)> {
    let rest = file_name.strip_prefix("session.v")?;
    let (stem, compressed) = if let Some(stem) = rest.strip_suffix(".jsonl.zstd") {
        (stem, true)
    } else if let Some(stem) = rest.strip_suffix(".jsonl") {
        (stem, false)
    } else {
        return None;
    };
    Some((stem.parse().ok()?, compressed))
}

fn session_transcript_paths_at(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut latest: HashMap<PathBuf, (u32, bool, PathBuf)> = HashMap::new();
    for entry in walkdir::WalkDir::new(root).min_depth(3).max_depth(3) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str() else {
            continue;
        };
        let Some((generation, compressed)) = transcript_generation(name) else {
            continue;
        };
        let Some(directory) = entry.path().parent() else {
            continue;
        };
        match latest.get_mut(directory) {
            Some(current) if (generation, compressed) <= (current.0, current.1) => {}
            Some(current) => {
                current.0 = generation;
                current.1 = compressed;
                current.2 = entry.into_path();
            }
            None => {
                latest.insert(directory.to_path_buf(), (generation, compressed, entry.into_path()));
            }
        }
    }
    let mut paths: Vec<PathBuf> = latest.into_values().map(|(_, _, path)| path).collect();
    paths.sort();
    Ok(paths)
}

// usage 统计与列表扫描共用同一发现规则。
pub(crate) fn session_transcript_paths() -> Result<Vec<PathBuf>> {
    session_transcript_paths_at(&sessions_root()?)
}

// --- family 索引 ---

#[derive(Clone)]
struct DshSessionRow {
    session_id: String,
    delegation_depth: u32,
    title: String,
    cwd: Option<String>,
    path: PathBuf,
    created_at: i64,
    updated_at: i64,
    token_usage: Option<SessionTokenUsage>,
    catalog_children: Vec<CatalogChild>,
}

#[derive(Clone)]
struct DshFamilyRow {
    row: DshSessionRow,
    family_root_id: String,
    // 父日志 catalog 声明的关系;root 自身为 None。
    parent_id: Option<String>,
    label: Option<String>,
}

impl FamilyRow for DshFamilyRow {
    fn member_path(&self) -> Cow<'_, Path> {
        Cow::Borrowed(&self.row.path)
    }
    fn family_root_id(&self) -> &str {
        &self.family_root_id
    }
    fn member_id(&self) -> &str {
        &self.row.session_id
    }
    fn member_created_at(&self) -> Option<i64> {
        Some(self.row.created_at)
    }
    fn member_updated_at(&self) -> Option<i64> {
        Some(self.row.updated_at)
    }
    fn member_tie_breaker(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.row.session_id)
    }
}

type DshFamily = Family<DshFamilyRow>;

struct DshIndexCacheEntry {
    source_key: String,
    fingerprint: String,
    index: FamilyIndex<DshFamilyRow>,
}

fn sessions_fingerprint(paths: &[PathBuf]) -> Result<String> {
    let mut parts = Vec::with_capacity(paths.len());
    for path in paths {
        let metadata = fs::metadata(path)
            .with_context(|| format!("Failed to read metadata for {}", path.display()))?;
        parts.push(format!(
            "{}:{}:{}",
            path.display(),
            crate::support::time::file_modified_timestamp_millis(path)?,
            metadata.len()
        ));
    }
    parts.sort();
    Ok(parts.join("\n"))
}

fn session_row(path: PathBuf, transcript: DshTranscript) -> DshSessionRow {
    let title = transcript
        .title
        .or(transcript.first_user_title)
        .map(super::normalize_title)
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| transcript.header.id.clone());
    DshSessionRow {
        session_id: transcript.header.id,
        delegation_depth: transcript.header.delegation_depth,
        title,
        cwd: transcript.header.cwd,
        created_at: transcript.header.created_at,
        updated_at: transcript.header.created_at.max(transcript.last_event_time),
        path,
        token_usage: transcript.usage_total,
        catalog_children: transcript.catalog_children,
    }
}

// 从 catalog 建立的父子链向上找 family 根;链断(父文件缺失)或成环时返回 None,
// 对应孤儿子会话——v1 隐藏,无处挂靠的子会话不单独展示。
fn family_root_id_of<'a>(id: &str, owners: &HashMap<&str, DshOwner<'a>>) -> Option<&'a str> {
    let mut current = owners.get(id)?.parent_id;
    let mut visited = HashSet::from([id.to_string()]);
    loop {
        if !visited.insert(current.to_string()) {
            return None;
        }
        match owners.get(current) {
            Some(owner) => current = owner.parent_id,
            None => return Some(current),
        }
    }
}

// 父日志 catalog 对子会话的声明:父 id + 可选 label。
#[derive(Clone, Copy)]
struct DshOwner<'a> {
    parent_id: &'a str,
    label: Option<&'a str>,
}

fn build_families(rows: HashMap<String, DshSessionRow>) -> Result<Vec<DshFamily>> {
    let mut owners: HashMap<&str, DshOwner> = HashMap::new();
    for row in rows.values() {
        for child in &row.catalog_children {
            let owner = DshOwner {
                parent_id: row.session_id.as_str(),
                label: child.label.as_deref(),
            };
            match owners.insert(child.child_id.as_str(), owner) {
                // 同一父会话重复声明同一子会话(continuable 模式)是正常的。
                Some(previous) if previous.parent_id == owner.parent_id => {}
                Some(_) => bail!("Conflicting DSH subagent ownership for {}", child.child_id),
                None => {}
            }
        }
    }

    let mut roots: HashMap<String, DshFamilyRow> = HashMap::new();
    let mut children: HashMap<String, Vec<DshFamilyRow>> = HashMap::new();
    for row in rows.values() {
        let root_id = family_root_id_of(&row.session_id, &owners);
        // 有 catalog 链时以链为准;无链时 delegationDepth==0 才是根,其余为
        // 孤儿子会话,不展示。
        let is_root = match root_id {
            Some(root_id) => root_id == row.session_id,
            None => row.delegation_depth == 0 && !owners.contains_key(row.session_id.as_str()),
        };
        let owner = owners.get(row.session_id.as_str());
        let family_row = DshFamilyRow {
            family_root_id: row.session_id.clone(),
            parent_id: owner.map(|owner| owner.parent_id.to_string()),
            label: owner.and_then(|owner| owner.label.map(str::to_string)),
            row: row.clone(),
        };
        if is_root {
            roots.insert(row.session_id.clone(), family_row);
        } else if let Some(root_id) = root_id {
            children
                .entry(root_id.to_string())
                .or_default()
                .push(DshFamilyRow {
                    family_root_id: root_id.to_string(),
                    ..family_row
                });
        }
    }

    let mut families = Vec::new();
    for (root_id, root) in roots {
        let mut members = vec![root];
        members.extend(children.remove(&root_id).unwrap_or_default());
        families.push(Family {
            root: members[0].clone(),
            members,
        });
    }
    Ok(families)
}

fn family_index() -> Result<FamilyIndex<DshFamilyRow>> {
    let root = sessions_root()?;
    let source_key = root.display().to_string();
    let paths = session_transcript_paths_at(&root)?;
    let fingerprint = sessions_fingerprint(&paths)?;

    if let Some(entry) = lock_index_cache()?
        .as_ref()
        .filter(|entry| entry.source_key == source_key && entry.fingerprint == fingerprint)
    {
        return Ok(entry.index.clone());
    }

    let mut rows = HashMap::new();
    for path in &paths {
        match read_transcript(path) {
            Ok(transcript) => {
                let row = session_row(path.clone(), transcript);
                if rows.insert(row.session_id.clone(), row).is_some() {
                    bail!("Conflicting DSH session ids for {}", path.display());
                }
            }
            Err(error) => {
                // 单个损坏转录跳过,不让一个坏文件隐藏其余会话。
                crate::logger::log_error(format!(
                    "DSH session skipped {}: {error}",
                    path.display()
                ));
            }
        }
    }

    let index = FamilyIndex::build_with_ids(build_families(rows)?);
    *lock_index_cache()? = Some(DshIndexCacheEntry {
        source_key,
        fingerprint,
        index: index.clone(),
    });
    Ok(index)
}

fn family_for_path(path: &Path) -> Result<DshFamily> {
    let key = crate::support::fs::path_key(path);
    family_index()?
        .sessions_by_path
        .get(&key)
        .cloned()
        .ok_or_else(|| anyhow!("Could not find DSH session for {}", path.display()))
}

fn session_summary(row: &DshSessionRow) -> SessionSummary {
    SessionSummary {
        source_app: SourceApp::Dsh,
        source_session_id: row.session_id.clone(),
        title: row.title.clone(),
        cwd: row.cwd.clone(),
        git_branch: None,
        transcript_path: row.path.display().to_string(),
        created_at: Some(row.created_at),
        updated_at: Some(row.updated_at),
        token_usage: row.token_usage,
    }
}

fn family_summary(family: &DshFamily) -> SessionSummary {
    let mut summary = session_summary(&family.root.row);
    family.apply_summary_aggregates(&mut summary);
    summary.token_usage = family.sum_token_usage(|row| row.row.token_usage);
    summary
}

fn family_cache_timestamp(family: &DshFamily) -> Result<i64> {
    let mut timestamp = 0i64;
    for row in &family.members {
        timestamp = timestamp.max(crate::support::time::file_modified_timestamp_millis(
            &row.row.path,
        )?);
    }
    Ok(timestamp)
}

// 子代理入口 marker:payload.type=subagent_started 是前端 subagent-group 分组契约。
fn marker_message(row: &DshFamilyRow) -> SessionMessage {
    let title = row
        .label
        .clone()
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| row.row.title.clone());
    SessionMessage {
        id: format!("dsh-subagent-{}", row.row.session_id),
        role: "assistant".to_string(),
        timestamp: Some(row.row.created_at),
        blocks: vec![ContentBlock {
            kind: "output_text".to_string(),
            text: Some(format!("Sub-agent session: {}\n{}", title, row.row.session_id)),
            tool_name: None,
            tool_call_id: None,
            is_error: None,
            payload: Some(json!({
                "type": "subagent_started",
                "session_id": row.row.session_id,
                "title": title,
                "parent_session_id": row.parent_id,
                "transcript_path": row.row.path.display().to_string(),
            })),
        }],
        session_id: Some(row.row.session_id.clone()),
    }
}

fn marker_event(row: &DshFamilyRow) -> SessionEvent {
    let title = row
        .label
        .clone()
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| row.row.title.clone());
    SessionEvent {
        id: format!("dsh-subagent-{}", row.row.session_id),
        kind: "subagent_started".to_string(),
        timestamp: Some(row.row.created_at),
        summary: format!("Sub-agent session started: {title}"),
        payload: Some(json!({
            "session_id": row.row.session_id,
            "title": title,
            "parent_session_id": row.parent_id,
        })),
        session_id: Some(row.row.session_id.clone()),
    }
}

// family 时间线:root 与全部子会话的消息/事件按时间归并;marker 声明子代理入口。
fn load_family_messages(family: &DshFamily) -> Result<Vec<SessionMessage>> {
    let mut messages: Vec<_> = family
        .members
        .iter()
        .filter(|row| row.row.session_id != family.root.row.session_id)
        .map(marker_message)
        .collect();
    for row in &family.members {
        messages.extend(read_transcript(&row.row.path)?.messages);
    }
    messages.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(messages)
}

fn load_family_events(family: &DshFamily) -> Result<Vec<SessionEvent>> {
    let mut events: Vec<_> = family
        .members
        .iter()
        .filter(|row| row.row.session_id != family.root.row.session_id)
        .map(marker_event)
        .collect();
    for row in &family.members {
        events.extend(read_transcript(&row.row.path)?.events);
    }
    events.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(events)
}

fn cached_family_messages_for(family: &DshFamily) -> Result<Vec<SessionMessage>> {
    cached_family_messages(
        &DSH_TIMELINE_CACHE,
        "DSH timeline",
        family.root.row.session_id.clone(),
        family_cache_timestamp(family)?,
        || load_family_messages(family),
    )
}

fn cached_family_events_for(family: &DshFamily) -> Result<Vec<SessionEvent>> {
    cached_family_events(
        &DSH_TIMELINE_CACHE,
        "DSH timeline",
        family.root.row.session_id.clone(),
        family_cache_timestamp(family)?,
        || load_family_events(family),
    )
}

impl SessionReader for DshBackend {
    fn list_entries(&self) -> Result<Vec<SessionFileEntry>> {
        let mut entries = family_index()?
            .families
            .iter()
            .map(|family| {
                let summary = family_summary(family);
                SessionFileEntry {
                    path: PathBuf::from(&summary.transcript_path),
                    sort_timestamp: summary.updated_at.unwrap_or_default(),
                    summary: Some(summary),
                }
            })
            .collect::<Vec<_>>();
        super::sort_entries(&mut entries);
        Ok(entries)
    }

    fn clear_cache(&self) -> Result<()> {
        lock_timeline_cache()?.clear();
        *lock_index_cache()? = None;
        Ok(())
    }

    fn resolve_path(&self, source_session_id: &str) -> Result<PathBuf> {
        family_index()?
            .path_for_id(source_session_id)
            .ok_or_else(|| anyhow!("Could not find DSH session {source_session_id}"))
    }

    fn parse_summary(&self, path: &Path) -> Result<SessionSummary> {
        Ok(family_summary(&family_for_path(path)?))
    }

    fn parse_overview(&self, path: &Path) -> Result<SessionOverview> {
        let family = family_for_path(path)?;
        let messages = cached_family_messages_for(&family)?;
        let events = cached_family_events_for(&family)?;
        Ok(SessionOverview {
            summary: family_summary(&family),
            source_paths: family.source_paths(),
            message_count: Some(messages.len()),
            event_count: Some(events.len()),
            agents: family_agents(&family, |row| {
                if row.row.session_id == family.root.row.session_id {
                    FamilyAgentLabel::Root
                } else {
                    FamilyAgentLabel::Child(
                        row.label.clone().unwrap_or_else(|| row.row.title.clone()),
                    )
                }
            }),
        })
    }

    fn parse_messages_page(
        &self,
        path: &Path,
        offset: usize,
        limit: usize,
    ) -> Result<SessionMessagePage> {
        let family = family_for_path(path)?;
        let messages = cached_family_messages_for(&family)?;
        let (page, start, next_offset, total_count) =
            crate::support::paging::slice_page(&messages, offset, limit);
        Ok(SessionMessagePage {
            messages: page,
            offset: start,
            limit,
            next_offset,
            total_count,
            has_more: next_offset.is_some(),
        })
    }

    fn parse_events_page(
        &self,
        path: &Path,
        offset: usize,
        limit: usize,
    ) -> Result<SessionEventPage> {
        let family = family_for_path(path)?;
        let events = cached_family_events_for(&family)?;
        let (page, start, next_offset, total_count) =
            crate::support::paging::slice_page(&events, offset, limit);
        Ok(SessionEventPage {
            events: page,
            offset: start,
            limit,
            next_offset,
            total_count,
            has_more: next_offset.is_some(),
        })
    }

    fn parse_agent_messages(
        &self,
        path: &Path,
        agent_session_id: &str,
    ) -> Result<Vec<SessionMessage>> {
        let family = family_for_path(path)?;
        if family
            .members
            .iter()
            .all(|row| row.row.session_id != agent_session_id)
        {
            bail!("DSH subagent does not belong to this session");
        }
        Ok(super::family_timeline::agent_messages(
            cached_family_messages_for(&family)?,
            agent_session_id,
        ))
    }
}

// 用量曲线的小时桶:assistant/message.usage 按事件 time 归本地时区小时。
pub(crate) fn usage_hours(path: &Path) -> Result<UsageHourBuckets> {
    Ok(read_transcript(path)?.usage_buckets)
}
