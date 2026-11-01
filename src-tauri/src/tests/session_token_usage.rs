use super::*;

fn list_first_session(source: SourceApp) -> Result<SessionSummary> {
    let page = session::catalog::list_sessions_inner(
        &state::session_index::SessionIndexState::default(),
        session::catalog::SourceSelection::One(source),
        0,
        20,
        "",
        false,
        true,
    )?;

    assert_eq!(page.total_count, 1, "夹具应恰好产生一个会话条目");
    Ok(page.sessions[0].clone())
}

#[test]
fn claude_listing_usage_sums_assistant_lines_across_family_members() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_id = "11111111-1111-4111-8111-111111111111";
    let project_dir = temp_home.join(".claude/projects/demo-project");

    write_jsonl(
        &project_dir.join(format!("{session_id}.jsonl")),
        &[
            json!({
                "timestamp": "2026-04-21T12:00:00.000Z",
                "sessionId": session_id,
                "type": "user",
                "message": { "content": "Root task" }
            }),
            json!({
                "timestamp": "2026-04-21T12:01:00.000Z",
                "sessionId": session_id,
                "type": "assistant",
                "message": {
                    "content": "answer",
                    "usage": {
                        "input_tokens": 100,
                        "output_tokens": 20,
                        "cache_read_input_tokens": 1000,
                        "cache_creation_input_tokens": 50
                    }
                }
            }),
            // 同文件内嵌 sidechain 回复也按 assistant 行计入。
            json!({
                "timestamp": "2026-04-21T12:02:00.000Z",
                "sessionId": session_id,
                "type": "assistant",
                "isSidechain": true,
                "message": {
                    "content": "sidechain answer",
                    "usage": {
                        "input_tokens": 10,
                        "output_tokens": 5,
                        "cache_read_input_tokens": 200
                    }
                }
            }),
        ],
    )?;
    write_jsonl(
        &project_dir.join("subagents/agent-sub-1.jsonl"),
        &[json!({
            "timestamp": "2026-04-21T12:03:00.000Z",
            "sessionId": session_id,
            "type": "assistant",
            "message": {
                "content": "subagent answer",
                "usage": {
                    "input_tokens": 500,
                    "output_tokens": 50,
                    "cache_read_input_tokens": 5000
                }
            }
        })],
    )?;

    let summary = list_first_session(SourceApp::ClaudeCode)?;
    assert_eq!(
        summary.token_usage,
        Some(session::model::SessionTokenUsage {
            input_tokens: 610,
            output_tokens: 75,
            cache_read_tokens: 6200,
            cache_write_tokens: 50,
        })
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn codex_listing_usage_takes_last_cumulative_and_sums_family_members() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let root_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let subagent_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

    let token_count_event = |input: u64, cached: u64, cache_write: u64, output: u64| {
        json!({
            "timestamp": "2026-04-21T12:05:00.000Z",
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {
                        "input_tokens": input,
                        "cached_input_tokens": cached,
                        "cache_write_input_tokens": cache_write,
                        "output_tokens": output,
                        "reasoning_output_tokens": output / 2,
                        "total_tokens": input + output
                    },
                    "last_token_usage": { "input_tokens": 1, "output_tokens": 1 },
                    "model_context_window": 200000
                }
            }
        })
    };

    write_jsonl(
        &temp_home.join(format!(
            ".codex/sessions/2026/04/21/rollout-2026-04-21T12-00-00-{root_id}.jsonl"
        )),
        &[
            json!({
                "timestamp": "2026-04-21T12:00:00.000Z",
                "type": "session_meta",
                "payload": {
                    "id": root_id,
                    "cwd": "/tmp/codex",
                    "timestamp": "2026-04-21T12:00:00.000Z"
                }
            }),
            json!({
                "timestamp": "2026-04-21T12:01:00.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "Root task" }]
                }
            }),
            // 两条累计值事件:第二条覆盖第一条,不能按增量求和。
            token_count_event(1000, 400, 100, 30),
            token_count_event(2000, 900, 150, 60),
        ],
    )?;
    write_jsonl(
        &temp_home.join(format!(
            ".codex/sessions/2026/04/21/rollout-2026-04-21T12-06-00-{subagent_id}.jsonl"
        )),
        &[
            json!({
                "timestamp": "2026-04-21T12:06:00.000Z",
                "type": "session_meta",
                "payload": {
                    "id": subagent_id,
                    "cwd": "/tmp/codex",
                    "timestamp": "2026-04-21T12:06:00.000Z",
                    "source": {
                        "subagent": {
                            "thread_spawn": { "parent_thread_id": root_id }
                        }
                    }
                }
            }),
            token_count_event(300, 100, 0, 10),
        ],
    )?;

    let summary = list_first_session(SourceApp::Codex)?;
    // root 取最后一条累计值并扣除缓存命中(input 2000-900),再与子代理线程相加。
    assert_eq!(
        summary.token_usage,
        Some(session::model::SessionTokenUsage {
            input_tokens: 1100 + 200,
            output_tokens: 60 + 10,
            cache_read_tokens: 900 + 100,
            cache_write_tokens: 150,
        })
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn pi_listing_usage_sums_assistant_and_subagent_run_usage() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_path = temp_home
        .join(".pi/agent/sessions/--tmp-pi-usage--")
        .join("2026-09-04T09-00-00-000Z_pi-usage.jsonl");

    write_jsonl(
        &session_path,
        &[
            json!({
                "type": "session",
                "version": 3,
                "id": "pi-usage",
                "timestamp": "2026-09-04T09:00:00.000Z",
                "cwd": "/tmp/pi-usage",
            }),
            json!({
                "type": "message",
                "id": "m1",
                "parentId": null,
                "timestamp": "2026-09-04T09:00:01.000Z",
                "message": { "role": "user", "content": "pi task" }
            }),
            json!({
                "type": "message",
                "id": "m2",
                "parentId": "m1",
                "timestamp": "2026-09-04T09:00:02.000Z",
                "message": {
                    "role": "assistant",
                    "content": "answer",
                    "usage": {
                        "input": 197,
                        "output": 180,
                        "cacheRead": 6656,
                        "cacheWrite": 0,
                        "reasoning": 37,
                        "totalTokens": 7033
                    }
                }
            }),
            json!({
                "type": "message",
                "id": "m3",
                "parentId": "m2",
                "timestamp": "2026-09-04T09:00:03.000Z",
                "message": {
                    "role": "toolResult",
                    "toolName": "subagent",
                    "toolCallId": "call-1",
                    "content": "subagent report",
                    "details": {
                        "results": [{
                            "title": "run",
                            "usage": {
                                "input": 636783,
                                "output": 31284,
                                "cacheRead": 2916352,
                                "cacheWrite": 0
                            }
                        }]
                    }
                }
            }),
        ],
    )?;

    let summary = list_first_session(SourceApp::Pi)?;
    assert_eq!(
        summary.token_usage,
        Some(session::model::SessionTokenUsage {
            input_tokens: 197 + 636783,
            output_tokens: 180 + 31284,
            cache_read_tokens: 6656 + 2916352,
            cache_write_tokens: 0,
        })
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn opencode_listing_usage_sums_assistant_tokens_with_reasoning() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    seed_opencode_family("ses_usage_root", "ses_usage_child")?;

    let connection = Connection::open(session::opencode::db_path()?)?;
    seed_opencode_message_row(
        &connection,
        "ses_usage_root",
        "root-assistant",
        1_744_366_402_000,
        2,
        "assistant",
        &json!({
            "time": { "created": 1_744_366_402_000_i64 },
            "agent": "build",
            "content": [{ "type": "text", "text": "Root answer" }],
            "tokens": {
                "input": 1000,
                "output": 100,
                "reasoning": 50,
                "cache": { "read": 500, "write": 20 }
            }
        }),
    )?;
    seed_opencode_message_row(
        &connection,
        "ses_usage_child",
        "child-assistant",
        1_744_366_403_000,
        2,
        "assistant",
        &json!({
            "time": { "created": 1_744_366_403_000_i64 },
            "agent": "build",
            "content": [{ "type": "text", "text": "Child answer" }],
            "tokens": {
                "input": 200,
                "output": 20,
                "cache": { "read": 0, "write": 0 }
            }
        }),
    )?;
    drop(connection);

    let summary = list_first_session(SourceApp::OpenCode)?;
    // reasoning 并入输出;child-msg-1 没有 tokens 字段,SUM 视为 0。
    assert_eq!(
        summary.token_usage,
        Some(session::model::SessionTokenUsage {
            input_tokens: 1200,
            output_tokens: (100 + 50) + 20,
            cache_read_tokens: 500,
            cache_write_tokens: 20,
        })
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

fn grokbuild_session_fixture(
    home: &Path,
    session_id: &str,
    attempt: Option<&str>,
    kind: Option<&str>,
    usage: Option<&Value>,
) -> Result<()> {
    let dir = home.join(".grok/sessions/not-a-cwd").join(session_id);
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("summary.json"),
        serde_json::to_vec(&json!({
            "info": {"id": session_id, "cwd": "/synthetic/project"},
            "chat_format_version": 1,
            "session_summary": "Synthetic summary",
            "attempt_id": attempt,
            "session_kind": kind,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:01:00Z"
        }))?,
    )?;
    write_jsonl(
        &dir.join("chat_history.jsonl"),
        &[
            json!({"type":"user","content":[{"type":"text","text":"Grok task"}]}),
            json!({"type":"assistant","content":"answer"}),
        ],
    )?;
    if let Some(usage) = usage {
        fs::write(dir.join("usage.json"), serde_json::to_vec(usage)?)?;
    }
    Ok(())
}

#[test]
fn grokbuild_listing_usage_reads_session_totals_and_sums_subagents() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let root_id = "grok-usage-root";
    let child_id = "grok-usage-child";
    grokbuild_session_fixture(
        &temp_home,
        root_id,
        None,
        None,
        Some(&json!({
            "sessionId": root_id,
            "session": {
                "inputTokens": 2000,
                "outputTokens": 60,
                "cachedReadTokens": 900,
                "cacheCreationTokens": 150,
                "reasoningTokens": 25,
                "totalTokens": 2060,
                "modelCalls": 3,
                "turnCount": 1
            }
        })),
    )?;
    grokbuild_session_fixture(
        &temp_home,
        child_id,
        Some("attempt-1"),
        Some("subagent"),
        // 子会话不带 usage.json:旧版本/中断场景下保持 None 而不是报错。
        None,
    )?;
    let meta_dir = temp_home
        .join(".grok/sessions/not-a-cwd")
        .join(root_id)
        .join("subagents")
        .join(child_id);
    fs::create_dir_all(&meta_dir)?;
    fs::write(
        meta_dir.join("meta.json"),
        serde_json::to_vec(&json!({
            "subagent_id": child_id,
            "attempt_id": "attempt-1",
            "parent_session_id": root_id,
            "child_session_id": child_id,
            "subagent_type": "general-purpose",
            "description": "usage child",
            "status": "completed",
            "started_at": "2026-01-01T00:00:10Z",
            "completed_at": "2026-01-01T00:00:20Z"
        }))?,
    )?;

    let summary = list_first_session(SourceApp::GrokBuild)?;
    // inputTokens 含缓存命中(input+output==totalTokens),扣除后口径对齐;
    // 子会话无 usage.json,按 0 计入。
    assert_eq!(
        summary.token_usage,
        Some(session::model::SessionTokenUsage {
            input_tokens: 2000 - 900,
            output_tokens: 60,
            cache_read_tokens: 900,
            cache_write_tokens: 150,
        })
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}
