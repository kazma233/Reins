use super::*;
use crate::session::SessionReader;

// engine_at 的持久缓存目录：每个测试独立目录，避免跨测试串扰。
fn temp_store() -> PathBuf {
    env::temp_dir().join(format!("reins-store-{}", Uuid::new_v4()))
}

#[test]
fn opencode_root_session_aggregates_subagent_sessions() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-opencode-{}", Uuid::new_v4()));
    let root_id = "ses_root_session";
    let child_id = "ses_child_session";
    seed_opencode_family_at(&root, root_id, child_id)?;

    let reader = session::opencode::engine_at(root, temp_store());
    let summary = reader.parse_summary(root_id)?;
    assert_eq!(summary.source_session_id, root_id);
    assert!(summary.title.contains("+1 subagents"));

    let detail = read_detail(&reader, root_id)?;

    assert!(
        detail
            .messages
            .iter()
            .any(|message| message.blocks.iter().any(|block| {
                block
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("Sub-agent session: Child task"))
            }))
    );
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.kind == "subagent_started")
    );
    assert!(
        detail
            .source_paths
            .iter()
            .any(|path| path.ends_with(&format!(":{child_id}")))
    );

    Ok(())
}

#[test]
fn opencode_overview_counts_match_loaded_timeline() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-opencode-{}", Uuid::new_v4()));
    let root_id = "ses_root_session";
    let child_id = "ses_child_session";
    seed_opencode_family_at(&root, root_id, child_id)?;

    let reader = session::opencode::engine_at(root, temp_store());
    let overview = reader.parse_overview(root_id)?;
    let detail = read_detail(&reader, root_id)?;

    assert_eq!(overview.message_count, Some(detail.messages.len()));
    assert_eq!(overview.event_count, Some(detail.events.len()));

    Ok(())
}

#[test]
fn codex_ignores_agents_banner_when_picking_title() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-codex-{}", Uuid::new_v4()));
    let session_id = "33333333-3333-4333-8333-333333333333";
    let transcript_path = root
        .join("sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T12-00-00-{session_id}.jsonl"));
    if let Some(parent) = transcript_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let transcript = vec![
        json!({
            "timestamp": "2026-04-21T12:00:00.000Z",
            "type": "session_meta",
            "payload": {
                "id": session_id,
                "cwd": "/tmp/reins/workspace"
            }
        }),
        json!({
            "timestamp": "2026-04-21T12:00:01.000Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "# AGENTS.md instructions for /tmp/reins/workspace\n<system-reminder>\nYour operational mode has changed from plan to build.\nYou are no longer in read-only mode.\nYou are permitted to make file changes, run shell commands, and utilize your arsenal of tools as needed.\n</system-reminder>"
                    }
                ]
            }
        }),
        json!({
            "timestamp": "2026-04-21T12:00:02.000Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "How do I list the current files?"
                    }
                ]
            }
        }),
    ];

    write_jsonl(&transcript_path, &transcript)?;

    let summary = session::codex::engine_at(root, temp_store()).parse_summary(session_id)?;

    assert_eq!(summary.source_session_id, session_id);
    assert_eq!(summary.title, "How do I list the current files?");

    Ok(())
}

#[test]
fn codex_skips_agents_instructions_block_when_picking_title() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-codex-{}", Uuid::new_v4()));
    let session_id = "44444444-4444-4444-8444-444444444444";
    let transcript_path = root
        .join("sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T12-00-00-{session_id}.jsonl"));
    if let Some(parent) = transcript_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let transcript = vec![
        json!({
            "timestamp": "2026-04-21T12:00:00.000Z",
            "type": "session_meta",
            "payload": {
                "id": session_id,
                "cwd": "/tmp/reins/workspace"
            }
        }),
        json!({
            "timestamp": "2026-04-21T12:00:01.000Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "# AGENTS.md instructions for /tmp/reins/workspace\n\n<INSTRUCTIONS>\n# 通用偏好\n\n- 使用中文回复，回复简洁直接。\n- 代码注释使用中文。\n</INSTRUCTIONS>"
                    },
                    {
                        "type": "text",
                        "text": "<environment_context>\n  <cwd>/tmp/reins/workspace</cwd>\n  <shell>zsh</shell>\n</environment_context>"
                    }
                ]
            }
        }),
        json!({
            "timestamp": "2026-04-21T12:00:02.000Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "帮我看一下这个报错"
                    }
                ]
            }
        }),
    ];

    write_jsonl(&transcript_path, &transcript)?;

    let summary = session::codex::engine_at(root, temp_store()).parse_summary(session_id)?;

    assert_eq!(summary.source_session_id, session_id);
    assert_eq!(summary.title, "帮我看一下这个报错");

    Ok(())
}

