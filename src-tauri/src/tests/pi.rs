use super::*;

fn pi_path(home: &Path, cwd: &str, timestamp: &str, id: &str) -> PathBuf {
    let encoded = format!(
        "--{}--",
        cwd.trim_start_matches('/').replace(['/', '\\', ':'], "-")
    );
    home.join(".pi/agent/sessions").join(encoded).join(format!(
        "{}_{}.jsonl",
        timestamp.replace([':', '.'], "-"),
        id
    ))
}

fn pi_header(id: &str, cwd: &str, timestamp: &str) -> Value {
    json!({
        "type": "session",
        "version": 3,
        "id": id,
        "timestamp": timestamp,
        "cwd": cwd,
    })
}

fn pi_entry(kind: &str, id: &str, parent_id: Option<&str>, timestamp: &str, extra: Value) -> Value {
    let mut entry = json!({
        "type": kind,
        "id": id,
        "parentId": parent_id,
        "timestamp": timestamp,
    });
    if let (Some(target), Some(source)) = (entry.as_object_mut(), extra.as_object()) {
        target.extend(source.clone());
    }
    entry
}

#[test]
fn pi_detects_valid_headers_and_ignores_invalid_files() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let older = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T09-00-00-000Z",
        "pi-old",
    );
    let newer = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T10-00-00-000Z",
        "pi-new",
    );
    write_jsonl(
        &older,
        &[
            pi_header("pi-old", "/tmp/pi-project", "2026-09-04T09:00:00.000Z"),
            pi_entry(
                "message",
                "old-msg",
                None,
                "2026-09-04T09:00:01.000Z",
                json!({
                    "message": { "role": "user", "content": "old" }
                }),
            ),
        ],
    )?;
    write_jsonl(
        &newer,
        &[
            pi_header("pi-new", "/tmp/pi-project", "2026-09-04T10:00:00.000Z"),
            pi_entry(
                "message",
                "new-msg",
                None,
                "2026-09-04T10:00:01.000Z",
                json!({
                    "message": { "role": "user", "content": "new" }
                }),
            ),
        ],
    )?;
    let newer_mtime =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1_800_000_000_000);
    fs::File::options()
        .write(true)
        .open(&newer)?
        .set_times(fs::FileTimes::new().set_modified(newer_mtime))?;
    write_jsonl(
        &temp_home.join(".pi/agent/sessions/broken.jsonl"),
        &[json!({ "not": "a session" })],
    )?;
    write_jsonl(
        &temp_home.join(".pi/agent/sessions/not-session.jsonl"),
        &[json!({ "type": "other", "id": "x" })],
    )?;
    write_jsonl(
        &temp_home.join(".pi/agent/sessions/missing-id.jsonl"),
        &[json!({
            "type": "session",
            "version": 3,
            "timestamp": "2026-09-04T10:00:00.000Z",
            "cwd": "/tmp/pi-project"
        })],
    )?;

    let entries = session::reader(SourceApp::Pi).list_entries()?;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, newer);
    assert_eq!(entries[1].path, older);
    assert!(entries.iter().all(|entry| entry.summary.is_none()));
    let sources = session::catalog::detect_sources_inner(
        &state::session_index::SessionIndexState::default(),
    )?;
    assert!(sources.iter().any(|source| {
        source.app == SourceApp::Pi && source.available && source.session_count == 2
    }));
    Ok(())
}

#[test]
fn pi_honors_configured_session_directory() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    let custom_sessions = temp_home.join("custom-pi-sessions");
    fs::create_dir_all(&custom_sessions)?;
    let guard = TestEnvGuard::set_home(&temp_home);
    guard.set_pi_session_dir(&custom_sessions);

    let path = custom_sessions.join("2026-09-04T10-00-00-000Z_pi-custom.jsonl");
    write_jsonl(
        &path,
        &[
            pi_header("pi-custom", "/tmp/pi-project", "2026-09-04T10:00:00.000Z"),
            pi_entry(
                "message",
                "message-1",
                None,
                "2026-09-04T10:00:01.000Z",
                json!({ "message": { "role": "user", "content": "custom directory" } }),
            ),
        ],
    )?;

    let entries = session::reader(SourceApp::Pi).list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, path);
    let sources = session::catalog::detect_sources_inner(
        &state::session_index::SessionIndexState::default(),
    )?;
    assert!(sources.iter().any(|source| {
        source.app == SourceApp::Pi && source.available && source.session_count == 1
    }));
    Ok(())
}

