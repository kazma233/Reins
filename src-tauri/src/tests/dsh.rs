use super::*;

// dsh fixture:转录事件结构对应 ~/.dsh/sessions 的实测样本,事件 schema 见
// aidocs/context/2026-10-07-dsh-session-storage-exploration.md。
// 三帧拼接的 zstd(帧1=header+permission,帧2=user 消息,帧3=assistant+usage),
// 由 Node zlib.zstdCompressSync 分段压缩生成;解出内容见下方断言。
const ZSTD_MULTIFRAME: &[u8] = &[
    0x28, 0xb5, 0x2f, 0xfd, 0x20, 0xfb, 0x75, 0x05, 0x00, 0x72, 0xcb, 0x23, 0x20, 0x50, 0x6b, 0xd3,
    0x06, 0xc0, 0x2a, 0xb3, 0x14, 0x09, 0x62, 0xe4, 0x46, 0x12, 0x11, 0xa7, 0x66, 0x9c, 0xfd, 0x08,
    0x6c, 0x0d, 0xeb, 0xef, 0x15, 0xcd, 0x6f, 0xf9, 0x14, 0x33, 0xab, 0x39, 0x4f, 0x87, 0xe7, 0xf1,
    0x17, 0x2d, 0x07, 0x71, 0x05, 0x95, 0x26, 0xac, 0xc6, 0x51, 0x7e, 0x9c, 0x71, 0x31, 0x6d, 0xc9,
    0x19, 0xb4, 0x36, 0xe4, 0xac, 0x70, 0xb5, 0xa9, 0x84, 0x22, 0x66, 0x5d, 0x1d, 0x1e, 0xe7, 0xc2,
    0x4c, 0xb8, 0x7b, 0xfd, 0x22, 0xd8, 0x74, 0x07, 0xda, 0x90, 0xf3, 0xc1, 0xaa, 0x33, 0x52, 0x83,
    0x16, 0x1d, 0x5f, 0x9e, 0xb6, 0xeb, 0x82, 0x06, 0xe7, 0x7c, 0xfe, 0x05, 0x8a, 0x6c, 0x46, 0x68,
    0x96, 0x6a, 0x34, 0x8e, 0xe1, 0x28, 0x36, 0x64, 0xa1, 0x50, 0x00, 0x24, 0x02, 0x8b, 0xe4, 0xbc,
    0x03, 0xf9, 0xc5, 0x2f, 0xc5, 0x66, 0xb4, 0xcb, 0x02, 0x83, 0x48, 0x64, 0x0b, 0xb8, 0x75, 0x06,
    0x9b, 0x53, 0x83, 0x54, 0xaf, 0x3e, 0x5f, 0x35, 0xac, 0x1f, 0x01, 0x0a, 0x00, 0x59, 0x10, 0x09,
    0xc3, 0x48, 0xb5, 0x14, 0x71, 0xc6, 0x6c, 0x01, 0x59, 0xa1, 0x8d, 0x43, 0xc0, 0xa9, 0x82, 0x84,
    0x0b, 0xce, 0xe7, 0x16, 0x6d, 0x4b, 0x13, 0x28, 0xb5, 0x2f, 0xfd, 0x20, 0xc8, 0x9d, 0x04, 0x00,
    0xb2, 0x09, 0x1f, 0x1f, 0x60, 0x49, 0xab, 0x03, 0x06, 0x8b, 0x4a, 0xd9, 0x62, 0xa4, 0x9a, 0x61,
    0x31, 0x2c, 0xdb, 0x4d, 0x06, 0x7e, 0x4c, 0xc1, 0xd0, 0xfe, 0x05, 0xec, 0x1c, 0x8d, 0x04, 0x08,
    0x58, 0x10, 0x27, 0x85, 0xc6, 0xac, 0xfd, 0x3d, 0xe5, 0xf4, 0x0b, 0xf8, 0x52, 0x88, 0x46, 0x18,
    0x93, 0x1b, 0x38, 0xf5, 0x69, 0x9c, 0xd4, 0x69, 0x06, 0xb0, 0x9e, 0xfa, 0x98, 0xb5, 0xa4, 0xe0,
    0x4b, 0xa3, 0x83, 0x3e, 0x03, 0x8d, 0x17, 0x3e, 0x2e, 0xd7, 0xc2, 0x1d, 0x6c, 0x71, 0x45, 0x29,
    0xe2, 0x3c, 0x5e, 0x84, 0xaf, 0x40, 0x7a, 0xdb, 0xb7, 0xb9, 0x65, 0x26, 0xad, 0x56, 0x31, 0x9f,
    0x03, 0x42, 0x08, 0x1d, 0x86, 0x81, 0x48, 0x89, 0x23, 0xa5, 0xaf, 0x24, 0xeb, 0x93, 0xd2, 0xcb,
    0xa1, 0x8f, 0x4f, 0xa0, 0x20, 0xbc, 0x62, 0xd0, 0xc3, 0xe8, 0xf4, 0x5f, 0xc2, 0x9a, 0x09, 0x07,
    0x00, 0x40, 0x84, 0x67, 0xb8, 0x7f, 0x36, 0xfb, 0x31, 0x89, 0x04, 0xb6, 0x4e, 0x35, 0xea, 0x50,
    0x8a, 0x60, 0x06, 0x28, 0xb5, 0x2f, 0xfd, 0x60, 0x8a, 0x00, 0xcd, 0x07, 0x00, 0xd6, 0x50, 0x32,
    0x21, 0x40, 0x8b, 0xda, 0x06, 0x38, 0xcb, 0x45, 0xab, 0x2e, 0x6a, 0x62, 0x30, 0x4c, 0xd4, 0x6e,
    0x41, 0x5f, 0x27, 0xac, 0x57, 0x39, 0x4c, 0x01, 0xfd, 0x58, 0x1c, 0x7e, 0x33, 0xab, 0xe2, 0xa8,
    0xcb, 0x01, 0x29, 0x00, 0x27, 0x00, 0x29, 0x00, 0xac, 0x1f, 0xc1, 0x67, 0xe7, 0xf9, 0x29, 0x92,
    0x24, 0x49, 0x18, 0x8b, 0x85, 0xc2, 0x0c, 0x30, 0x0c, 0x5e, 0x17, 0xd4, 0xbf, 0x04, 0x7e, 0x41,
    0xf9, 0x77, 0x6d, 0x24, 0xb3, 0x2e, 0x01, 0xbd, 0xd9, 0x8c, 0x98, 0xf3, 0x78, 0x55, 0x0f, 0xfd,
    0x48, 0xaa, 0xcb, 0xd1, 0x08, 0x5d, 0x72, 0x29, 0x5c, 0xfb, 0x63, 0x91, 0xa3, 0x8f, 0x6f, 0x07,
    0xed, 0xdf, 0x1b, 0x8b, 0x36, 0x71, 0xea, 0x42, 0xc0, 0xf7, 0x6a, 0x6f, 0x52, 0x7e, 0x87, 0xa4,
    0xf5, 0x08, 0x2a, 0x6d, 0xfe, 0x19, 0xfc, 0xae, 0x8f, 0x64, 0xd0, 0x4c, 0x2d, 0x6a, 0x24, 0x33,
    0x93, 0xe8, 0xd7, 0xbe, 0x1c, 0xef, 0xea, 0xf8, 0xa5, 0x07, 0x31, 0xa4, 0xa5, 0x7e, 0x87, 0xe8,
    0x49, 0xcb, 0xf1, 0x67, 0xd1, 0x43, 0x59, 0x2c, 0xf1, 0x2f, 0xa1, 0x7d, 0x69, 0x03, 0x6f, 0x22,
    0x02, 0x00, 0xed, 0x67, 0x55, 0xa9, 0x3c, 0x5e, 0x11, 0xa8, 0x92, 0x0a, 0x8b, 0xf1, 0xaf, 0x35,
    0x81, 0x04, 0x24, 0x3b, 0xf5, 0x6f, 0xa1, 0x8d, 0xd6, 0x81, 0xe2, 0xa4, 0x16, 0xa8, 0x43, 0x49,
    0xe5, 0xc7, 0x68, 0x96, 0x7e, 0xc6, 0xe0, 0x79, 0x35, 0x11, 0x00, 0x33, 0x09, 0xc7, 0x49, 0xf4,
    0x66, 0x66, 0x49, 0x11, 0xaa, 0xc6, 0xae, 0x80, 0x2f, 0xbb, 0x18, 0x66, 0xca, 0xf2, 0x74, 0xd2,
    0xc1, 0xe3, 0xe6, 0x42, 0x29, 0xec, 0x6c, 0xe0, 0xa3, 0x7a, 0xfc, 0xa3, 0xf8, 0x60, 0x9c, 0x22,
    0x02, 0xc1, 0xc3, 0xab, 0x98, 0x01,
];