#[test]
fn claude_ignores_agents_banner_when_picking_title() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-claude-{}", Uuid::new_v4()));
    let session_file = root.join("projects/demo-project").join("session-123.jsonl");
    let transcript = vec![
        json!({
            "type": "user",
            "message": {
                "content": "# AGENTS.md instructions for /tmp/reins/workspace\n<system-reminder>\nYour operational mode has changed from plan to build.\nYou are no longer in read-only mode.\nYou are permitted to make file changes, run shell commands, and utilize your arsenal of tools as needed.\n</system-reminder>"
            }
        }),
        json!({
            "type": "user",
            "message": {
                "content": "How do I list the current files?"
            }
        }),
    ];

    write_jsonl(&session_file, &transcript)?;

    let summary =
        session::claude_code::engine_at(root, temp_store()).parse_summary("session-123")?;

    assert_eq!(summary.title, "How do I list the current files?");

    Ok(())
}

#[test]
fn claude_keeps_injected_context_as_readable_block() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-claude-{}", Uuid::new_v4()));
    let session_file = root
        .join("projects/demo-project")
        .join("session-context.jsonl");
    write_jsonl(
        &session_file,
        &[
            // 字符串正文的注入上下文（Claude Code 的典型形态）：清洗后没有对话内容
            json!({
                "type": "user",
                "message": {
                    "content": "# AGENTS.md instructions for /tmp/reins/workspace\n<system-reminder>\nYour operational mode has changed from plan to build.\n</system-reminder>"
                }
            }),
        ],
    )?;

    let reader = session::claude_code::engine_at(root, temp_store());
    let detail = read_detail(&reader, "session-context")?;
    let block = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .find(|block| block.kind == "context_injection")
        .expect("context injection block");
    let text = block.text.as_deref().unwrap_or_default();
    assert!(text.contains("AGENTS.md instructions for /tmp/reins/workspace"));
    assert!(text.contains("<system-reminder>"));
    assert!(!detail.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.kind == "empty_message")
    }));

    Ok(())
}

#[test]
fn claude_turns_image_blocks_into_renderable_image_blocks() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-claude-{}", Uuid::new_v4()));
    let session_file = root
        .join("projects/demo-project")
        .join("session-image.jsonl");
    write_jsonl(
        &session_file,
        &[json!({
            "type": "user",
            "message": {
                "content": [
                    { "type": "text", "text": "看下这两张图" },
                    {
                        "type": "image",
                        "source": { "type": "base64", "media_type": "image/jpeg", "data": "/9j/4AAQ" }
                    },
                    {
                        "type": "image",
                        "source": { "type": "url", "url": "https://example.invalid/a.png" }
                    }
                ]
            }
        })],
    )?;

    let reader = session::claude_code::engine_at(root, temp_store());
    let detail = read_detail(&reader, "session-image")?;
    let references: Vec<&str> = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter(|block| block.kind == "image")
        .filter_map(|block| block.text.as_deref())
        .collect();
    assert_eq!(
        references,
        vec![
            "data:image/jpeg;base64,/9j/4AAQ",
            "https://example.invalid/a.png"
        ]
    );

    Ok(())
}