#[test]
fn pi_active_branch_and_events_follow_the_last_entry_chain() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T10-00-00-000Z",
        "pi-tree",
    );
    let user = pi_entry(
        "message",
        "user-1",
        None,
        "2026-09-04T10:00:01.000Z",
        json!({
            "message": { "role": "user", "content": "active question" }
        }),
    );
    let assistant = pi_entry(
        "message",
        "assistant-1",
        Some("user-1"),
        "2026-09-04T10:00:02.000Z",
        json!({
            "message": { "role": "assistant", "content": [{ "type": "text", "text": "active answer" }] }
        }),
    );
    let inactive = pi_entry(
        "message",
        "inactive-user",
        Some("user-1"),
        "2026-09-04T10:00:03.000Z",
        json!({
            "message": { "role": "user", "content": "inactive branch" }
        }),
    );
    let active_leaf = pi_entry(
        "message",
        "active-user-2",
        Some("assistant-1"),
        "2026-09-04T10:00:04.000Z",
        json!({
            "message": { "role": "user", "content": [{ "type": "text", "text": "latest request" }, { "type": "image", "data": "AA==", "mimeType": "image/png" }] }
        }),
    );
    let model = pi_entry(
        "model_change",
        "model-1",
        Some("active-user-2"),
        "2026-09-04T10:00:05.000Z",
        json!({
            "provider": "anthropic", "modelId": "sonnet"
        }),
    );
    let thinking = pi_entry(
        "thinking_level_change",
        "thinking-1",
        Some("model-1"),
        "2026-09-04T10:00:06.000Z",
        json!({
            "thinkingLevel": "high"
        }),
    );
    let compaction = pi_entry(
        "compaction",
        "compact-1",
        Some("thinking-1"),
        "2026-09-04T10:00:07.000Z",
        json!({
            "summary": "kept context", "retainedTail": [{ "role": "user", "content": "tail" }]
        }),
    );
    let branch_summary = pi_entry(
        "branch_summary",
        "branch-1",
        Some("compact-1"),
        "2026-09-04T10:00:08.000Z",
        json!({
            "fromId": "inactive-user", "summary": "inactive context"
        }),
    );
    let custom = pi_entry(
        "custom",
        "custom-1",
        Some("branch-1"),
        "2026-09-04T10:00:09.000Z",
        json!({
            "customType": "test", "data": { "value": 1 }
        }),
    );
    let label = pi_entry(
        "label",
        "label-1",
        Some("custom-1"),
        "2026-09-04T10:00:10.000Z",
        json!({
            "targetId": "user-1", "label": "checkpoint"
        }),
    );
    let info = pi_entry(
        "session_info",
        "info-1",
        Some("label-1"),
        "2026-09-04T10:00:11.000Z",
        json!({
            "name": "Named Pi session"
        }),
    );
    let mut header = pi_header("pi-tree", "/tmp/pi-project", "2026-09-04T10:00:00.000Z");
    header["parentSession"] = Value::String("/tmp/parent.jsonl".to_string());
    let mut lines = vec![header];
    lines.extend([
        user,
        assistant,
        inactive,
        active_leaf,
        model,
        thinking,
        compaction,
        branch_summary,
        custom,
        label,
        info,
    ]);
    write_jsonl(&path, &lines)?;

    let reader = session::reader(SourceApp::Pi);
    let summary = reader.parse_summary("pi-tree")?;
    assert_eq!(summary.title, "Named Pi session");
    let detail = read_detail(reader, "pi-tree")?;
    let texts = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter().filter_map(|block| block.text.clone()))
        .collect::<Vec<_>>();
    assert!(texts.iter().any(|text| text.contains("active question")));
    assert!(texts.iter().any(|text| text.contains("latest request")));
    assert!(!texts.iter().any(|text| text.contains("inactive branch")));
    for kind in [
        "model_change",
        "thinking_level_change",
        "compaction",
        "branch_summary",
        "custom",
        "label",
        "session_info",
    ] {
        assert!(detail.events.iter().any(|event| event.kind == kind));
    }
    assert!(detail.events.iter().any(|event| {
        event.kind == "compaction"
            && event
                .payload
                .as_ref()
                .is_some_and(|payload| payload.get("retainedTail").is_some())
    }));
    // source_paths 来自目录扫描(Windows 反斜杠),按路径语义比较而非拼写。
    assert_eq!(detail.source_paths.len(), 1);
    assert_eq!(Path::new(&detail.source_paths[0]), path.as_path());
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.kind == "parent_session")
    );
    let overview = reader.parse_overview("pi-tree")?;
    assert_eq!(overview.message_count, Some(detail.messages.len()));
    assert_eq!(overview.event_count, Some(detail.events.len()));
    assert_eq!(overview.agents.len(), 1);
    // 详情正文全量契约:与 overview 计数一致,无分页元数据。
    let messages = reader.parse_messages("pi-tree")?;
    assert_eq!(messages.len(), detail.messages.len());
    let events = reader.parse_events("pi-tree")?;
    assert_eq!(events.len(), detail.events.len());
    Ok(())
}

#[test]
fn pi_parses_subagent_tool_result_into_structured_run_block() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T12-00-00-000Z",
        "pi-subagent",
    );
    let subagent_details = json!({
        "mode": "single",
        "agentScope": "both",
        "results": [{
            "agent": "scout",
            "agentSource": "project",
            "task": "梳理代码库",
            "exitCode": 0,
            "stderr": "",
            "usage": { "input": 636783, "output": 31284, "cacheRead": 2916352, "turns": 18 },
            "model": "acme/chat-b",
            "stopReason": "stop",
            "messages": [
                { "role": "user", "content": "Task: 梳理代码库", "timestamp": 1789091776409_i64 },
                {
                    "role": "assistant",
                    "content": [
                        { "type": "thinking", "thinking": "先看目录结构" },
                        { "type": "text", "text": "结论：只读调查完成" },
                    ],
                    "usage": { "input": 100, "output": 20 },
                    "stopReason": "stop",
                    "timestamp": 1789091776480_i64,
                },
            ],
        }],
    });
    let mut lines = vec![pi_header(
        "pi-subagent",
        "/tmp/pi-project",
        "2026-09-04T12:00:00.000Z",
    )];
    lines.push(pi_entry(
        "message",
        "sa-u1",
        None,
        "2026-09-04T12:00:01.000Z",
        json!({ "message": { "role": "user", "content": "派个 scout 调查" } }),
    ));
    lines.push(pi_entry(
        "message",
        "sa-a1",
        Some("sa-u1"),
        "2026-09-04T12:00:02.000Z",
        json!({
            "message": {
                "role": "assistant",
                "content": [{
                    "type": "toolCall",
                    "id": "call-sub-1",
                    "name": "subagent",
                    "arguments": { "agent": "scout", "task": "梳理代码库" },
                }],
            }
        }),
    ));
    lines.push(pi_entry(
        "message",
        "sa-t1",
        Some("sa-a1"),
        "2026-09-04T12:00:03.000Z",
        json!({
            "message": {
                "role": "toolResult",
                "toolCallId": "call-sub-1",
                "toolName": "subagent",
                "content": [{ "type": "text", "text": "## 结论\n整体判断…" }],
                "details": subagent_details,
                "isError": false,
            }
        }),
    ));
    write_jsonl(&path, &lines)?;

    let reader = session::reader(SourceApp::Pi);
    let detail = read_detail(reader, "pi-subagent")?;
    assert_eq!(detail.messages.len(), 3);

    // subagent toolResult 解析为单个结构化块,报告文本保留。
    let run_message = &detail.messages[2];
    assert_eq!(run_message.blocks.len(), 1);
    let block = &run_message.blocks[0];
    assert_eq!(block.kind, "subagent_run");
    assert_eq!(block.tool_name.as_deref(), Some("subagent"));
    assert_eq!(block.tool_call_id.as_deref(), Some("call-sub-1"));
    assert_eq!(block.text.as_deref(), Some("## 结论\n整体判断…"));

    // payload.runs 携带 run 元数据,原始 messages 数组被解析后的嵌套消息替代。
    let payload = block.payload.as_ref().expect("subagent payload");
    assert_eq!(payload["runs"][0]["agent"], "scout");
    assert_eq!(payload["runs"][0]["agentSource"], "project");
    assert_eq!(payload["runs"][0]["usage"]["turns"], 18);
    assert!(payload["runs"][0].get("messages").is_none());
    let nested = payload["runs"][0]["nestedMessages"]
        .as_array()
        .expect("nested messages");
    assert_eq!(nested.len(), 2);
    assert_eq!(nested[0]["role"], "user");
    assert_eq!(nested[1]["role"], "assistant");
    assert_eq!(nested[1]["timestamp"], 1789091776480_i64);
    let nested_blocks = nested[1]["blocks"].as_array().expect("nested blocks");
    assert_eq!(nested_blocks[0]["kind"], "thinking");
    assert_eq!(nested_blocks[1]["kind"], "output_text");

    // 嵌套消息 id 全局唯一,可与主会话消息共存于同一列表。
    assert_eq!(nested[0]["id"], "sa-t1-r0-m0");
    assert_eq!(nested[1]["id"], "sa-t1-r0-m1");

    // 普通 toolResult 不受影响,仍是 tool_result 块。
    let normal_path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T12-10-00-000Z",
        "pi-normal-tool",
    );
    let mut normal_lines = vec![pi_header(
        "pi-normal-tool",
        "/tmp/pi-project",
        "2026-09-04T12:10:00.000Z",
    )];
    normal_lines.push(pi_entry(
        "message",
        "nt-t1",
        None,
        "2026-09-04T12:10:01.000Z",
        json!({
            "message": {
                "role": "toolResult",
                "toolCallId": "call-1",
                "toolName": "bash",
                "content": [{ "type": "text", "text": "output" }],
                "isError": false,
            }
        }),
    ));
    write_jsonl(&normal_path, &normal_lines)?;
    let normal_detail = read_detail(reader, "pi-normal-tool")?;
    assert_eq!(normal_detail.messages[0].blocks[0].kind, "tool_result");

    // Pi 的 subagent 内嵌在 toolResult.details 里,没有子会话可查:
    // 必须报错而不是返回空,否则调用方无法区分"不支持"与"空子会话"
    assert!(
        reader
            .parse_agent_messages("pi-subagent", "any-agent-id")
            .is_err()
    );
    Ok(())
}