fn dsh_test_home() -> Result<(PathBuf, TestEnvGuard)> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let guard = TestEnvGuard::set_home(&temp_home);
    Ok((temp_home, guard))
}

fn session_header(id: &str, created_at: i64, delegation_depth: u32) -> Value {
    json!({
        "type": "session",
        "version": 4,
        "id": id,
        "createdAt": created_at,
        "cwd": "D:\\projects\\demo",
        "isSeeded": false,
        "delegationDepth": delegation_depth,
        "agentPreset": "standard",
    })
}

fn event(seq: i64, time: i64, kind: &str, data: Value) -> Value {
    json!({"type": kind, "seq": seq, "time": time, "data": data})
}

fn user_message(seq: i64, time: i64, text: &str, source_kind: &str) -> Value {
    with_surface_append(event(
        seq,
        time,
        "user/message",
        json!({
            "content": [{"type": "text", "text": text}],
            "source": {"kind": source_kind, "rpcId": "rpc-1"},
            "role": "user",
            "id": format!("user-{seq}"),
        }),
    ))
}

fn surface_replace(seq: i64, time: i64, kind: &str, data: Value, start: i64, end: i64) -> Value {
    let mut value = event(seq, time, kind, data);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "surfaceOp".to_string(),
            json!({"op": "replace", "startSeq": start, "endSeq": end}),
        );
    }
    value
}