#[test]
fn claude_renders_local_command_records_as_readable_blocks() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-claude-{}", Uuid::new_v4()));
    let session_file = root
        .join("projects/demo-project")
        .join("session-local-command.jsonl");
    write_jsonl(
        &session_file,
        &[
            json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": "<local-command-caveat>The command below was run directly in Claude Code, not sent to you as a request, and its output goes straight to the user. It's recorded here as context for later messages.</local-command-caveat>"
                }
            }),
            json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": "<command-name>/exit</command-name>\n            <command-message>exit</command-message>\n            <command-args></command-args>"
                }
            }),
            json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": "<local-command-stdout>See ya!</local-command-stdout>"
                }
            }),
        ],
    )?;

    let reader = session::claude_code::engine_at(root, temp_store());
    let detail = read_detail(&reader, "session-local-command")?;
    let blocks: Vec<(&str, Option<&str>)> = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .map(|block| (block.kind.as_str(), block.text.as_deref()))
        .collect();

    assert!(blocks.contains(&("local_command", Some("/exit"))));
    assert!(blocks.contains(&("local_command_output", Some("See ya!"))));
    // caveat 是记录用上下文，归到上下文块
    assert!(
        blocks
            .iter()
            .any(|(kind, text)| *kind == "context_injection"
                && text.is_some_and(|text| text.starts_with("<local-command-caveat>")))
    );
    // 三条都不是对话内容，但都不再退化成原始报文兜底块
    assert!(!blocks.iter().any(|(kind, _)| *kind == "empty_message"));

    Ok(())
}

#[test]
fn claude_skips_unreadable_files_when_indexing() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-claude-{}", Uuid::new_v4()));
    let valid_path = root.join("projects/demo-project").join("session-123.jsonl");
    let invalid_path = root.join("projects/demo-project").join("broken.jsonl");

    write_jsonl(
        &valid_path,
        &[json!({
            "type": "user",
            "message": {
                "content": "Valid task"
            }
        })],
    )?;

    fs::write(&invalid_path, "{ not valid json }\n")?;

    let reader = session::claude_code::engine_at(root, temp_store());
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, valid_path);

    Ok(())
}

#[test]
fn claude_exposes_unsupported_message_content() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-claude-{}", Uuid::new_v4()));
    let session_file = root
        .join("projects/demo-project")
        .join("session-unsupported.jsonl");
    write_jsonl(
        &session_file,
        &[json!({
            "timestamp": "2026-04-21T12:00:00.000Z",
            "sessionId": "session-unsupported",
            "type": "assistant",
            "message": {
                "content": {
                    "unexpected": true
                }
            }
        })],
    )?;

    let reader = session::claude_code::engine_at(root, temp_store());
    let detail = read_detail(&reader, "session-unsupported")?;

    assert!(detail.messages.iter().any(|message| {
        message.blocks.iter().any(|block| {
            block.kind == "unsupported_content"
                && block
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("\"unexpected\": true"))
        })
    }));

    Ok(())
}

#[test]
fn codex_turns_input_image_into_renderable_image_block() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-codex-{}", Uuid::new_v4()));
    let session_id = "88888888-8888-4888-8888-888888888888";
    let transcript_path = root
        .join("sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T12-00-00-{session_id}.jsonl"));
    write_jsonl(
        &transcript_path,
        &[
            json!({
                "timestamp": "2026-04-21T12:00:00.000Z",
                "type": "session_meta",
                "payload": { "id": session_id, "cwd": "/tmp/reins/workspace" }
            }),
            // 粘贴的图片：没有 text 字段，引用要提成正文而不是当成未支持块
            json!({
                "timestamp": "2026-04-21T12:00:01.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "看下这张图" },
                        {
                            "type": "input_image",
                            "detail": "high",
                            "image_url": "data:image/png;base64,iVBORw0KGgo="
                        }
                    ]
                }
            }),
            json!({
                "timestamp": "2026-04-21T12:00:02.000Z",
                "type": "response_item",
                "payload": {
                    "type": "reasoning",
                    "content": null,
                    "encrypted_content": "SYNTHETIC_BLOB"
                }
            }),
        ],
    )?;

    let detail = read_detail(&session::codex::engine_at(root, temp_store()), session_id)?;
    let block = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .find(|block| block.kind == "image")
        .expect("image block");
    assert_eq!(
        block.text.as_deref(),
        Some("data:image/png;base64,iVBORw0KGgo=")
    );
    assert!(!detail.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.kind == "unsupported_block")
    }));
    // raw 事件给整条记录：reasoning 的 payload 也带 timestamp/type 信封
    let reasoning = detail
        .events
        .iter()
        .find(|event| event.kind == "reasoning")
        .expect("reasoning event");
    let payload = reasoning.payload.as_ref().expect("raw payload");
    assert_eq!(
        payload.get("type").and_then(Value::as_str),
        Some("response_item")
    );
    assert_eq!(
        payload.get("timestamp").and_then(Value::as_str),
        Some("2026-04-21T12:00:02.000Z")
    );
    // 加密字段是唯一例外
    assert!(!serde_json::to_string(payload)?.contains("SYNTHETIC_BLOB"));

    Ok(())
}