#[test]
fn pi_preserves_tool_custom_unknown_and_broken_chain_content() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T10-00-00-000Z",
        "pi-roles",
    );
    let entries = vec![
        pi_header("pi-roles", "/tmp/pi-project", "2026-09-04T10:00:00.000Z"),
        pi_entry(
            "message",
            "u",
            None,
            "2026-09-04T10:00:01.000Z",
            json!({
                "message": { "role": "user", "content": "question" }
            }),
        ),
        pi_entry(
            "message",
            "a",
            Some("u"),
            "2026-09-04T10:00:02.000Z",
            json!({
                "message": { "role": "assistant", "content": [
                    { "type": "text", "text": "answer" },
                    { "type": "thinking", "thinking": "reason" },
                    { "type": "toolCall", "id": "call-1", "name": "bash", "arguments": { "command": "pwd" } },
                    { "type": "futureBlock", "value": 42 }
                ] }
            }),
        ),
        pi_entry(
            "message",
            "tool",
            Some("a"),
            "2026-09-04T10:00:03.000Z",
            json!({
                "message": { "role": "toolResult", "toolCallId": "call-1", "toolName": "bash", "content": [{ "type": "text", "text": "/tmp" }] }
            }),
        ),
        pi_entry(
            "message",
            "unknown",
            Some("tool"),
            "2026-09-04T10:00:04.000Z",
            json!({
                "message": { "role": "futureRole", "content": "must survive" }
            }),
        ),
        pi_entry(
            "custom_message",
            "custom",
            Some("unknown"),
            "2026-09-04T10:00:05.000Z",
            json!({
                "customType": "extension", "content": "custom context", "display": true
            }),
        ),
        pi_entry(
            "message",
            "orphan",
            Some("missing"),
            "2026-09-04T10:00:06.000Z",
            json!({
                "message": { "role": "user", "content": "orphan" }
            }),
        ),
        pi_entry(
            "label",
            "final",
            Some("custom"),
            "2026-09-04T10:00:07.000Z",
            json!({
                "targetId": "custom", "label": "final"
            }),
        ),
    ];
    write_jsonl(&path, &entries)?;

    let detail = read_detail(session::reader(SourceApp::Pi), "pi-roles")?;
    assert!(detail.messages.iter().any(|message| {
        message.id == "a"
            && message
                .blocks
                .iter()
                .any(|block| block.kind == "output_text" && block.text.as_deref() == Some("answer"))
    }));
    assert!(
        detail
            .messages
            .iter()
            .any(|message| message.role == "toolResult")
    );
    assert!(detail.messages.iter().any(|message| {
        message.role == "toolResult"
            && message
                .blocks
                .iter()
                .any(|block| block.kind == "tool_result")
    }));
    assert!(
        detail
            .messages
            .iter()
            .any(|message| message.role == "custom")
    );
    assert!(detail.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.kind == "unsupported_content")
    }));
    assert!(
        detail
            .messages
            .iter()
            .any(|message| message.blocks.iter().any(|block| {
                block
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("must survive"))
            }))
    );
    assert!(!detail.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("orphan"))
    }));
    Ok(())
}

#[test]
fn pi_skips_zero_width_text_placeholders_without_losing_tool_context() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T10-00-00-000Z",
        "pi-zero-width-placeholder",
    );
    write_jsonl(
        &path,
        &[
            pi_header(
                "pi-zero-width-placeholder",
                "/tmp/pi-project",
                "2026-09-04T10:00:00.000Z",
            ),
            pi_entry(
                "message",
                "assistant-1",
                None,
                "2026-09-04T10:00:01.000Z",
                json!({
                    "message": {
                        "role": "assistant",
                        "content": [
                            { "type": "text", "text": "\u{200B}" },
                            { "type": "thinking", "thinking": "reason" },
                            { "type": "toolCall", "id": "call-1", "name": "bash", "arguments": { "command": "pwd" } }
                        ]
                    }
                }),
            ),
        ],
    )?;

    let detail = read_detail(session::reader(SourceApp::Pi), "pi-zero-width-placeholder")?;
    let message = detail
        .messages
        .iter()
        .find(|message| message.id == "assistant-1")
        .expect("assistant message");
    assert!(
        !message
            .blocks
            .iter()
            .any(|block| block.kind == "output_text")
    );
    assert!(
        message
            .blocks
            .iter()
            .any(|block| block.kind == "thinking" && block.text.as_deref() == Some("reason"))
    );
    assert!(message.blocks.iter().any(|block| {
        block.kind == "tool_use"
            && block.tool_name.as_deref() == Some("bash")
            && block.tool_call_id.as_deref() == Some("call-1")
    }));
    Ok(())
}