fn assistant_message(seq: i64, time: i64, content: Vec<Value>, usage: Value) -> Value {
    with_surface_append(event(
        seq,
        time,
        "assistant/message",
        json!({
            "turn": 1,
            "step": 1,
            "message": {
                "role": "assistant",
                "content": content,
                "source": {"kind": "model", "provider": "deepseek-account", "model": "deepseek-flash"},
                "id": format!("assistant-{seq}"),
            },
            "usage": usage,
        }),
    ))
}

fn tool_result(seq: i64, time: i64, call_id: &str, text: &str, is_error: bool) -> Value {
    with_surface_append(event(
        seq,
        time,
        "tool/result",
        json!({
            "turn": 1,
            "step": 1,
            "message": {
                "role": "tool",
                "source": {"kind": "tool", "callId": call_id},
                "toolCallId": call_id,
                "content": [{"type": "text", "text": text}],
                "isError": is_error,
                "id": format!("tool-result-{seq}"),
            },
        }),
    ))
}

// surface 事件统一补 surfaceOp:"append"
fn with_surface_append(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert("surfaceOp".to_string(), json!("append"));
    }
    value
}

fn write_session(
    home: &Path,
    session_id: &str,
    generation: u32,
    compressed: bool,
    lines: &[Value],
) -> Result<PathBuf> {
    let directory = home
        .join(".dsh/sessions/--D-projects-demo--")
        .join(session_id);
    fs::create_dir_all(&directory)?;
    let file_name = if compressed {
        format!("session.v{generation}.jsonl.zstd")
    } else {
        format!("session.v{generation}.jsonl")
    };
    let path = directory.join(file_name);
    write_jsonl(&path, lines)?;
    Ok(path)
}

