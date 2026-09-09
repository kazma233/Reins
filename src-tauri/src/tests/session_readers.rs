use super::*;

#[test]
fn opencode_root_session_aggregates_subagent_sessions() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let root_id = "ses_root_session";
    let child_id = "ses_child_session";
    seed_opencode_family(root_id, child_id)?;

    let root_path = temp_home
        .join(".local/share/opencode/session")
        .join(format!("{root_id}.opencode"));

    let summary = session::reader(SourceApp::OpenCode).parse_summary(&root_path)?;
    assert_eq!(summary.source_session_id, root_id);
    assert!(summary.title.contains("+1 subagents"));

    let detail = session::timeline::get_session_inner(
        SourceApp::OpenCode,
        root_id,
        Some(root_path.to_string_lossy().as_ref()),
    )?;

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
            .any(|path| path.ends_with(&format!("{child_id}.opencode")))
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn opencode_overview_counts_match_loaded_timeline() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let root_id = "ses_root_session";
    let child_id = "ses_child_session";
    seed_opencode_family(root_id, child_id)?;

    let root_path = temp_home
        .join(".local/share/opencode/session")
        .join(format!("{root_id}.opencode"));
    let reader = session::reader(SourceApp::OpenCode);
    let overview = reader.parse_overview(&root_path)?;
    let detail = reader.parse_detail(&root_path)?;

    assert_eq!(overview.message_count, Some(detail.messages.len()));
    assert_eq!(overview.event_count, Some(detail.events.len()));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn codex_ignores_agents_banner_when_picking_title() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_id = "33333333-3333-4333-8333-333333333333";
    let transcript_path = temp_home
        .join(".codex/sessions/2026/04/21")
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

    let summary = session::reader(SourceApp::Codex).parse_summary(&transcript_path)?;

    assert_eq!(summary.source_session_id, session_id);
    assert_eq!(summary.title, "How do I list the current files?");

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn codex_skips_agents_instructions_block_when_picking_title() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_id = "44444444-4444-4444-8444-444444444444";
    let transcript_path = temp_home
        .join(".codex/sessions/2026/04/21")
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

    let summary = session::reader(SourceApp::Codex).parse_summary(&transcript_path)?;

    assert_eq!(summary.source_session_id, session_id);
    assert_eq!(summary.title, "帮我看一下这个报错");

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn claude_ignores_agents_banner_when_picking_title() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_file = temp_home
        .join(".claude/projects/demo-project")
        .join("session-123.jsonl");
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

    let summary = session::reader(SourceApp::ClaudeCode).parse_summary(&session_file)?;

    assert_eq!(summary.title, "How do I list the current files?");

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn claude_skips_unreadable_files_when_indexing() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let valid_path = temp_home
        .join(".claude/projects/demo-project")
        .join("session-123.jsonl");
    let invalid_path = temp_home
        .join(".claude/projects/demo-project")
        .join("broken.jsonl");

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

    let entries = session::reader(SourceApp::ClaudeCode).list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, valid_path);

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn claude_exposes_unsupported_message_content() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_file = temp_home
        .join(".claude/projects/demo-project")
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

    let detail = session::reader(SourceApp::ClaudeCode).parse_detail(&session_file)?;

    assert!(detail.messages.iter().any(|message| {
        message.blocks.iter().any(|block| {
            block.kind == "unsupported_content"
                && block
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("\"unexpected\": true"))
        })
    }));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn codex_exposes_unsupported_message_content_and_blocks() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_id = "66666666-6666-4666-8666-666666666666";
    let transcript_path = temp_home
        .join(".codex/sessions/2026/04/21")
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

    let detail = session::reader(SourceApp::Codex).parse_detail(&transcript_path)?;

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

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn opencode_exposes_messages_without_visible_parts() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let db_path = session::opencode::db_path()?;
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(&db_path)?;
    connection.execute_batch(
        "
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            parent_id TEXT,
            slug TEXT NOT NULL,
            directory TEXT NOT NULL,
            title TEXT NOT NULL,
            version TEXT NOT NULL,
            share_url TEXT,
            summary_additions INTEGER,
            summary_deletions INTEGER,
            summary_files INTEGER,
            summary_diffs TEXT,
            revert TEXT,
            permission TEXT,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            time_compacting INTEGER,
            time_archived INTEGER,
            workspace_id TEXT
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        CREATE TABLE part (
            id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        ",
    )?;

    let session_id = "ses_empty_parts";
    connection.execute(
        "INSERT INTO session (id, project_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
    connection.execute(
        "INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "msg-empty",
            session_id,
            1_744_366_400_000_i64,
            1_744_366_400_000_i64,
            serde_json::to_string(&json!({
                "role": "assistant",
                "time": { "created": 1_744_366_400_000_i64 }
            }))?
        ],
    )?;

    let opencode_session_dir = session::opencode::root()?.join("session");
    fs::create_dir_all(&opencode_session_dir)?;
    let root_path = opencode_session_dir.join(format!("{session_id}.opencode"));
    File::create(&root_path)?;

    let detail = session::reader(SourceApp::OpenCode).parse_detail(&root_path)?;

    assert!(detail.messages.iter().any(|message| {
        message.blocks.iter().any(|block| {
            block.kind == "empty_message"
                && block
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("\"role\": \"assistant\""))
        })
    }));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn claude_root_session_aggregates_subagent_sessions() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let root_id = "root-session";
    let child_id = "child-session";
    let project_dir = temp_home.join(".claude/projects/demo-project");
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

    let reader = session::reader(SourceApp::ClaudeCode);
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, root_path);
    assert!(
        entries[0]
            .summary
            .as_ref()
            .is_some_and(|summary| summary.title.contains("+1 subagents"))
    );

    let summary = reader.parse_summary(&root_path)?;
    assert!(summary.title.contains("+1 subagents"));

    let overview = reader.parse_overview(&root_path)?;
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

    let detail = reader.parse_detail(&root_path)?;
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

    let (_, exported_paths) = session::claude_code::write_session(&detail, "exported-session")?;
    let exported = fs::read_to_string(&exported_paths[0])?;
    assert!(!exported.contains("Sub-agent session:"));
    assert!(!exported.contains("\"subagent_started\""));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

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

    let summary = session::reader(SourceApp::Codex).parse_summary(&root_path)?;
    assert_eq!(summary.source_session_id, root_id);
    assert!(summary.title.contains("+1 subagents"));

    let detail = session::timeline::get_session_inner(
        SourceApp::Codex,
        root_id,
        Some(root_path.to_string_lossy().as_ref()),
    )?;

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

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn codex_resume_segments_keep_original_segment_as_root() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_id = "88888888-8888-4888-8888-888888888888";
    let sessions_dir = temp_home.join(".codex/sessions/2026/09/02");
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

    let reader = session::reader(SourceApp::Codex);
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    // root 必须是原始段：标题取自原始段首条消息，而不是续跑段的"继续"。
    assert_eq!(entries[0].path, original_path);
    let summary = entries[0].summary.as_ref().expect("summary");
    assert_eq!(summary.title, "Original task");
    // 续跑段与原始段共享 session id，是同一条线程而不是 subagent。
    assert!(!summary.title.contains("subagents"));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn codex_title_strips_markdown_link_syntax() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let transcript_path = temp_home
        .join(".codex/sessions/2026/09/02")
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
                        "text": "[$mongo-slow-log-report](/Users/fang/Desktop/skills/mongo-slow-log-report/SKILL.md) 帮我看下这份skill有没有问题"
                    }]
                }
            }),
        ],
    )?;

    let entries = session::reader(SourceApp::Codex).list_entries()?;
    assert_eq!(entries.len(), 1);
    // 链接语法必须剥掉只留 label，否则 72 字符截断后标题全是 [label](url)。
    assert_eq!(
        entries[0].summary.as_ref().expect("summary").title,
        "$mongo-slow-log-report 帮我看下这份skill有没有问题"
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}