#[test]
fn codex_keeps_injected_context_as_readable_block() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-codex-{}", Uuid::new_v4()));
    let session_id = "77777777-7777-4777-8777-777777777777";
    let transcript_path = root
        .join("sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T12-00-00-{session_id}.jsonl"));
    write_jsonl(
        &transcript_path,
        &[
            json!({
                "timestamp": "2026-04-21T12:00:00.000Z",
                "type": "session_meta",
                "payload": { "id": session_id, "cwd": "/tmp/reins/workspace" }
            }),
            // 宿主注入的上下文消息：清洗后没有对话内容，但整段原文要能看
            json!({
                "timestamp": "2026-04-21T12:00:01.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {
                            "type": "input_text",
                            "text": "# AGENTS.md instructions for /tmp/reins/workspace\n\n<INSTRUCTIONS>\n使用中文回复。\n</INSTRUCTIONS>"
                        },
                        {
                            "type": "input_text",
                            "text": "<environment_context>\n  <cwd>/tmp/reins/workspace</cwd>\n</environment_context>"
                        }
                    ],
                    "internal_chat_message_metadata_passthrough": {
                        "content_item_kinds": [
                            "agents_md.instructions",
                            "environments.environment_context"
                        ]
                    }
                }
            }),
        ],
    )?;

    let detail = read_detail(&session::codex::engine_at(root, temp_store()), session_id)?;
    let block = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .find(|block| block.kind == "context_injection")
        .expect("context injection block");
    let text = block.text.as_deref().unwrap_or_default();
    assert!(text.contains("AGENTS.md instructions for /tmp/reins/workspace"));
    assert!(text.contains("<environment_context>"));
    // 不再退化成一行原始报文诊断块
    assert!(!detail.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.kind == "empty_message")
    }));

    Ok(())
}

#[test]
fn codex_exposes_unsupported_message_content_and_blocks() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-codex-{}", Uuid::new_v4()));
    let session_id = "66666666-6666-4666-8666-666666666666";
    let transcript_path = root
        .join("sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T12-00-00-{session_id}.jsonl"));
    write_jsonl(
        &transcript_path,
        &[
            json!({
                "timestamp": "2026-04-21T12:00:00.000Z",
                "type": "session_meta",
                "payload": {
                    "id": session_id,
                    "cwd": "/tmp/reins/workspace"
                }
            }),
            json!({
                "timestamp": "2026-04-21T12:00:01.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": {
                        "unexpected": true
                    }
                }
            }),
            json!({
                "timestamp": "2026-04-21T12:00:02.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        {
                            "type": "new_block_shape",
                            "value": 42
                        }
                    ]
                }
            }),
        ],
    )?;

    let detail = read_detail(&session::codex::engine_at(root, temp_store()), session_id)?;

    assert!(detail.messages.iter().any(|message| {
        message.blocks.iter().any(|block| {
            block.kind == "unsupported_content"
                && block
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("\"unexpected\": true"))
        })
    }));
    assert!(detail.messages.iter().any(|message| {
        message.blocks.iter().any(|block| {
            block.kind == "unsupported_block"
                && block
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("\"new_block_shape\""))
        })
    }));

    Ok(())
}