#[test]
fn dsh_decodes_multiframe_zstd_and_picks_highest_generation() -> Result<()> {
    let (home, _guard) = dsh_test_home()?;
    let directory = home
        .join(".dsh/sessions/--D-projects-demo--")
        .join("session-multi");
    fs::create_dir_all(&directory)?;
    // 旧代 v3 明文共存,读取必须取最高代 v4 的多帧 zstd
    write_jsonl(
        &directory.join("session.v3.jsonl"),
        &[session_header("session-multi", 1791355600000, 0)],
    )?;
    fs::write(&directory.join("session.v4.jsonl.zstd"), ZSTD_MULTIFRAME)?;

    let reader = session::reader(SourceApp::Dsh);
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    let path = &entries[0].path;

    let summary = reader.parse_summary(path)?;
    assert_eq!(summary.source_app, SourceApp::Dsh);
    assert_eq!(summary.source_session_id, "session-multi");
    // 多帧里的第二条 user 消息是标题来源;created=header.createdAt,updated=最后事件 time
    assert_eq!(summary.title, "frame two request");
    assert_eq!(summary.created_at, Some(1791355700000));
    assert_eq!(summary.updated_at, Some(1791355700003));
    // assistant/message 的 usage 字段 1:1 映射
    let usage = summary.token_usage.expect("usage present");
    assert_eq!(
        (
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens,
            usage.cache_write_tokens
        ),
        (11, 7, 3, 5)
    );

    let detail = read_detail(reader, path)?;
    // 三个帧的内容全部解出:帧1 只有 header+permission(无消息),帧2/帧3 各一条消息
    assert!(detail.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("frame two request"))
    }));
    let assistant = detail
        .messages
        .iter()
        .find(|message| message.role == "assistant")
        .expect("assistant message from frame three");
    assert!(
        assistant
            .blocks
            .iter()
            .any(|block| block.kind == "thinking" && block.text.as_deref() == Some("think"))
    );
    assert!(
        assistant.blocks.iter().any(
            |block| block.kind == "text" && block.text.as_deref() == Some("frame three answer")
        )
    );
    // permission/preset 走事件时间线
    let permission = detail
        .events
        .iter()
        .find(|event| event.kind == "permission/preset")
        .expect("permission event");
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.kind == "permission/preset")
    );
    // raw payload 给整条记录（含 seq/type 信封），不是只给 data
    let payload = permission.payload.as_ref().expect("raw payload");
    assert_eq!(
        payload.get("type").and_then(Value::as_str),
        Some("permission/preset")
    );
    assert!(payload.get("seq").is_some());
    assert!(payload.get("data").is_some());
    fs::remove_dir_all(&home).ok();
    Ok(())
}

#[test]
fn dsh_tolerates_truncated_zstd_tail() -> Result<()> {
    let (home, _guard) = dsh_test_home()?;
    let directory = home
        .join(".dsh/sessions/--D-projects-demo--")
        .join("session-live");
    fs::create_dir_all(&directory)?;
    // 截 420 字节:帧1+帧2 完整,帧3 从中间截断(live 写入中的文件)
    fs::write(
        &directory.join("session.v4.jsonl.zstd"),
        &ZSTD_MULTIFRAME[..420],
    )?;

    let reader = session::reader(SourceApp::Dsh);
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    let detail = read_detail(reader, &entries[0].path)?;
    assert!(detail.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("frame two request"))
    }));
    assert!(
        !detail
            .messages
            .iter()
            .any(|message| message.blocks.iter().any(|block| block
                .text
                .as_deref()
                .is_some_and(|text| text.contains("frame three"))))
    );
    fs::remove_dir_all(&home).ok();
    Ok(())
}

