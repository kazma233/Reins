use serde_json::Value;

use super::ContentBlock;

pub(crate) fn summarize_event(kind: &str, value: &Value) -> String {
    if let Some(message) = json_string(value, &["message"]) {
        return normalize_title(message);
    }

    if let Some(event_type) = json_string(value, &["type"]) {
        if event_type != kind {
            return format!("{kind}: {}", normalize_title(event_type));
        }
    }

    if let Some(name) = json_string(value, &["name"]) {
        return format!("{kind}: {}", normalize_title(name));
    }

    kind.to_string()
}

pub(crate) fn tool_text(payload: &Value) -> Option<String> {
    if let Some(arguments) = json_string(payload, &["arguments"]) {
        return Some(arguments);
    }

    if let Some(output) = payload.get("output") {
        return stringify_json(output);
    }

    None
}

pub(crate) fn stringify_json(value: &Value) -> Option<String> {
    serde_json::to_string_pretty(value).ok()
}

pub(crate) fn diagnostic_block(
    kind: &str,
    text: impl Into<String>,
    payload: Option<Value>,
) -> ContentBlock {
    ContentBlock {
        kind: kind.to_string(),
        text: Some(text.into()),
        tool_name: None,
        tool_call_id: None,
        is_error: None,
        payload,
    }
}

fn raw_source_block(kind: &str, value: Value) -> ContentBlock {
    let text = stringify_json(&value).unwrap_or_else(|| "null".to_string());

    diagnostic_block(kind, text, Some(value))
}

pub(crate) fn unsupported_content_block(_source: &str, content: Option<&Value>) -> ContentBlock {
    raw_source_block(
        "unsupported_content",
        content.cloned().unwrap_or(Value::Null),
    )
}

pub(crate) fn unsupported_block(_source: &str, block: &Value) -> ContentBlock {
    raw_source_block("unsupported_block", block.clone())
}

pub(crate) fn empty_message_block(
    _source: &str,
    _reason: &str,
    payload: Option<Value>,
) -> ContentBlock {
    raw_source_block("empty_message", payload.unwrap_or(Value::Null))
}

/// 消息没有可见内容、但来源自己给了错误文案（Pi 的 errorMessage）：正文放错误
/// 文案，让前端按错误样式展示；原始报文留在 payload 里供展开查看。
pub(crate) fn message_error_block(error: &str, payload: Option<Value>) -> ContentBlock {
    ContentBlock {
        kind: "message_error".to_string(),
        text: Some(error.to_string()),
        tool_name: None,
        tool_call_id: None,
        is_error: Some(true),
        payload,
    }
}

pub(crate) fn empty_tool_block(_source: &str, payload: &Value) -> ContentBlock {
    raw_source_block("empty_tool", payload.clone())
}

/// raw 页展示的原始记录：整条记录原样给出去，但加密字段不下发——不可读的加密
/// 载荷不进界面（Grok Build 的思考块同样只投影可展示内容）。
pub(crate) fn record_payload(record: &Value) -> Value {
    strip_encrypted(record)
}

fn strip_encrypted(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| !key.to_ascii_lowercase().contains("encrypted"))
                .map(|(key, item)| (key.clone(), strip_encrypted(item)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(strip_encrypted).collect()),
        other => other.clone(),
    }
}

/// 会话里的图片项：各来源字段不同（Codex `image_url`、Pi `data`+`mimeType`、
/// Claude `source.data`/`media_type`、Grok Build `url`），统一还原成前端能直接用的
/// 引用：能拼成 data: URL 的拼成 data: URL，否则原样给出外链/路径。
/// 缺字段时不猜：返回 None，调用方仍按未支持块展示原始报文。
pub(crate) fn image_reference(item: &Value) -> Option<String> {
    if let Some(reference) =
        json_string(item, &["image_url"]).or_else(|| json_string(item, &["url"]))
    {
        return Some(reference);
    }

    if let Some(url) = json_string(item, &["source", "url"]) {
        return Some(url);
    }

    let data = json_string(item, &["data"]).or_else(|| json_string(item, &["source", "data"]))?;
    let mime = json_string(item, &["mimeType"])
        .or_else(|| json_string(item, &["source", "media_type"]))?;

    Some(format!("data:{mime};base64,{data}"))
}

/// 宿主注入的上下文（AGENTS.md 指令、environment_context、system-reminder 等）
/// 清洗后会让消息没有可展示的对话内容，但原文本身要能看。判定复用清洗用的同一批
/// 前缀，且要求整条消息的文本项都是注入内容；只要有一条真对话文本就不算。
pub(crate) fn injected_context_block(
    content: Option<&Value>,
    payload: &Value,
) -> Option<ContentBlock> {
    let texts = match content? {
        // Claude Code 的 user 消息允许直接给字符串正文
        Value::String(text) => vec![text.clone()],
        Value::Array(items) => items
            .iter()
            .map(|item| json_string(item, &["text"]))
            .collect::<Option<Vec<_>>>()?,
        _ => return None,
    };

    if texts.is_empty()
        || !texts
            .iter()
            .all(|text| is_transport_message(text) || is_title_noise(text))
    {
        return None;
    }

    Some(diagnostic_block(
        "context_injection",
        texts.join("\n\n"),
        Some(payload.clone()),
    ))
}