#[test]
fn opencode_exposes_messages_without_visible_parts() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-opencode-{}", Uuid::new_v4()));
    let db_path = root.join("opencode.db");
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(&db_path)?;
    connection.execute_batch(
        "
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            parent_id TEXT,
            slug TEXT NOT NULL,
            directory TEXT NOT NULL,
            title TEXT,
            version TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL
        );
        CREATE TABLE session_message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            type TEXT NOT NULL,
            seq INTEGER NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        ",
    )?;

    let session_id = "ses_empty_parts";
    connection.execute(
        "INSERT INTO session_v2 (id, project_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            session_id,
            "project-1",
            session_id,
            "/tmp/root",
            "Empty parts",
            "1",
            1_744_366_400_000_i64,
            1_744_366_400_000_i64
        ],
    )?;
    // assistant 行的 data 没有 content 数组：不产生可见块，应回退为
    // empty_message 诊断块并携带原始 data 作为 payload
    connection.execute(
        "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "msg-empty",
            session_id,
            "assistant",
            1,
            1_744_366_400_000_i64,
            1_744_366_400_000_i64,
            serde_json::to_string(&json!({
                "time": { "created": 1_744_366_400_000_i64 },
                "agent": "build"
            }))?
        ],
    )?;

    let detail = read_detail(
        &session::opencode::engine_at(root, temp_store()),
        session_id,
    )?;

    assert!(detail.messages.iter().any(|message| {
        message.blocks.iter().any(|block| {
            block.kind == "empty_message"
                && block
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("\"agent\": \"build\""))
        })
    }));

    Ok(())
}

#[test]
fn opencode_v2_parses_tool_content_and_non_message_events() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-opencode-{}", Uuid::new_v4()));
    let db_path = root.join("opencode.db");
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(&db_path)?;
    connection.execute_batch(
        "
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            parent_id TEXT,
            slug TEXT NOT NULL,
            directory TEXT NOT NULL,
            title TEXT,
            version TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL
        );
        CREATE TABLE session_message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            type TEXT NOT NULL,
            seq INTEGER NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        ",
    )?;

    let session_id = "ses_v2_content";
    connection.execute(
        "INSERT INTO session_v2 (id, project_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            session_id,
            "project-1",
            session_id,
            "/tmp/root",
            "V2 content",
            "1",
            1_744_366_400_000_i64,
            1_744_366_400_000_i64
        ],
    )?;

    // v2 assistant 消息：工具输入在 state.input、输出在 state.content 块数组
    let message_time = 1_744_366_400_000_i64;
    connection.execute(
        "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "msg-assistant",
            session_id,
            "assistant",
            1,
            message_time,
            message_time,
            serde_json::to_string(&json!({
                "time": { "created": message_time },
                "agent": "build",
                "content": [
                    { "type": "reasoning", "text": "thinking out loud" },
                    {
                        "type": "tool",
                        "id": "call_v2_1",
                        "name": "grep",
                        "state": {
                            "status": "completed",
                            "input": { "pattern": "session_v2" },
                            "content": [
                                { "type": "text", "text": "match line 1" },
                                { "type": "text", "text": "match line 2" }
                            ],
                            "metadata": {}
                        }
                    },
                    { "type": "text", "text": "done" }
                ]
            }))?
        ],
    )?;

    // 非 user/assistant 行归入事件，kind 用 type 列
    let idle_time = 1_744_366_401_000_i64;
    connection.execute(
        "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "msg-idle",
            session_id,
            "idle",
            2,
            idle_time,
            idle_time,
            serde_json::to_string(&json!({
                "time": { "created": idle_time },
                "outcome": "succeeded"
            }))?
        ],
    )?;
    let compaction_time = 1_744_366_402_000_i64;
    connection.execute(
        "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "msg-compaction",
            session_id,
            "compaction",
            3,
            compaction_time,
            compaction_time,
            serde_json::to_string(&json!({
                "status": "completed",
                "reason": "auto",
                "summary": "## 总结：v2 读取改造"
            }))?
        ],
    )?;

    let detail = read_detail(
        &session::opencode::engine_at(root, temp_store()),
        session_id,
    )?;

    // 消息时间线只有 user/assistant 行
    assert_eq!(detail.messages.len(), 1);
    let blocks = &detail.messages[0].blocks;
    assert_eq!(blocks[0].kind, "thinking");
    assert_eq!(blocks[0].text.as_deref(), Some("thinking out loud"));

    let call = &blocks[1];
    assert_eq!(call.kind, "function_call");
    assert_eq!(call.tool_name.as_deref(), Some("grep"));
    assert_eq!(call.tool_call_id.as_deref(), Some("call_v2_1"));
    assert!(
        call.text
            .as_deref()
            .is_some_and(|text| text.contains("session_v2"))
    );

    let output = &blocks[2];
    assert_eq!(output.kind, "function_call_output");
    assert_eq!(output.tool_call_id.as_deref(), Some("call_v2_1"));
    assert_eq!(output.text.as_deref(), Some("match line 1\nmatch line 2"));

    assert_eq!(blocks[3].kind, "text");
    assert_eq!(blocks[3].text.as_deref(), Some("done"));

    // idle/compaction 进事件面板，摘要带最有信息量的字段
    assert_eq!(detail.events.len(), 2);
    let idle = detail
        .events
        .iter()
        .find(|event| event.kind == "idle")
        .expect("idle event");
    assert_eq!(idle.summary, "idle: succeeded");
    // raw payload 给整行：id/type/time_created + data 列原文
    let idle_payload = idle.payload.as_ref().expect("raw payload is the row");
    assert_eq!(
        idle_payload.get("type").and_then(Value::as_str),
        Some("idle")
    );
    assert_eq!(
        idle_payload.get("id").and_then(Value::as_str),
        Some("msg-idle")
    );
    assert!(idle_payload.get("time_created").is_some());
    assert!(idle_payload.get("data").is_some());
    let compaction = detail
        .events
        .iter()
        .find(|event| event.kind == "compaction")
        .expect("compaction event");
    assert!(
        compaction.summary.contains("v2 读取改造"),
        "unexpected compaction summary: {}",
        compaction.summary
    );

    Ok(())
}