fn mapping_fixture(id: &str) -> Vec<Value> {
    let mut lines = vec![session_header(id, 1000, 0)];
    let mut seq = 0;
    for kind in ["permission/preset", "sandbox/mode", "approval/policy"] {
        lines.push(event(seq, 1001, kind, json!({"value": "workspace-write"})));
        seq += 1;
    }
    lines.push(with_surface_append(event(
        seq,
        1002,
        "system/message",
        json!({"message": {"role": "system", "content": [
            {"type": "text", "text": "You are an AI agent powered by DeepSeek Harness."}
        ]}}),
    )));
    seq += 1;
    lines.push(user_message(seq, 1003, "帮我分析这个项目", "user"));
    seq += 1;
    lines.push(user_message(
        seq,
        1004,
        "AGENTS.md instructions for this workspace, keep it short.",
        "agent-instructions",
    ));
    seq += 1;
    lines.push(user_message(
        seq,
        1005,
        "Current runtime context. Snapshot of sandbox policy.",
        "runtime-context",
    ));
    seq += 1;
    // request/header 带全量工具定义,量最大且在忽略列表
    lines.push(event(
        seq,
        1006,
        "request/header",
        json!({
            "header": {"config": {"provider": "deepseek-account", "model": "deepseek-flash"},
                       "tools": [{"name": "pwsh", "parameters": {"type": "object"}}]}
        }),
    ));
    seq += 1;
    lines.push(event(
        seq,
        1007,
        "request/context",
        json!({"provider": "deepseek-account", "model": "deepseek-flash", "contextWindow": 1000000}),
    ));
    seq += 1;
    lines.push(event(
        seq,
        1008,
        "tool/call",
        json!({
            "callId": "call_1", "name": "pwsh", "arguments": "{\"command\": \"ls\"}"
        }),
    ));
    seq += 1;
    lines.push(assistant_message(
        seq,
        1009,
        vec![
            json!({"type": "reasoning", "text": ""}),
            json!({"type": "reasoning", "text": "先想想目录结构"}),
            json!({"type": "text", "text": "开始分析"}),
            json!({"type": "tool-call", "id": "call_1", "name": "pwsh", "arguments": "{\"command\": \"ls\"}"}),
        ],
        json!({"inputTokens": 100, "outputTokens": 10, "cacheReadTokens": 20, "cacheWriteTokens": 5, "totalTokens": 135}),
    ));
    seq += 1;
    lines.push(tool_result(seq, 1010, "call_1", "file-a\nfile-b", false));
    seq += 1;
    lines.push(tool_result(seq, 1011, "call_2", "Error: boom", true));
    seq += 1;
    // 纯噪音/重复:全部忽略
    for kind in [
        "step/start",
        "step/end",
        "turn/start",
        "agent/inbox/spliced",
        "session/title-llm-request",
    ] {
        lines.push(event(seq, 1012, kind, json!({"turn": 1, "step": 1})));
        seq += 1;
    }
    lines.push(event(
        seq,
        1013,
        "session-log-deepseek/delivery-accepted",
        json!({"sessionId": id, "throughSeq": seq}),
    ));
    seq += 1;
    lines.push(event(seq, 1014, "llm/retry-attempt", json!({"attempt": 1})));
    seq += 1;
    lines.push(event(
        seq,
        1015,
        "turn/end",
        json!({"turn": 1, "reason": {"kind": "completed"}}),
    ));
    seq += 1;
    // 未知 type:ignorable 才跳过,否则降级为事件
    lines.push(event(
        seq,
        1016,
        "future/experimental",
        json!({"detail": "skip me"}),
    ));
    lines
        .last_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("ignorable".to_string(), json!(true));
    seq += 1;
    lines.push(event(
        seq,
        1017,
        "future/unknown",
        json!({"detail": "show me"}),
    ));
    seq += 1;
    lines.push(event(
        seq,
        1018,
        "session/title",
        json!({"title": "Initial title", "source": {"kind": "fallback"}}),
    ));
    seq += 1;
    lines.push(event(
        seq,
        1019,
        "session/title",
        json!({"title": "Final title", "source": {"kind": "provider"}}),
    ));
    seq += 1;
    lines.push(assistant_message(
        seq,
        1020,
        vec![
            json!({"type": "reasoning", "text": "再想想"}),
            json!({"type": "text", "text": "总结完成"}),
        ],
        json!({"inputTokens": 50, "outputTokens": 5, "cacheReadTokens": 0, "cacheWriteTokens": 0, "totalTokens": 55}),
    ));
    lines
}