#[test]
fn pi_preserves_message_entries_without_message_payloads() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T10-00-00-000Z",
        "pi-missing-message",
    );
    write_jsonl(
        &path,
        &[
            pi_header(
                "pi-missing-message",
                "/tmp/pi-project",
                "2026-09-04T10:00:00.000Z",
            ),
            pi_entry(
                "message",
                "missing-message",
                None,
                "2026-09-04T10:00:01.000Z",
                json!({ "futureField": { "value": 42 } }),
            ),
        ],
    )?;

    let detail = read_detail(session::reader(SourceApp::Pi), "pi-missing-message")?;
    assert_eq!(detail.messages.len(), 1);
    assert_eq!(detail.messages[0].role, "unknown");
    assert_eq!(detail.messages[0].blocks[0].kind, "unsupported_content");
    assert!(
        detail.messages[0].blocks[0]
            .text
            .as_deref()
            .is_some_and(|text| text.contains("futureField"))
    );
    Ok(())
}

#[test]
fn pi_cycle_at_leaf_stops_without_looping() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T10-00-00-000Z",
        "pi-cycle",
    );
    write_jsonl(
        &path,
        &[
            pi_header("pi-cycle", "/tmp/pi-project", "2026-09-04T10:00:00.000Z"),
            pi_entry(
                "message",
                "cycle-a",
                Some("cycle-b"),
                "2026-09-04T10:00:01.000Z",
                json!({ "message": { "role": "user", "content": "cycle a" } }),
            ),
            pi_entry(
                "message",
                "cycle-b",
                Some("cycle-a"),
                "2026-09-04T10:00:02.000Z",
                json!({ "message": { "role": "assistant", "content": "cycle b" } }),
            ),
        ],
    )?;

    let detail = read_detail(session::reader(SourceApp::Pi), "pi-cycle")?;
    assert_eq!(detail.messages.len(), 2);
    assert_eq!(detail.messages[0].id, "cycle-a");
    assert_eq!(detail.messages[1].id, "cycle-b");
    Ok(())
}

#[test]
fn pi_reads_settings_json_session_dir_and_env_takes_precedence() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(temp_home.join(".pi/agent"))?;
    let settings_dir = temp_home.join("settings-sessions");
    fs::create_dir_all(&settings_dir)?;
    fs::write(
        temp_home.join(".pi/agent/settings.json"),
        json!({ "sessionDir": settings_dir.display().to_string() }).to_string(),
    )?;
    let guard = TestEnvGuard::set_home(&temp_home);
    write_jsonl(
        &settings_dir.join("2026-09-04T10-00-00-000Z_pi-settings.jsonl"),
        &[
            pi_header("pi-settings", "/tmp/pi-project", "2026-09-04T10:00:00.000Z"),
            pi_entry(
                "message",
                "message-1",
                None,
                "2026-09-04T10:00:01.000Z",
                json!({ "message": { "role": "user", "content": "settings directory" } }),
            ),
        ],
    )?;

    let entries = session::reader(SourceApp::Pi).list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path.parent(), Some(settings_dir.as_path()));

    // env 优先级高于 settings.json
    let env_dir = temp_home.join("env-sessions");
    fs::create_dir_all(&env_dir)?;
    guard.set_pi_session_dir(&env_dir);
    write_jsonl(
        &env_dir.join("2026-09-04T10-00-00-000Z_pi-env.jsonl"),
        &[
            pi_header("pi-env", "/tmp/pi-project", "2026-09-04T10:00:00.000Z"),
            pi_entry(
                "message",
                "message-1",
                None,
                "2026-09-04T10:00:01.000Z",
                json!({ "message": { "role": "user", "content": "env directory" } }),
            ),
        ],
    )?;

    let entries = session::reader(SourceApp::Pi).list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path.parent(), Some(env_dir.as_path()));
    Ok(())
}

#[test]
fn pi_delete_in_configured_dir_keeps_the_session_root() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let guard = TestEnvGuard::set_home(&temp_home);
    let custom_dir = temp_home.join("custom-pi-sessions");
    fs::create_dir_all(&custom_dir)?;
    guard.set_pi_session_dir(&custom_dir);
    let path = custom_dir.join("2026-09-04T10-00-00-000Z_pi-delete-custom.jsonl");
    write_jsonl(
        &path,
        &[
            pi_header(
                "pi-delete-custom",
                "/tmp/pi-project",
                "2026-09-04T10:00:00.000Z",
            ),
            pi_entry(
                "message",
                "message-1",
                None,
                "2026-09-04T10:00:01.000Z",
                json!({ "message": { "role": "user", "content": "custom root" } }),
            ),
        ],
    )?;

    session::delete::delete_session_inner(
        &state::session_index::SessionIndexState::default(),
        SourceApp::Pi,
        "pi-delete-custom",
    )?;
    assert!(!path.exists());
    // 用户显式配置的目录即使被清空也不能被顺手删除
    assert!(custom_dir.exists());
    Ok(())
}

#[test]
fn pi_parses_v1_linear_session_with_full_history() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = temp_home
        .join(".pi/agent/sessions/--tmp-pi-v1--")
        .join("2026-09-04T10-00-00-000Z_pi-v1.jsonl");
    // v1 线性文件:entry 无 id/parentId(header 仍有 id),compaction 用 firstKeptEntryIndex
    write_jsonl(
        &path,
        &[
            json!({
                "type": "session", "version": 1, "id": "pi-v1",
                "timestamp": "2026-09-04T10:00:00.000Z", "cwd": "/tmp/pi-v1"
            }),
            json!({
                "type": "message", "timestamp": "2026-09-04T10:00:01.000Z",
                "message": { "role": "user", "content": "v1 first question" }
            }),
            json!({
                "type": "message", "timestamp": "2026-09-04T10:00:02.000Z",
                "message": { "role": "assistant", "content": [{ "type": "text", "text": "v1 answer" }] }
            }),
            json!({
                "type": "compaction", "timestamp": "2026-09-04T10:00:03.000Z",
                "summary": "v1 compacted", "firstKeptEntryIndex": 2, "tokensBefore": 100
            }),
            json!({
                "type": "message", "timestamp": "2026-09-04T10:00:04.000Z",
                "message": { "role": "user", "content": "v1 after compaction" }
            }),
        ],
    )?;

    let reader = session::reader(SourceApp::Pi);
    let detail = read_detail(reader, "pi-v1")?;
    let texts = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter().filter_map(|block| block.text.clone()))
        .collect::<Vec<_>>();
    assert!(texts.iter().any(|text| text.contains("v1 first question")));
    assert!(texts.iter().any(|text| text.contains("v1 answer")));
    assert!(
        texts
            .iter()
            .any(|text| text.contains("v1 after compaction"))
    );
    assert!(
        detail
            .messages
            .iter()
            .any(|message| message.role == "compactionSummary")
    );

    // compaction 的行索引迁移成 entry id
    let compaction = detail
        .events
        .iter()
        .find(|event| event.kind == "compaction")
        .expect("compaction event");
    let payload = compaction.payload.as_ref().expect("compaction payload");
    assert!(payload.get("firstKeptEntryIndex").is_none());
    let kept_id = payload
        .get("firstKeptEntryId")
        .and_then(Value::as_str)
        .expect("firstKeptEntryId");
    assert!(!kept_id.is_empty());

    // 标题取自第一条用户消息,而不是退化成 session id
    let summary = reader.parse_summary("pi-v1")?;
    assert_eq!(summary.title, "v1 first question");
    Ok(())
}

