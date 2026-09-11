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
    let summary = reader.parse_summary(&path)?;
    assert_eq!(summary.title, "Named Pi session");
    let detail = reader.parse_detail(&path)?;
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
    assert_eq!(detail.source_paths, vec![path.display().to_string()]);
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.kind == "parent_session")
    );
    let overview = reader.parse_overview(&path)?;
    assert_eq!(overview.message_count, Some(detail.messages.len()));
    assert_eq!(overview.event_count, Some(detail.events.len()));
    assert_eq!(overview.agents.len(), 1);
    let message_page = reader.parse_messages_page(&path, 1, 2)?;
    assert_eq!(message_page.total_count, detail.messages.len());
    assert_eq!(message_page.offset, 1);
    assert_eq!(message_page.messages.len(), 2);
    // compaction 展开后链上有 4 条消息(含 compactionSummary 与 retainedTail),还有第 3 页
    assert_eq!(message_page.next_offset, Some(3));
    let event_page = reader.parse_events_page(&path, 1, 3)?;
    assert_eq!(event_page.total_count, detail.events.len());
    assert_eq!(event_page.offset, 1);
    assert_eq!(event_page.events.len(), 3);
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
            "model": "fanggeek/gpt-5.6-terra",
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
    let detail = reader.parse_detail(&path)?;
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
    let normal_detail = reader.parse_detail(&normal_path)?;
    assert_eq!(normal_detail.messages[0].blocks[0].kind, "tool_result");

    // Pi 的 subagent 内嵌在 toolResult.details 里,没有子会话可查:
    // 必须报错而不是返回空,否则调用方无法区分"不支持"与"空子会话"
    assert!(reader.parse_agent_messages(&path, "any-agent-id").is_err());
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

    let detail = session::reader(SourceApp::Pi).parse_detail(&path)?;
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

    let detail = session::reader(SourceApp::Pi).parse_detail(&path)?;
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

    let detail = session::reader(SourceApp::Pi).parse_detail(&path)?;
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

    let detail = session::reader(SourceApp::Pi).parse_detail(&path)?;
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
fn pi_import_into_configured_dir_writes_at_dir_root() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let guard = TestEnvGuard::set_home(&temp_home);

    // 默认布局:文件落在 <agentDir>/sessions/<encoded-cwd>/ 下
    let (_, default_paths) =
        session::pi::write_session(&sample_detail(SourceApp::Codex), "pi-layout-default")?;
    assert!(default_paths[0].contains("--tmp-reins-workspace--"));

    // 官方契约:显式配置的 sessionDir 是最终目录,文件直接写在其根下
    let custom_dir = temp_home.join("custom-pi-sessions");
    fs::create_dir_all(&custom_dir)?;
    guard.set_pi_session_dir(&custom_dir);
    let (_, custom_paths) =
        session::pi::write_session(&sample_detail(SourceApp::Codex), "pi-layout-custom")?;
    assert_eq!(
        Path::new(&custom_paths[0]).parent(),
        Some(custom_dir.as_path())
    );
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
        Some(path.to_string_lossy().as_ref()),
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
    let detail = reader.parse_detail(&path)?;
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
    let summary = reader.parse_summary(&path)?;
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

    let detail = session::reader(SourceApp::Pi).parse_detail(&path)?;
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
fn pi_roundtrip_preserves_tool_failure_state() -> Result<()> {
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

    let detail = session::reader(SourceApp::Pi).parse_detail(&path)?;
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

    let (_, paths) = session::pi::write_session(&detail, "pi-failure-rt")?;
    let lines = fs::read_to_string(&paths[0])?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let bash = lines
        .iter()
        .find(|line| line["message"]["role"] == "bashExecution")
        .expect("bash execution entry");
    assert_eq!(bash["message"]["command"], "exit 1");
    assert_eq!(bash["message"]["output"], "boom");
    assert_eq!(bash["message"]["exitCode"], 1);
    assert_eq!(bash["message"]["cancelled"], true);
    assert_eq!(bash["message"]["truncated"], true);
    let tool_result = lines
        .iter()
        .find(|line| line["message"]["role"] == "toolResult")
        .expect("tool result entry");
    assert_eq!(tool_result["message"]["isError"], true);
    Ok(())
}

#[test]
fn pi_export_entry_timestamps_follow_message_time() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let (_, paths) = session::pi::write_session(&sample_detail(SourceApp::Codex), "pi-clocks")?;
    let lines = fs::read_to_string(&paths[0])?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // 第一条消息的外层 timestamp 跟随消息时间(session_info 占用创建时间,消息紧随其后)
    let first_message = lines
        .iter()
        .find(|line| line["type"] == "message")
        .expect("first message entry");
    let outer = support::time::parse_timestamp(first_message["timestamp"].as_str().unwrap())
        .expect("outer timestamp");
    assert_eq!(outer, 1_744_366_400_001_i64);

    // header 与 session_info 同为创建时刻;其余 entry 外层 timestamp 严格递增
    let mut previous: Option<i64> = None;
    for line in lines.iter().skip(1) {
        let Some(raw) = line["timestamp"].as_str() else {
            continue;
        };
        let timestamp = support::time::parse_timestamp(raw).expect("valid timestamp");
        if let Some(previous) = previous {
            assert!(timestamp > previous, "entry timestamps must increase");
        }
        previous = Some(timestamp);
    }

    // 倒序消息时间也不会让文件内时间回退
    let mut detail = sample_detail(SourceApp::Codex);
    for (index, message) in detail.messages.iter_mut().enumerate() {
        message.timestamp = Some(1_744_366_400_000 - (index as i64) * 10_000);
    }
    let (_, paths) = session::pi::write_session(&detail, "pi-clocks-reverse")?;
    let lines = fs::read_to_string(&paths[0])?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut previous: Option<i64> = None;
    for line in lines.iter().skip(1) {
        let Some(raw) = line["timestamp"].as_str() else {
            continue;
        };
        let timestamp = support::time::parse_timestamp(raw).expect("valid timestamp");
        if let Some(previous) = previous {
            assert!(timestamp > previous, "reverse clock must stay monotonic");
        }
        previous = Some(timestamp);
    }
    Ok(())
}