#[test]
fn claude_root_session_aggregates_subagent_sessions() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-claude-{}", Uuid::new_v4()));
    let root_id = "root-session";
    let child_id = "child-session";
    let project_dir = root.join("projects/demo-project");
    let root_path = project_dir.join(format!("{root_id}.jsonl"));
    let child_path = project_dir
        .join("subagents")
        .join(format!("agent-{child_id}.jsonl"));
    let child_meta_path = project_dir
        .join("subagents")
        .join(format!("agent-{child_id}.meta.json"));

    write_jsonl(
        &root_path,
        &[
            json!({
                "timestamp": "2026-04-21T12:00:00.000Z",
                "sessionId": root_id,
                "type": "user",
                "message": {
                    "content": "Main task"
                }
            }),
            json!({
                "timestamp": "2026-04-21T12:00:01.000Z",
                "sessionId": root_id,
                "type": "assistant",
                "message": {
                    "content": "Root answer"
                }
            }),
        ],
    )?;

    write_jsonl(
        &child_path,
        &[
            json!({
                "timestamp": "2026-04-21T15:00:00.000Z",
                "sessionId": root_id,
                "type": "user",
                "message": {
                    "content": "Child task"
                }
            }),
            json!({
                "timestamp": "2026-04-21T15:00:01.000Z",
                "sessionId": root_id,
                "type": "assistant",
                "message": {
                    "content": "Child answer"
                }
            }),
        ],
    )?;

    fs::write(
        &child_meta_path,
        r#"{"agentType":"Explore","description":"Find fetch_rss scheduling code"}"#,
    )?;

    let reader = session::claude_code::engine_at(root, temp_store());
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, root_path);
    assert!(
        entries[0]
            .summary
            .as_ref()
            .is_some_and(|summary| summary.title.contains("+1 subagents"))
    );

    let summary = reader.parse_summary(root_id)?;
    assert!(summary.title.contains("+1 subagents"));

    let overview = reader.parse_overview(root_id)?;
    assert_eq!(overview.summary.source_session_id, root_id);
    assert_eq!(overview.agents.len(), 2);
    assert_eq!(
        overview
            .agents
            .iter()
            .map(|agent| agent.session_id.as_str())
            .collect::<HashSet<_>>()
            .len(),
        2
    );
    assert!(
        overview
            .agents
            .iter()
            .any(|agent| agent.is_root && agent.session_id == root_id && agent.label == "主 Agent")
    );
    assert!(overview.agents.iter().any(|agent| {
        !agent.is_root
            && agent.session_id == child_id
            && agent.label == "Find fetch_rss scheduling code(子)"
    }));

    let detail = read_detail(&reader, root_id)?;
    // Path equality treats '/' and '\' as equivalent on Windows, unlike the
    // raw string comparison against the mixed-separator test paths.
    assert_eq!(detail.source_paths.len(), 2);
    assert!(
        detail
            .source_paths
            .iter()
            .any(|path| Path::new(path) == root_path)
    );
    assert!(
        detail
            .source_paths
            .iter()
            .any(|path| Path::new(path) == child_path)
    );
    assert!(detail.messages.iter().any(|message| {
        message.blocks.iter().any(|block| {
            block.text.as_deref().is_some_and(|text| {
                text.contains("Sub-agent session: Find fetch_rss scheduling code")
            })
        })
    }));
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.kind == "subagent_started")
    );

    Ok(())
}