#[test]
fn pi_restores_compaction_retained_tail_as_messages() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T10-00-00-000Z",
        "pi-compaction",
    );
    write_jsonl(
        &path,
        &[
            pi_header(
                "pi-compaction",
                "/tmp/pi-project",
                "2026-09-04T10:00:00.000Z",
            ),
            pi_entry(
                "message",
                "user-1",
                None,
                "2026-09-04T10:00:01.000Z",
                json!({ "message": { "role": "user", "content": "old question" } }),
            ),
            pi_entry(
                "compaction",
                "compact-1",
                Some("user-1"),
                "2026-09-04T10:00:02.000Z",
                json!({
                    "summary": "compacted context",
                    "retainedTail": [
                        { "role": "user", "content": "kept question", "timestamp": 1_744_366_401_000_i64 },
                        { "role": "assistant", "content": [{ "type": "text", "text": "kept answer" }], "timestamp": 1_744_366_402_000_i64 }
                    ]
                }),
            ),
            pi_entry(
                "message",
                "user-2",
                Some("compact-1"),
                "2026-09-04T10:00:03.000Z",
                json!({ "message": { "role": "user", "content": "after compaction" } }),
            ),
        ],
    )?;

    let detail = read_detail(session::reader(SourceApp::Pi), "pi-compaction")?;
    let texts = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter().filter_map(|block| block.text.clone()))
        .collect::<Vec<_>>();
    assert!(texts.iter().any(|text| text.contains("kept question")));
    assert!(texts.iter().any(|text| text.contains("kept answer")));
    // 浏览器展示完整链,compaction 之前的旧消息保留(官方只在重建上下文时裁剪)
    assert!(texts.iter().any(|text| text.contains("old question")));

    // retainedTail 恢复出的消息带有自己的时间
    let kept = detail
        .messages
        .iter()
        .find(|message| {
            message
                .blocks
                .iter()
                .any(|block| block.text.as_deref() == Some("kept question"))
        })
        .expect("kept question message");
    assert_eq!(kept.timestamp, Some(1_744_366_401_000_i64));
    Ok(())
}

#[test]
fn pi_parses_tool_failure_state() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T10-00-00-000Z",
        "pi-failure",
    );
    write_jsonl(
        &path,
        &[
            pi_header("pi-failure", "/tmp/pi-project", "2026-09-04T10:00:00.000Z"),
            pi_entry(
                "message",
                "bash-1",
                None,
                "2026-09-04T10:00:01.000Z",
                json!({
                    "message": {
                        "role": "bashExecution",
                        "command": "exit 1",
                        "output": "boom",
                        "exitCode": 1,
                        "cancelled": true,
                        "truncated": true,
                        "timestamp": 1_744_366_401_000_i64
                    }
                }),
            ),
            pi_entry(
                "message",
                "assistant-1",
                Some("bash-1"),
                "2026-09-04T10:00:02.000Z",
                json!({
                    "message": {
                        "role": "assistant",
                        "content": [{ "type": "toolCall", "id": "call-x", "name": "bash", "arguments": { "command": "ls" } }],
                        "timestamp": 1_744_366_402_000_i64
                    }
                }),
            ),
            pi_entry(
                "message",
                "tool-1",
                Some("assistant-1"),
                "2026-09-04T10:00:03.000Z",
                json!({
                    "message": {
                        "role": "toolResult",
                        "toolCallId": "call-x",
                        "toolName": "bash",
                        "content": [{ "type": "text", "text": "err" }],
                        "isError": true,
                        "timestamp": 1_744_366_403_000_i64
                    }
                }),
            ),
        ],
    )?;

    let detail = read_detail(session::reader(SourceApp::Pi), "pi-failure")?;
    let tool_result = detail
        .messages
        .iter()
        .find(|message| message.role == "toolResult")
        .expect("tool result message");
    assert!(
        tool_result
            .blocks
            .iter()
            .any(|block| block.is_error == Some(true))
    );
    Ok(())
}

