//! OpenCode 家族的 tool part 装配:opencode(v2 session_message 的 data)与
//! zcode(part 表的行)的 tool 块同构——state.input 是参数对象或字符串、
//! state.status 标记错误,差异只在字段名与输出承载,由 ToolShape 声明。
//! 输入/输出拆成两条 UI 块,与前端 function_call/function_call_output 的
//! 分组契约一致;输出文本提到 payload 顶层,前端 resultOutputText 直接读
//! payload.output。

use serde_json::Value;

use super::model::ContentBlock;

/// 一个来源 tool part 的形态差异。
pub(crate) struct ToolShape {
    /// tool 名的字段(zcode "tool",opencode "name")。
    pub(crate) tool_name_key: &'static str,
    /// call id 的字段(zcode "callID",opencode "id")。
    pub(crate) call_id_key: &'static str,
    /// state 内承载输出的键;输入块的 payload 要剥掉它,避免成倍变大。
    pub(crate) output_key: &'static str,
    /// 输出键取值到文本的提取(zcode 是字符串,opencode v2 是块数组)。
    pub(crate) extract_output: fn(&Value) -> Option<String>,
}

/// zcode:state.output 直接是字符串。
pub(crate) fn text_output(value: &Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

/// opencode v2:state.content 是块数组(通常为 {type:"text",text}),提取
/// 其中文本拼接;数组为空返回 None(running 中的工具),非文本块降级为
/// JSON 展示。
pub(crate) fn content_blocks_output(content: &Value) -> Option<String> {
    let items = content.as_array()?;
    let texts = items
        .iter()
        .filter_map(|item| super::json_string(item, &["text"]))
        .collect::<Vec<_>>();

    if texts.is_empty() {
        return (!items.is_empty())
            .then(|| super::stringify_json(content))
            .flatten();
    }

    Some(texts.join("\n"))
}

pub(crate) fn tool_blocks(value: &Value, shape: &ToolShape) -> Vec<ContentBlock> {
    let tool_name = super::json_string(value, &[shape.tool_name_key]);
    let tool_call_id = super::json_string(value, &[shape.call_id_key]);
    let state = value.get("state");
    let input = state.and_then(|state| state.get("input"));
    let output_text = state
        .and_then(|state| state.get(shape.output_key))
        .and_then(shape.extract_output);
    let is_error = state
        .and_then(|state| state.get("status"))
        .and_then(Value::as_str)
        == Some("error");
    let mut blocks = Vec::new();

    if input.is_some_and(|item| !item.is_null()) {
        let mut payload = value.clone();
        // 输出内容不重复放进输入块，避免 payload 成倍变大
        if let Some(state) = payload.get_mut("state").and_then(Value::as_object_mut) {
            state.remove(shape.output_key);
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