#[test]
fn dsh_maps_event_types_to_messages_events_and_usage() -> Result<()> {
    let (home, _guard) = dsh_test_home()?;
    let path = write_session(
        &home,
        "session-map",
        4,
        false,
        &mapping_fixture("session-map"),
    )?;

    let reader = session::reader(SourceApp::Dsh);
    let summary = reader.parse_summary(&path)?;
    // session/title latest-wins
    assert_eq!(summary.title, "Final title");
    // usage 两轮累计,字段 1:1 无换算
    let usage = summary.token_usage.expect("usage present");
    assert_eq!(
        (
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens,
            usage.cache_write_tokens
        ),
        (150, 15, 20, 5)
    );

    let detail = read_detail(reader, &path)?;

    // 人类 user 消息进消息时间线;合成注入不进
    let user = detail
        .messages
        .iter()
        .find(|message| message.role == "user")
        .expect("human user message");
    assert!(
        user.blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("帮我分析这个项目"))
    );
    assert_eq!(
        detail
            .messages
            .iter()
            .filter(|message| message.role == "user")
            .count(),
        1
    );

    // assistant:空 reasoning 跳过、非空→thinking、text→text、tool-call→function_call
    let first_assistant = detail
        .messages
        .iter()
        .find(|message| message.role == "assistant")
        .expect("assistant message");
    assert!(
        first_assistant.blocks.iter().any(
            |block| block.kind == "thinking" && block.text.as_deref() == Some("先想想目录结构")
        )
    );
    assert_eq!(
        first_assistant
            .blocks
            .iter()
            .filter(|block| block.kind == "thinking")
            .count(),
        1
    );
    let call = first_assistant
        .blocks
        .iter()
        .find(|block| block.kind == "function_call")
        .expect("tool call block");
    assert_eq!(call.tool_name.as_deref(), Some("pwsh"));
    assert_eq!(call.tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(call.text.as_deref(), Some("{\"command\": \"ls\"}"));
    assert_eq!(
        call.payload
            .as_ref()
            .and_then(|payload| payload.get("input")),
        Some(&json!({"command": "ls"}))
    );

    // tool/result:两条 role=tool 消息,错误标记透出
    let outputs: Vec<_> = detail
        .messages
        .iter()
        .filter(|message| message.role == "tool")
        .collect();
    assert_eq!(outputs.len(), 2);
    let failed = outputs
        .iter()
        .find_map(|message| {
            message
                .blocks
                .iter()
                .find(|block| block.tool_call_id.as_deref() == Some("call_2"))
        })
        .expect("failed tool result");
    assert_eq!(failed.is_error, Some(true));
    assert_eq!(failed.text.as_deref(), Some("Error: boom"));
    assert!(
        failed
            .payload
            .as_ref()
            .is_some_and(
                |payload| payload.get("toolCallId").and_then(Value::as_str) == Some("call_2")
            )
    );

    // 合成注入/系统消息转事件,summary 取首个 text 块截断
    let instructions = detail
        .events
        .iter()
        .find(|event| event.kind == "agent-instructions")
        .expect("agent-instructions event");
    assert!(instructions.summary.contains("AGENTS.md instructions"));
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.kind == "runtime-context")
    );
    let system = detail
        .events
        .iter()
        .find(|event| event.kind == "system/message")
        .expect("system message event");
    assert!(system.summary.contains("AI agent powered"));

    // 通用事件在时间线;忽略项与 ignorable 未知项不在任何时间线
    for expected in [
        "request/context",
        "turn/end",
        "future/unknown",
        "permission/preset",
    ] {
        assert!(
            detail.events.iter().any(|event| event.kind == expected),
            "missing event {expected}"
        );
    }
    for ignored in [
        "request/header",
        "tool/call",
        "step/start",
        "step/end",
        "turn/start",
        "agent/inbox/spliced",
        "session/title-llm-request",
        "session-log-deepseek/delivery-accepted",
        "llm/retry-attempt",
        "future/experimental",
    ] {
        assert!(
            !detail.events.iter().any(|event| event.kind == ignored),
            "ignored type {ignored} leaked into timeline"
        );
        assert!(
            !detail
                .messages
                .iter()
                .any(|message| message.blocks.iter().any(|block| block.kind == ignored)),
            "ignored type {ignored} leaked into messages"
        );
    }
    fs::remove_dir_all(&home).ok();
    Ok(())
}

#[test]
fn dsh_title_falls_back_to_first_user_message_then_session_id() -> Result<()> {
    let (home, _guard) = dsh_test_home()?;
    write_session(
        &home,
        "session-titled",
        4,
        false,
        &[
            session_header("session-titled", 1000, 0),
            user_message(0, 1100, "看看这个项目的结构", "user"),
            assistant_message(
                1,
                1200,
                vec![json!({"type": "text", "text": "done"})],
                json!({"inputTokens": 1, "outputTokens": 1, "cacheReadTokens": 0, "cacheWriteTokens": 0}),
            ),
        ],
    )?;
    write_session(
        &home,
        "session-empty",
        4,
        false,
        &[session_header("session-empty", 2000, 0)],
    )?;

    let reader = session::reader(SourceApp::Dsh);
    let summaries: Vec<SessionSummary> = reader
        .list_entries()?
        .into_iter()
        .map(|entry| entry.summary.expect("summary present"))
        .collect();
    let titled = summaries
        .iter()
        .find(|summary| summary.source_session_id == "session-titled")
        .expect("titled session");
    assert_eq!(titled.title, "看看这个项目的结构");
    let empty = summaries
        .iter()
        .find(|summary| summary.source_session_id == "session-empty")
        .expect("empty session");
    assert_eq!(empty.title, "session-empty");
    assert_eq!(empty.created_at, Some(2000));
    fs::remove_dir_all(&home).ok();
    Ok(())
}