#[test]
fn pi_shows_message_error_text_instead_of_raw_empty_message_row() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T11-00-00-000Z",
        "pi-aborted",
    );
    write_jsonl(
        &path,
        &[
            pi_header("pi-aborted", "/tmp/pi-project", "2026-09-04T11:00:00.000Z"),
            pi_entry(
                "message",
                "assistant-aborted",
                None,
                "2026-09-04T11:00:01.000Z",
                json!({
                    "message": {
                        "role": "assistant",
                        "content": [],
                        "api": "openai-responses",
                        "model": "deepseek-flash",
                        "provider": "reins-custom-gateway",
                        "stopReason": "error",
                        "errorMessage": "This operation was aborted",
                        "timestamp": 1_744_366_401_000_i64
                    }
                }),
            ),
        ],
    )?;

    let detail = read_detail(session::reader(SourceApp::Pi), "pi-aborted")?;
    let block = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .find(|block| block.kind == "message_error")
        .expect("error block");
    assert_eq!(block.text.as_deref(), Some("This operation was aborted"));
    assert_eq!(block.is_error, Some(true));
    // 原始报文保留在 payload 里，界面仍可展开查看
    assert!(
        block
            .payload
            .as_ref()
            .is_some_and(|payload| payload.to_string().contains("deepseek-flash"))
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn pi_keeps_injected_context_as_readable_block() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T12-00-00-000Z",
        "pi-context",
    );
    write_jsonl(
        &path,
        &[
            pi_header("pi-context", "/tmp/pi-project", "2026-09-04T12:00:00.000Z"),
            pi_entry(
                "message",
                "user-context",
                None,
                "2026-09-04T12:00:01.000Z",
                json!({
                    "message": {
                        "role": "user",
                        "content": [
                            {
                                "type": "text",
                                "text": "<environment_context>\n  <cwd>/tmp/pi-project</cwd>\n</environment_context>"
                            },
                            {
                                "type": "text",
                                "text": "<system-reminder>\n读代码前先看 AGENTS.md\n</system-reminder>"
                            }
                        ],
                        "timestamp": 1_744_366_401_000_i64
                    }
                }),
            ),
        ],
    )?;

    let detail = read_detail(session::reader(SourceApp::Pi), "pi-context")?;
    let block = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .find(|block| block.kind == "context_injection")
        .expect("context injection block");
    let text = block.text.as_deref().unwrap_or_default();
    assert!(text.contains("<environment_context>"));
    assert!(text.contains("<system-reminder>"));
    assert!(!detail.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.kind == "empty_message")
    }));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn pi_keeps_raw_diagnostic_block_when_empty_message_has_no_semantics() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T13-00-00-000Z",
        "pi-blank",
    );
    write_jsonl(
        &path,
        &[
            pi_header("pi-blank", "/tmp/pi-project", "2026-09-04T13:00:00.000Z"),
            pi_entry(
                "message",
                "assistant-blank",
                None,
                "2026-09-04T13:00:01.000Z",
                json!({
                    "message": {
                        "role": "assistant",
                        "content": [],
                        "stopReason": "stop",
                        "timestamp": 1_744_366_401_000_i64
                    }
                }),
            ),
        ],
    )?;

    let detail = read_detail(session::reader(SourceApp::Pi), "pi-blank")?;
    // 既没有错误也没有注入上下文：保持原始报文兜底，便于排查来源侧异常
    assert!(detail.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.kind == "empty_message")
    }));
    assert!(
        !detail
            .messages
            .iter()
            .flat_map(|message| message.blocks.iter())
            .any(|block| block.kind == "context_injection")
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn pi_turns_image_item_into_renderable_image_block() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T14-00-00-000Z",
        "pi-image",
    );
    write_jsonl(
        &path,
        &[
            pi_header("pi-image", "/tmp/pi-project", "2026-09-04T14:00:00.000Z"),
            pi_entry(
                "message",
                "user-image",
                None,
                "2026-09-04T14:00:01.000Z",
                json!({
                    "message": {
                        "role": "user",
                        "content": [
                            { "type": "text", "text": "这张图是什么" },
                            { "type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png" }
                        ],
                        "timestamp": 1_744_366_401_000_i64
                    }
                }),
            ),
        ],
    )?;

    let detail = read_detail(session::reader(SourceApp::Pi), "pi-image")?;
    let block = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .find(|block| block.kind == "image")
        .expect("image block");
    // data + mimeType 还原成 data URL，前端才能直接渲染
    assert_eq!(
        block.text.as_deref(),
        Some("data:image/png;base64,iVBORw0KGgo=")
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn pi_renders_system_prompt_sections_as_readable_block() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-09-04T15-00-00-000Z",
        "pi-system",
    );
    write_jsonl(
        &path,
        &[
            pi_header("pi-system", "/tmp/pi-project", "2026-09-04T15:00:00.000Z"),
            pi_entry(
                "message",
                "system-1",
                None,
                "2026-09-04T15:00:01.000Z",
                json!({
                    "message": {
                        "role": "system",
                        "content": "",
                        "sections": {
                            "cwd": "<cwd>\n/tmp/pi-project\n</cwd>",
                            "preamble": "You are an expert coding assistant.",
                            "rules": "<rules>\n- be concise\n</rules>"
                        },
                        "toolsAdded": [
                            {
                                "name": "read",
                                "description": "Read the contents of a file.",
                                "parameters": { "type": "object" },
                                "constrainedSampling": { "type": "json_schema" }
                            }
                        ],
                        "toolsRemoved": [{ "name": "bash" }],
                        "timestamp": 1_744_366_401_000_i64
                    }
                }),
            ),
        ],
    )?;

    let detail = read_detail(session::reader(SourceApp::Pi), "pi-system")?;
    let block = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .find(|block| block.kind == "system_prompt")
        .expect("system prompt block");
    let text = block.text.as_deref().unwrap_or_default();
    assert!(text.contains("3 段：cwd、preamble、rules"));
    assert!(text.contains("## preamble\nYou are an expert coding assistant."));
    assert!(text.contains("## rules\n<rules>\n- be concise\n</rules>"));
    assert!(text.contains("- read: Read the contents of a file."));
    assert!(text.contains("## 本次移除的工具\n- bash"));
    // 工具的 parameters schema 不进正文
    assert!(!text.contains("json_schema"));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