pub(crate) fn is_transport_message(text: &str) -> bool {
    let trimmed = text.trim();

    trimmed.starts_with("<environment_context>")
        || trimmed.starts_with("<local-command-caveat>")
        || trimmed.starts_with("<turn_aborted>")
        || trimmed.starts_with("<permissions instructions>")
        || trimmed.starts_with("<collaboration_mode>")
        || trimmed.starts_with("<skills_instructions>")
        || trimmed.starts_with("<subagent_notification>")
}

pub(crate) fn is_title_noise(text: &str) -> bool {
    let trimmed = text.trim_start();

    if trimmed.is_empty() {
        return false;
    }

    let stripped = trimmed.trim_start_matches('#').trim_start();
    stripped.starts_with("AGENTS.md instructions for")
        || trimmed.starts_with("<system-reminder>")
        || trimmed.starts_with("</system-reminder>")
}

pub(crate) fn title_candidate_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim();

    if trimmed.is_empty() {
        return None;
    }

    if !is_title_noise(trimmed) && !is_transport_message(trimmed) {
        return Some(normalize_title(trimmed.to_string()));
    }

    // 带噪音前缀的消息（如 Codex 注入的 AGENTS.md 指令）复用消息展示的块级清洗，
    // 保证 <INSTRUCTIONS> 等块内容不会被逐行扫描误判为标题。
    let sanitized = sanitize_user_message_text(trimmed)?;

    sanitized
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| normalize_title(line.to_string()))
}

pub(crate) fn sanitize_user_message_text(text: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut skip_until: Option<&str> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();

        if let Some(end_tag) = skip_until {
            if line.starts_with(end_tag) {
                skip_until = None;
            }
            continue;
        }

        if line.is_empty() {
            lines.push(String::new());
            continue;
        }

        // 块起始标记必须先于单行噪音判断：<system-reminder> 等开标签本身
        // 也命中 is_title_noise/is_transport_message，顺序反了会导致 skip_until
        // 永远不生效、块内容泄漏出来。
        if line.starts_with("<INSTRUCTIONS>") {
            skip_until = Some("</INSTRUCTIONS>");
            continue;
        }

        if line.starts_with("<system-reminder>") {
            skip_until = Some("</system-reminder>");
            continue;
        }

        if line.starts_with("<environment_context>") {
            skip_until = Some("</environment_context>");
            continue;
        }

        if line.starts_with("<subagent_notification>") {
            skip_until = Some("</subagent_notification>");
            continue;
        }

        if is_title_noise(line) || is_transport_message(line) {
            continue;
        }

        if line.starts_with('<') && line.ends_with('>') {
            continue;
        }

        lines.push(raw_line.trim_end().to_string());
    }

    let sanitized = lines.join("\n");
    let trimmed = sanitized.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub(crate) fn sanitize_user_blocks(blocks: Vec<ContentBlock>) -> Vec<ContentBlock> {
    blocks
        .into_iter()
        .filter_map(|mut block| {
            block.text = block
                .text
                .take()
                .and_then(|text| sanitize_user_message_text(&text));

            (block.text.is_some()
                || matches!(
                    block.kind.as_str(),
                    "tool_use" | "tool_result" | "function_call" | "function_call_output"
                ))
            .then_some(block)
        })
        .collect()
}

pub(crate) fn normalize_title(text: String) -> String {
    let normalized = strip_markdown_links(&text)
        .replace('\n', " ")
        .trim()
        .to_string();

    if normalized.chars().count() <= 72 {
        return normalized;
    }

    normalized.chars().take(72).collect::<String>() + "..."
}

// 标题里剥掉 markdown 链接语法：[label](url) 只留 label。不剥的话
// 72 字符截断可能全落在链接语法上（如 [name](/very/long/path)），
// 标题里看不到任何实际文本。
fn strip_markdown_links(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(open) = rest.find('[') {
        let Some(label_end) = rest[open..].find("](") else {
            break;
        };
        let label_end = open + label_end;
        let Some(url_end) = rest[label_end + 2..].find(')') else {
            break;
        };
        let url_end = label_end + 2 + url_end;

        result.push_str(&rest[..open]);
        result.push_str(&rest[open + 1..label_end]);
        rest = &rest[url_end + 1..];
    }

    result.push_str(rest);
    result
}

use super::json_string;