// 末段断言走 session::timeline::get_session_agent_messages_inner(命令层入口,
// 经注册表读 env BACKEND),因此整个用例保持 env 根,不迁移 engine_at。
#[test]
fn codex_root_session_aggregates_subagent_sessions() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let root_id = "44444444-4444-4444-8444-444444444444";
    let child_id = "55555555-5555-4555-8555-555555555555";
    let root_path = temp_home
        .join(".codex/sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T12-00-00-{root_id}.jsonl"));
    let child_path = temp_home
        .join(".codex/sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T15-00-00-{child_id}.jsonl"));

    write_jsonl(
        &root_path,
        &[
            json!({
                "timestamp": "2026-04-21T12:00:00.000Z",
                "type": "session_meta",
                "payload": {
                    "id": root_id,
                    "cwd": "/tmp/reins/workspace"
                }
            }),
            json!({
                "timestamp": "2026-04-21T12:00:01.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "Main task" }]
                }
            }),
            json!({
                "timestamp": "2026-04-21T12:00:02.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "Main answer" }]
                }
            }),
        ],
    )?;

    write_jsonl(
        &child_path,
        &[
            json!({
                "timestamp": "2026-04-21T15:00:00.000Z",
                "type": "session_meta",
                "payload": {
                    "id": child_id,
                    "cwd": "/tmp/reins/workspace",
                    "source": {
                        "subagent": {
                            "thread_spawn": {
                                "parent_thread_id": root_id,
                                "agent_nickname": "Plato",
                                "agent_role": "worker"
                            }
                        }
                    },
                    "agent_nickname": "Plato",
                    "agent_role": "worker"
                }
            }),
            json!({
                "timestamp": "2026-04-21T15:00:01.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "Child task" }]
                }
            }),
            json!({
                "timestamp": "2026-04-21T15:00:02.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "Child answer" }]
                }
            }),
        ],
    )?;

    let summary = session::reader(SourceApp::Codex).parse_summary(root_id)?;
    assert_eq!(summary.source_session_id, root_id);
    assert!(summary.title.contains("+1 subagents"));

    let detail = read_detail(session::reader(SourceApp::Codex), root_id)?;

    assert!(
        detail
            .messages
            .iter()
            .any(|message| message.blocks.iter().any(|block| {
                block
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("Sub-agent session: Plato"))
            }))
    );
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.kind == "subagent_started")
    );
    assert!(
        detail
            .source_paths
            .iter()
            .any(|path| path.ends_with(&format!("{child_id}.jsonl")))
    );

    // 子代理弹窗取数：只返回该 agent 的消息(marker + 子会话消息),与分页进度无关。
    // 走命令层入口而非 reader,连 resolve_session_path 一起覆盖
    let child_messages =
        session::timeline::get_session_agent_messages_inner(SourceApp::Codex, root_id, child_id)?;
    assert_eq!(child_messages.len(), 3);
    assert!(
        child_messages
            .iter()
            .all(|message| message.session_id.as_deref() == Some(child_id))
    );
    assert!(child_messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("Child answer"))
    }));
    assert!(!child_messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("Main answer"))
    }));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn codex_resume_segments_keep_original_segment_as_root() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-codex-{}", Uuid::new_v4()));
    let session_id = "88888888-8888-4888-8888-888888888888";
    let sessions_dir = root.join("sessions/2026/09/02");
    let original_path =
        sessions_dir.join(format!("rollout-2026-09-02T16-00-15-{session_id}.jsonl"));
    let resume_path = sessions_dir.join(format!(
        "rollout-2026-09-02T17-32-03-{session_id}_99999999-9999-4999-8999-999999999999.jsonl"
    ));

    write_jsonl(
        &original_path,
        &[
            json!({
                "timestamp": "2026-09-02T08:00:15.000Z",
                "type": "session_meta",
                "payload": {
                    "id": session_id,
                    "timestamp": "2026-09-02T08:00:15.000Z",
                    "cwd": "/tmp/reins/workspace"
                }
            }),
            json!({
                "timestamp": "2026-09-02T08:00:16.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "Original task" }]
                }
            }),
        ],
    )?;

    // resume 续跑段：与原始段共享 session id，靠 history_base 串链。
    write_jsonl(
        &resume_path,
        &[
            json!({
                "timestamp": "2026-09-02T09:32:03.000Z",
                "type": "session_meta",
                "payload": {
                    "id": session_id,
                    "timestamp": "2026-09-02T09:32:03.000Z",
                    "cwd": "/tmp/reins/workspace",
                    "history_base": {
                        "thread_id": session_id,
                        "end_ordinal_exclusive": 834,
                        "end_byte_offset": 3018401
                    }
                }
            }),
            json!({
                "timestamp": "2026-09-02T09:32:04.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "继续" }]
                }
            }),
        ],
    )?;

    let reader = session::codex::engine_at(root, temp_store());
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    // root 必须是原始段：标题取自原始段首条消息，而不是续跑段的"继续"。
    assert_eq!(entries[0].path, original_path);
    let summary = entries[0].summary.as_ref().expect("summary");
    assert_eq!(summary.title, "Original task");
    // 续跑段与原始段共享 session id，是同一条线程而不是 subagent。
    assert!(!summary.title.contains("subagents"));

    Ok(())
}