// pi-subagents 扩展派生的子代理会话(带 parentSession + "#<8位hex>" 会话名)
// 折叠进主会话:列表只出主会话一条,时间线注入 marker 入口,名册列出子代理,
// 弹窗按子会话 id 取完整消息。fork 子会话(有 parentSession、无会话名)不归族,
// 保持独立条目——fork 复制了父会话历史,折叠会整段重复。
#[test]
fn pi_folds_subagent_child_sessions_into_the_parent_family() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let main_path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-10-29T02-00-00-000Z",
        "pi-main",
    );
    let child_a_path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-10-29T02-01-00-000Z",
        "pi-child-a",
    );
    let child_b_path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-10-29T02-02-00-000Z",
        "pi-child-b",
    );
    let fork_path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-10-29T02-03-00-000Z",
        "pi-fork",
    );

    write_jsonl(
        &main_path,
        &[
            pi_header("pi-main", "/tmp/pi-project", "2026-10-29T02:00:00.000Z"),
            pi_entry(
                "message",
                "main-user",
                None,
                "2026-10-29T02:00:01.000Z",
                json!({ "message": { "role": "user", "content": "main question" } }),
            ),
        ],
    )?;

    // 子代理子会话:header 带 parentSession,session_info 名形如 <type>#<8位hex>,
    // 消息含带 usage 的 assistant(家族用量与模型合并的数据来源)。
    let child_header = |id: &str, timestamp: &str| {
        let mut header = pi_header(id, "/tmp/pi-project", timestamp);
        header["parentSession"] = json!(main_path.display().to_string());
        header
    };
    let child_assistant = |text: &str| {
        json!({
            "message": {
                "role": "assistant",
                "content": [{ "type": "text", "text": text }],
                "provider": "reins",
                "model": "deepseek-flash",
                "usage": { "input": 100, "output": 50, "cacheRead": 10, "cacheWrite": 0 }
            }
        })
    };
    write_jsonl(
        &child_a_path,
        &[
            child_header("pi-child-a", "2026-10-29T02:01:00.000Z"),
            pi_entry(
                "session_info",
                "info-a",
                None,
                "2026-10-29T02:01:00.000Z",
                json!({ "name": "Explore#aaaaaaaa" }),
            ),
            pi_entry(
                "message",
                "child-a-user",
                None,
                "2026-10-29T02:01:01.000Z",
                json!({ "message": { "role": "user", "content": "child a task" } }),
            ),
            pi_entry(
                "message",
                "child-a-assistant",
                Some("child-a-user"),
                "2026-10-29T02:01:02.000Z",
                child_assistant("child a answer"),
            ),
        ],
    )?;
    write_jsonl(
        &child_b_path,
        &[
            child_header("pi-child-b", "2026-10-29T02:02:00.000Z"),
            pi_entry(
                "session_info",
                "info-b",
                None,
                "2026-10-29T02:02:00.000Z",
                json!({ "name": "Explore#bbbbbbbb" }),
            ),
            pi_entry(
                "message",
                "child-b-user",
                None,
                "2026-10-29T02:02:01.000Z",
                json!({ "message": { "role": "user", "content": "child b task" } }),
            ),
            pi_entry(
                "message",
                "child-b-assistant",
                Some("child-b-user"),
                "2026-10-29T02:02:02.000Z",
                child_assistant("child b answer"),
            ),
        ],
    )?;
    // fork 子会话:有血缘、无会话名,不归族。
    write_jsonl(
        &fork_path,
        &[
            child_header("pi-fork", "2026-10-29T02:03:00.000Z"),
            pi_entry(
                "message",
                "fork-user",
                None,
                "2026-10-29T02:03:01.000Z",
                json!({ "message": { "role": "user", "content": "fork question" } }),
            ),
        ],
    )?;

    let reader = session::reader(SourceApp::Pi);
    let entries = reader.list_entries()?;
    let mut entry_ids = entries
        .iter()
        .map(|entry| entry.source_session_id.as_str())
        .collect::<Vec<_>>();
    entry_ids.sort();
    assert_eq!(entry_ids, ["pi-fork", "pi-main"]);

    let summary = reader.parse_summary("pi-main")?;
    assert_eq!(summary.title, "main question (+2 subagents)");
    // 家族用量 = 全部成员求和:两个子会话各 input=100/output=50/cacheRead=10。
    let usage = summary.token_usage.expect("family usage sums members");
    assert_eq!(usage.input_tokens, 200);
    assert_eq!(usage.output_tokens, 100);
    assert_eq!(usage.cache_read_tokens, 20);
    assert!(summary.models.iter().any(|model| model.model == "deepseek-flash"));

    let detail = read_detail(reader, "pi-main")?;
    let markers = detail
        .messages
        .iter()
        .filter(|message| {
            message.blocks.iter().any(|block| {
                block
                    .payload
                    .as_ref()
                    .is_some_and(|payload| payload["type"] == "subagent_started")
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(markers.len(), 2);
    assert!(
        markers
            .iter()
            .any(|message| message.session_id.as_deref() == Some("pi-child-a"))
    );
    let texts = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter().filter_map(|block| block.text.clone()))
        .collect::<Vec<_>>();
    assert!(texts.iter().any(|text| text.contains("child a answer")));
    assert!(texts.iter().any(|text| text.contains("child b answer")));

    let overview = reader.parse_overview("pi-main")?;
    assert_eq!(overview.agents.len(), 3);
    assert_eq!(
        overview
            .agents
            .iter()
            .filter(|agent| agent.is_root)
            .count(),
        1
    );
    assert!(
        overview
            .agents
            .iter()
            .any(|agent| agent.session_id == "pi-child-a" && agent.label == "Explore#aaaaaaaa(子)")
    );
    assert_eq!(overview.source_paths.len(), 3);

    // 弹窗口径:marker + 该子代理会话的完整消息;fork 不是子代理,不可寻址。
    let agent_messages = session::timeline::get_session_agent_messages_inner(
        SourceApp::Pi,
        "pi-main",
        "pi-child-a",
    )?;
    assert_eq!(agent_messages.len(), 3);
    assert_eq!(agent_messages[0].session_id.as_deref(), Some("pi-child-a"));
    let fork_messages = session::timeline::get_session_agent_messages_inner(
        SourceApp::Pi,
        "pi-main",
        "pi-fork",
    );
    assert!(fork_messages.is_err());

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

// 删除单位是 family:主会话与全部子代理会话文件一起清;fork 子会话不属于
// 该 family,保留。
#[test]
fn pi_deletes_family_members_together() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let main_path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-10-29T03-00-00-000Z",
        "pi-main",
    );
    let child_path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-10-29T03-01-00-000Z",
        "pi-child",
    );
    let fork_path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-10-29T03-02-00-000Z",
        "pi-fork",
    );

    write_jsonl(
        &main_path,
        &[pi_header("pi-main", "/tmp/pi-project", "2026-10-29T03:00:00.000Z")],
    )?;
    let mut child_header = pi_header("pi-child", "/tmp/pi-project", "2026-10-29T03:01:00.000Z");
    child_header["parentSession"] = json!(main_path.display().to_string());
    write_jsonl(
        &child_path,
        &[
            child_header,
            pi_entry(
                "session_info",
                "info-child",
                None,
                "2026-10-29T03:01:00.000Z",
                json!({ "name": "scout#cccccccc" }),
            ),
        ],
    )?;
    let mut fork_header = pi_header("pi-fork", "/tmp/pi-project", "2026-10-29T03:02:00.000Z");
    fork_header["parentSession"] = json!(main_path.display().to_string());
    write_jsonl(&fork_path, &[fork_header])?;

    let reader = session::reader(SourceApp::Pi);
    let plan = reader.delete_plan_input("pi-main")?;
    assert_eq!(plan.root_id, "pi-main");
    assert_eq!(plan.members.len(), 2);

    session::delete_session(SourceApp::Pi, "pi-main")?;
    assert!(!main_path.exists());
    assert!(!child_path.exists());
    assert!(fork_path.exists());

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