#[test]
fn dsh_surface_replace_folds_covered_range() -> Result<()> {
    let (home, _guard) = dsh_test_home()?;
    let lines = vec![
        session_header("session-fold", 1000, 0),
        user_message(1, 1100, "原始问题", "user"),
        assistant_message(
            2,
            1200,
            vec![json!({"type": "text", "text": "原始回答"})],
            json!({"inputTokens": 10, "outputTokens": 2, "cacheReadTokens": 0, "cacheWriteTokens": 0}),
        ),
        tool_result(3, 1250, "call_1", "原始工具输出", false),
        surface_replace(
            4,
            1300,
            "user/message",
            json!({
                "content": [{"type": "text", "text": "压缩后的摘要"}],
                "source": {"kind": "user"},
                "role": "user",
                "id": "user-fold",
            }),
            1,
            3,
        ),
    ];
    let path = write_session(&home, "session-fold", 4, false, &lines)?;

    let reader = session::reader(SourceApp::Dsh);
    let detail = read_detail(reader, &path)?;
    let message_texts: Vec<&str> = detail
        .messages
        .iter()
        .flat_map(|message| {
            message
                .blocks
                .iter()
                .filter_map(|block| block.text.as_deref())
        })
        .collect();
    assert!(message_texts.contains(&"压缩后的摘要"));
    for folded in ["原始问题", "原始回答", "原始工具输出"] {
        assert!(
            !message_texts.contains(&folded),
            "replaced node {folded} must be folded away"
        );
    }
    // 折叠不影响 usage 统计(消耗是计费事实)
    let usage = reader.parse_summary(&path)?.token_usage.expect("usage");
    assert_eq!((usage.input_tokens, usage.output_tokens), (10, 2));
    fs::remove_dir_all(&home).ok();
    Ok(())
}