#[test]
fn codex_title_strips_markdown_link_syntax() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-codex-{}", Uuid::new_v4()));
    let session_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let transcript_path = root
        .join("sessions/2026/09/02")
        .join(format!("rollout-2026-09-02T16-00-15-{session_id}.jsonl"));

    write_jsonl(
        &transcript_path,
        &[
            json!({
                "timestamp": "2026-09-02T08:00:15.000Z",
                "type": "session_meta",
                "payload": {
                    "id": session_id,
                    "timestamp": "2026-09-02T08:00:15.000Z",
                    "cwd": "/tmp/reins/workspace"
                }
            }),
            json!({
                "timestamp": "2026-09-02T08:00:16.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{
                        "type": "input_text",
                        "text": "[$mongo-slow-log-report]($HOME/Desktop/skills/mongo-slow-log-report/SKILL.md) 帮我看下这份skill有没有问题"
                    }]
                }
            }),
        ],
    )?;

    let entries = session::codex::engine_at(root, temp_store()).list_entries()?;
    assert_eq!(entries.len(), 1);
    // 链接语法必须剥掉只留 label，否则 72 字符截断后标题全是 [label](url)。
    assert_eq!(
        entries[0].summary.as_ref().expect("summary").title,
        "$mongo-slow-log-report 帮我看下这份skill有没有问题"
    );

    Ok(())
}
