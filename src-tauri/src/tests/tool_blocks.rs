// tool part 装配的形态测试:两种 ToolShape 共用同一装配体,断言字段差异
// 由 shape 承载、payload 手术(输入块剥输出、输出提到顶层)与前端契约。

use serde_json::json;

use crate::session::tool_blocks::{ToolShape, content_blocks_output, text_output, tool_blocks};

const ZCODE: ToolShape = ToolShape {
    tool_name_key: "tool",
    call_id_key: "callID",
    output_key: "output",
    extract_output: text_output,
};

const OPENCODE: ToolShape = ToolShape {
    tool_name_key: "name",
    call_id_key: "id",
    output_key: "content",
    extract_output: content_blocks_output,
};

#[test]
fn zcode_shape_splits_input_and_string_output() {
    let part = json!({
        "type": "tool",
        "tool": "read_file",
        "callID": "call-1",
        "state": {
            "status": "completed",
            "input": {"path": "a.rs"},
            "output": "file body"
        }
    });

    let blocks = tool_blocks(&part, &ZCODE);
    assert_eq!(blocks.len(), 2);

    assert_eq!(blocks[0].kind, "function_call");
    assert_eq!(blocks[0].tool_name.as_deref(), Some("read_file"));
    assert_eq!(blocks[0].tool_call_id.as_deref(), Some("call-1"));
    assert!(blocks[0].text.as_deref().unwrap().contains("a.rs"));
    // 输入块 payload 剥掉输出,不重复放大。
    assert!(
        blocks[0].payload.as_ref().unwrap()["state"]
            .get("output")
            .is_none()
    );

    assert_eq!(blocks[1].kind, "function_call_output");
    assert_eq!(blocks[1].text.as_deref(), Some("file body"));
    // 输出提到顶层,前端 resultOutputText 直接读 payload.output。
    assert_eq!(
        blocks[1].payload.as_ref().unwrap()["output"].as_str(),
        Some("file body")
    );
    assert_eq!(blocks[1].is_error, None);
}

#[test]
fn opencode_shape_joins_content_blocks() {
    let part = json!({
        "type": "tool",
        "name": "bash",
        "id": "call-2",
        "state": {
            "status": "error",
            "input": "cargo test",
            "content": [
                {"type": "text", "text": "line 1"},
                {"type": "text", "text": "line 2"}
            ]
        }
    });

    let blocks = tool_blocks(&part, &OPENCODE);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].tool_name.as_deref(), Some("bash"));
    assert_eq!(blocks[0].text.as_deref(), Some("cargo test"));
    assert_eq!(blocks[1].text.as_deref(), Some("line 1\nline 2"));
    assert_eq!(blocks[1].is_error, Some(true));
    // 输入块剥掉的是 content 键。
    assert!(
        blocks[0].payload.as_ref().unwrap()["state"]
            .get("content")
            .is_none()
    );
}

#[test]
fn running_tool_without_output_has_input_block_only() {
    let part = json!({
        "name": "bash",
        "id": "call-3",
        "state": {
            "status": "running",
            "input": "cargo build",
            "content": []
        }
    });

    let blocks = tool_blocks(&part, &OPENCODE);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].kind, "function_call");
}

#[test]
fn non_text_content_falls_back_to_json() {
    let content = json!([{"type": "image", "url": "file://x.png"}]);
    assert!(
        content_blocks_output(&content)
            .as_deref()
            .is_some_and(|text| text.contains("image"))
    );
}