// 旧版扩展(workflow/async run)的子代理会话落在 <主会话stem>/<runId>/run-0/
// 嵌套目录,session_info 名是 "subagent-<agent>-<runId>-1"(无 # 后缀)。
// 通用判据(parentSession + session_info 有名)应同样归族。
#[test]
fn pi_folds_legacy_workflow_run_sessions_into_the_parent_family() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let main_path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-10-16T09-32-05-247Z",
        "pi-legacy-main",
    );
    write_jsonl(
        &main_path,
        &[
            pi_header("pi-legacy-main", "/tmp/pi-project", "2026-10-16T09:32:05.247Z"),
            pi_entry(
                "message",
                "legacy-user",
                None,
                "2026-10-16T09:32:06.000Z",
                json!({ "message": { "role": "user", "content": "legacy main" } }),
            ),
        ],
    )?;

    let run_dir = temp_home
        .join(".pi/agent/sessions/--tmp-pi-project--")
        .join("2026-10-16T09-32-05-247Z_pi-legacy-main")
        .join("d460ee11-run")
        .join("run-0");
    fs::create_dir_all(&run_dir)?;
    let mut run_header = pi_header("pi-legacy-run", "/tmp/pi-project", "2026-10-23T10:21:33.902Z");
    run_header["parentSession"] = json!(main_path.display().to_string());
    write_jsonl(
        &run_dir.join("session.jsonl"),
        &[
            run_header,
            pi_entry(
                "session_info",
                "run-info",
                None,
                "2026-10-23T10:21:33.904Z",
                json!({ "name": "subagent-scout-d460ee11-d658-4bde-854f-2220be32933b-1" }),
            ),
            pi_entry(
                "message",
                "run-user",
                None,
                "2026-10-23T10:21:34.000Z",
                json!({ "message": { "role": "user", "content": "run task" } }),
            ),
            pi_entry(
                "message",
                "run-assistant",
                Some("run-user"),
                "2026-10-23T10:21:35.000Z",
                json!({
                    "message": {
                        "role": "assistant",
                        "content": [{ "type": "text", "text": "run answer" }],
                        "provider": "reins",
                        "model": "deepseek-flash",
                        "usage": { "input": 10, "output": 5, "cacheRead": 0, "cacheWrite": 0 }
                    }
                }),
            ),
        ],
    )?;

    let reader = session::reader(SourceApp::Pi);
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    let summary = reader.parse_summary("pi-legacy-main")?;
    assert_eq!(summary.title, "legacy main (+1 subagents)");
    let usage = summary.token_usage.expect("run usage merged");
    assert_eq!(usage.input_tokens, 10);

    let detail = read_detail(reader, "pi-legacy-main")?;
    assert!(
        detail
            .messages
            .iter()
            .any(|message| message.session_id.as_deref() == Some("pi-legacy-run"))
    );
    let agent_messages = session::timeline::get_session_agent_messages_inner(
        SourceApp::Pi,
        "pi-legacy-main",
        "pi-legacy-run",
    )?;
    assert_eq!(agent_messages.len(), 3);

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

// subagent-tasks runner(task.json + output.jsonl 事件流)按 task.json 的
// sessionId 归族;时间线取 message_end 的最终消息,摘要从 assistant 消息合成。
#[test]
fn pi_folds_subagent_task_runs_into_the_parent_family() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let main_path = pi_path(
        &temp_home,
        "/tmp/pi-project",
        "2026-10-23T02-44-12-987Z",
        "pi-task-main",
    );
    write_jsonl(
        &main_path,
        &[
            pi_header("pi-task-main", "/tmp/pi-project", "2026-10-23T02:44:12.987Z"),
            pi_entry(
                "message",
                "task-main-user",
                None,
                "2026-10-23T02:44:13.000Z",
                json!({ "message": { "role": "user", "content": "main question" } }),
            ),
        ],
    )?;

    let task_dir = temp_home
        .join(".pi/agent/subagent-tasks")
        .join("e29ae30b-task");
    fs::create_dir_all(&task_dir)?;
    let event = |kind: &str, message: Value| {
        json!({ "type": kind, "message": message })
    };
    write_jsonl(
        &task_dir.join("output.jsonl"),
        &[
            event(
                "message_start",
                json!({ "role": "user", "content": "task prompt (streaming)" }),
            ),
            event(
                "message_end",
                json!({ "role": "user", "content": "task prompt", "timestamp": 1792724376000i64 }),
            ),
            event(
                "message_end",
                json!({
                    "role": "assistant",
                    "content": [{ "type": "text", "text": "task answer" }],
                    "provider": "reins",
                    "model": "deepseek-flash",
                    "usage": { "input": 100, "output": 50, "cacheRead": 10, "cacheWrite": 0 },
                    "timestamp": 1792724595685i64
                }),
            ),
        ],
    )?;
    fs::write(
        task_dir.join("task.json"),
        serde_json::to_vec(&json!({
            "version": 1,
            "id": "e29ae30b-task",
            "status": "completed",
            "agent": "scout",
            "sessionId": "pi-task-main",
            "createdAt": 1792724375849i64,
        }))?,
    )?;

    let reader = session::reader(SourceApp::Pi);
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    let summary = reader.parse_summary("pi-task-main")?;
    assert_eq!(summary.title, "main question (+1 subagents)");
    let usage = summary.token_usage.expect("task usage merged");
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 50);
    assert!(summary.models.iter().any(|model| model.model == "deepseek-flash"));

    let detail = read_detail(reader, "pi-task-main")?;
    // marker + 流式增量(message_start)不入时间线,只有 message_end 的最终消息
    let run_messages = detail
        .messages
        .iter()
        .filter(|message| {
            message.session_id.as_deref() == Some("e29ae30b-task")
                && !message.blocks.iter().any(|block| {
                    block
                        .payload
                        .as_ref()
                        .is_some_and(|payload| payload["type"] == "subagent_started")
                })
        })
        .count();
    assert_eq!(run_messages, 2);
    let texts = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter().filter_map(|block| block.text.clone()))
        .collect::<Vec<_>>();
    assert!(!texts.iter().any(|text| text.contains("streaming")));
    assert!(texts.iter().any(|text| text.contains("task answer")));

    let agent_messages = session::timeline::get_session_agent_messages_inner(
        SourceApp::Pi,
        "pi-task-main",
        "e29ae30b-task",
    )?;
    assert_eq!(agent_messages.len(), 3);

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}