#[test]
fn dsh_family_folds_subagents_and_hides_orphans() -> Result<()> {
    let (home, _guard) = dsh_test_home()?;
    write_session(
        &home,
        "session-root",
        4,
        false,
        &[
            session_header("session-root", 1000, 0),
            user_message(0, 1100, "主任务", "user"),
            event(
                1,
                1150,
                "subagent/catalog",
                json!({
                    "childId": "session-child",
                    "childCreatedAt": 1200,
                    "version": 1,
                    "mode": "one-shot",
                    "label": "Research task",
                }),
            ),
            assistant_message(
                2,
                1300,
                vec![json!({"type": "text", "text": "主会话回答"})],
                json!({"inputTokens": 100, "outputTokens": 20, "cacheReadTokens": 0, "cacheWriteTokens": 0}),
            ),
        ],
    )?;
    write_session(
        &home,
        "session-child",
        4,
        false,
        &[
            session_header("session-child", 1200, 1),
            user_message(0, 1250, "子任务请求", "user"),
            assistant_message(
                1,
                1350,
                vec![json!({"type": "text", "text": "子任务完成"})],
                json!({"inputTokens": 30, "outputTokens": 8, "cacheReadTokens": 5, "cacheWriteTokens": 0}),
            ),
        ],
    )?;
    // 孤儿子会话:无 catalog 声明,隐藏
    write_session(
        &home,
        "session-orphan",
        4,
        false,
        &[
            session_header("session-orphan", 1500, 1),
            user_message(0, 1550, "孤儿消息", "user"),
        ],
    )?;

    let reader = session::reader(SourceApp::Dsh);
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1, "root family is the only entry");
    let root_path = &entries[0].path;

    let summary = reader.parse_summary(root_path)?;
    assert_eq!(summary.source_session_id, "session-root");
    assert!(summary.title.contains("主任务 (+1 subagents)"));
    // family 用量 = root + child 之和
    let usage = summary.token_usage.expect("usage");
    assert_eq!(
        (
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens,
            usage.cache_write_tokens
        ),
        (130, 28, 5, 0)
    );

    let overview = reader.parse_overview(root_path)?;
    assert_eq!(overview.agents.len(), 2);
    assert!(
        overview
            .agents
            .iter()
            .any(|agent| agent.session_id == "session-child"
                && agent.label.contains("Research task"))
    );
    // catalog 的 label 优先作为 marker 标题
    assert_eq!(overview.summary.title, summary.title);

    let detail = read_detail(reader, root_path)?;
    let marker = detail
        .messages
        .iter()
        .find(|message| message.id == "dsh-subagent-session-child")
        .expect("subagent marker message");
    assert!(
        marker
            .blocks
            .iter()
            .any(|block| block.payload.as_ref().is_some_and(|payload| {
                payload.get("type").and_then(Value::as_str) == Some("subagent_started")
            }))
    );
    assert!(detail.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("子任务完成"))
    }));
    assert!(
        !detail
            .messages
            .iter()
            .any(|message| message.blocks.iter().any(|block| block
                .text
                .as_deref()
                .is_some_and(|text| text.contains("孤儿消息"))))
    );

    // 子代理入口按 agent session id 取消息;孤儿不属于任何 family
    let agent_messages = reader.parse_agent_messages(root_path, "session-child")?;
    assert!(
        agent_messages
            .iter()
            .any(|message| message.blocks.iter().any(|block| block
                .text
                .as_deref()
                .is_some_and(|text| text.contains("子任务"))))
    );
    assert!(
        reader
            .parse_agent_messages(root_path, "session-orphan")
            .is_err()
    );

    // resolve_path 反查子会话转录
    let child_path = reader.resolve_path("session-child")?;
    assert!(child_path.ends_with("session.v4.jsonl"));
    assert!(child_path.to_string_lossy().contains("session-child"));
    assert!(reader.resolve_path("session-orphan").is_err());
    fs::remove_dir_all(&home).ok();
    Ok(())
}

#[test]
fn dsh_usage_hours_bucketed_by_assistant_event_time() -> Result<()> {
    let (home, _guard) = dsh_test_home()?;
    let hour = |h: u32| {
        chrono::TimeZone::with_ymd_and_hms(&chrono::Local, 2026, 4, 21, h, 0, 0)
            .single()
            .expect("valid local timestamp")
            .timestamp_millis()
    };
    let expected_key = |ts: i64| {
        chrono::DateTime::from_timestamp_millis(ts)
            .expect("valid timestamp")
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%dT%H")
            .to_string()
    };
    let morning = hour(10);
    let afternoon = hour(14);
    let path = write_session(
        &home,
        "session-hours",
        4,
        false,
        &[
            session_header("session-hours", 1000, 0),
            assistant_message(
                0,
                morning,
                vec![json!({"type": "text", "text": "morning"})],
                json!({"inputTokens": 100, "outputTokens": 10, "cacheReadTokens": 20, "cacheWriteTokens": 0}),
            ),
            assistant_message(
                1,
                afternoon,
                vec![json!({"type": "text", "text": "afternoon"})],
                json!({"inputTokens": 50, "outputTokens": 5, "cacheReadTokens": 0, "cacheWriteTokens": 0}),
            ),
        ],
    )?;

    let buckets = session::dsh::usage_hours(&path)?;
    assert_eq!(
        buckets,
        BTreeMap::from([
            (
                expected_key(morning),
                session::model::SessionTokenUsage {
                    input_tokens: 100,
                    output_tokens: 10,
                    cache_read_tokens: 20,
                    cache_write_tokens: 0,
                }
            ),
            (
                expected_key(afternoon),
                session::model::SessionTokenUsage {
                    input_tokens: 50,
                    output_tokens: 5,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                }
            ),
        ])
    );
    fs::remove_dir_all(&home).ok();
    Ok(())
}

#[test]
fn dsh_delete_session_is_unsupported() {
    assert!(session::delete_session(SourceApp::Dsh, Path::new("ignored")).is_err());
}
