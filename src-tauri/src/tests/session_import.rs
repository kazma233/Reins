use super::*;

#[test]
fn cross_import_roundtrip_works_in_temp_home() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let codex_source_id = "11111111-1111-4111-8111-111111111111";
    let claude_source_id = "22222222-2222-4222-8222-222222222222";

    let _ = session::codex::write_session(&sample_detail(SourceApp::Codex), codex_source_id)?;
    let codex_to_claude = session::import::import_session_inner(
        SourceApp::Codex,
        codex_source_id,
        SourceApp::ClaudeCode,
        None,
    )?;
    let imported_claude = session::timeline::get_session_inner(
        SourceApp::ClaudeCode,
        &codex_to_claude.created_session_id,
        None,
    )?;

    assert!(!imported_claude.messages.is_empty());
    assert_eq!(imported_claude.summary.source_app, SourceApp::ClaudeCode);
    assert_eq!(
        codex_to_claude.resume_cwd.as_deref(),
        Some("/tmp/reins/workspace")
    );

    let _ = session::claude_code::write_session(
        &sample_detail(SourceApp::ClaudeCode),
        claude_source_id,
    )?;
    let claude_to_codex = session::import::import_session_inner(
        SourceApp::ClaudeCode,
        claude_source_id,
        SourceApp::Codex,
        None,
    )?;
    let imported_codex = session::timeline::get_session_inner(
        SourceApp::Codex,
        &claude_to_codex.created_session_id,
        None,
    )?;

    seed_opencode_session(&sample_detail(SourceApp::OpenCode), codex_source_id)?;

    let opencode_transcript_path = temp_home
        .join(".local/share/opencode/session")
        .join(format!("{codex_source_id}.opencode"));

    assert!(!imported_codex.messages.is_empty());
    assert_eq!(imported_codex.summary.source_app, SourceApp::Codex);
    assert_eq!(
        claude_to_codex.resume_cwd.as_deref(),
        Some("/tmp/reins/workspace")
    );
    assert!(codex_timestamps_are_monotonic(
        &claude_to_codex.created_session_id
    )?);
    assert!(codex_has_event_type(
        &claude_to_codex.created_session_id,
        "user_message"
    )?);
    assert!(codex_has_event_type(
        &claude_to_codex.created_session_id,
        "agent_message"
    )?);
    assert!(codex_has_event_type(
        &claude_to_codex.created_session_id,
        "task_complete"
    )?);

    let opencode_to_codex = session::import::import_session_inner(
        SourceApp::OpenCode,
        codex_source_id,
        SourceApp::Codex,
        Some(opencode_transcript_path.to_string_lossy().as_ref()),
    )?;
    let imported_from_opencode_codex = session::timeline::get_session_inner(
        SourceApp::Codex,
        &opencode_to_codex.created_session_id,
        None,
    )?;

    assert!(!imported_from_opencode_codex.messages.is_empty());
    assert_eq!(
        imported_from_opencode_codex.summary.source_app,
        SourceApp::Codex
    );
    assert_eq!(
        opencode_to_codex.resume_cwd.as_deref(),
        Some("/tmp/reins/workspace")
    );
    assert!(codex_timestamps_are_monotonic(
        &opencode_to_codex.created_session_id
    )?);
    assert!(codex_has_event_type(
        &opencode_to_codex.created_session_id,
        "user_message"
    )?);
    assert!(codex_has_event_type(
        &opencode_to_codex.created_session_id,
        "agent_message"
    )?);
    assert!(codex_has_event_type(
        &opencode_to_codex.created_session_id,
        "task_complete"
    )?);
    assert!(has_codex_function_call_pair(
        &opencode_to_codex.created_session_id,
        "call_import_1"
    )?);

    let opencode_to_claude = session::import::import_session_inner(
        SourceApp::OpenCode,
        codex_source_id,
        SourceApp::ClaudeCode,
        Some(opencode_transcript_path.to_string_lossy().as_ref()),
    )?;
    let imported_from_opencode_claude = session::timeline::get_session_inner(
        SourceApp::ClaudeCode,
        &opencode_to_claude.created_session_id,
        None,
    )?;

    assert!(!imported_from_opencode_claude.messages.is_empty());
    assert_eq!(
        imported_from_opencode_claude.summary.source_app,
        SourceApp::ClaudeCode
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn pi_import_roundtrip_writes_valid_v3_chain_and_reads_back() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let source_id = "33333333-3333-4333-8333-333333333333";
    let source_path = temp_home
        .join(".claude/projects/demo")
        .join(format!("{source_id}.jsonl"));
    write_jsonl(
        &source_path,
        &[
            json!({
                "timestamp": "2026-09-04T10:00:00.000Z", "sessionId": source_id,
                "type": "user", "cwd": "/tmp/pi-import", "message": { "content": "Import to Pi" }
            }),
            json!({
                "timestamp": "2026-09-04T10:00:01.000Z", "sessionId": source_id,
                "type": "assistant", "message": { "content": "Pi answer" }
            }),
            json!({
                "timestamp": "2026-09-04T10:00:02.000Z", "sessionId": source_id,
                "type": "user", "message": { "content": [{
                    "type": "tool_result", "tool_use_id": "call-pi", "content": "Pi tool output"
                }] }
            }),
        ],
    )?;

    let result = session::import::import_session_inner(
        SourceApp::ClaudeCode,
        source_id,
        SourceApp::Pi,
        Some(source_path.to_string_lossy().as_ref()),
    )?;
    assert_eq!(result.target_app, SourceApp::Pi);
    assert_eq!(result.created_paths.len(), 1);
    let imported_path = PathBuf::from(&result.created_paths[0]);
    let lines = fs::read_to_string(&imported_path)?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert_eq!(lines[0]["type"], "session");
    assert_eq!(lines[0]["version"], 3);
    assert_eq!(lines[0]["id"], result.created_session_id);
    for line in &lines[1..] {
        assert!(line["id"].is_string());
        assert!(line.get("parentId").is_some());
    }
    let detail =
        session::timeline::get_session_inner(SourceApp::Pi, &result.created_session_id, None)?;
    assert_eq!(detail.summary.source_app, SourceApp::Pi);
    assert!(!detail.messages.is_empty());
    assert!(
        detail
            .messages
            .iter()
            .any(|message| message.role == "toolResult")
    );
    Ok(())
}

#[test]
fn pi_import_writes_tool_calls_and_results_as_valid_agent_messages() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let session_id = "pi-export-tool-shape";

    let (_, paths) = session::pi::write_session(&sample_detail(SourceApp::Codex), session_id)?;
    let lines = fs::read_to_string(&paths[0])?
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let assistant = lines
        .iter()
        .find(|line| {
            line["type"] == "message"
                && line["message"]["role"] == "assistant"
                && line["message"]["content"]
                    .as_array()
                    .is_some_and(|content| content.iter().any(|block| block["type"] == "toolCall"))
        })
        .expect("assistant tool call entry");
    assert_eq!(assistant["message"]["api"], "imported");
    assert_eq!(assistant["message"]["provider"], "imported");
    assert_eq!(assistant["message"]["model"], "imported");
    assert!(assistant["message"]["usage"].is_object());
    assert_eq!(assistant["message"]["stopReason"], "toolUse");

    let tool_result = lines
        .iter()
        .find(|line| line["type"] == "message" && line["message"]["role"] == "toolResult")
        .expect("tool result entry");
    assert_eq!(tool_result["message"]["toolCallId"], "call_import_1");
    assert_eq!(tool_result["message"]["content"][0]["type"], "text");

    let detail = session::timeline::get_session_inner(SourceApp::Pi, session_id, None)?;
    assert!(detail.messages.iter().any(|message| {
        message.role == "assistant" && message.blocks.iter().any(|block| block.kind == "tool_use")
    }));
    assert!(detail.messages.iter().any(|message| {
        message.role == "toolResult"
            && message
                .blocks
                .iter()
                .any(|block| block.kind == "tool_result")
    }));
    Ok(())
}

#[test]
fn pi_source_import_preserves_tool_results_in_codex_and_claude() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let source_id = "pi-source-tool-result";
    let source_path = temp_home
        .join(".pi/agent/sessions/--tmp-pi-import--")
        .join("2026-09-04T10-00-00-000Z_pi-source-tool-result.jsonl");
    write_jsonl(
        &source_path,
        &[
            json!({
                "type": "session",
                "version": 3,
                "id": source_id,
                "timestamp": "2026-09-04T10:00:00.000Z",
                "cwd": "/tmp/pi-import"
            }),
            json!({
                "type": "message",
                "id": "user-1",
                "parentId": null,
                "timestamp": "2026-09-04T10:00:01.000Z",
                "message": {
                    "role": "user",
                    "content": "Run pwd"
                }
            }),
            json!({
                "type": "message",
                "id": "assistant-1",
                "parentId": "user-1",
                "timestamp": "2026-09-04T10:00:02.000Z",
                "message": {
                    "role": "assistant",
                    "content": [{
                        "type": "toolCall",
                        "id": "call-pi-source",
                        "name": "bash",
                        "arguments": { "command": "pwd" }
                    }]
                }
            }),
            json!({
                "type": "message",
                "id": "tool-1",
                "parentId": "assistant-1",
                "timestamp": "2026-09-04T10:00:03.000Z",
                "message": {
                    "role": "toolResult",
                    "toolCallId": "call-pi-source",
                    "toolName": "bash",
                    "content": [{ "type": "text", "text": "/tmp/pi-import" }],
                    "isError": false
                }
            }),
            json!({
                "type": "message",
                "id": "assistant-2",
                "parentId": "tool-1",
                "timestamp": "2026-09-04T10:00:04.000Z",
                "message": {
                    "role": "assistant",
                    "content": [{ "type": "text", "text": "Done" }]
                }
            }),
        ],
    )?;

    let codex_result = session::import::import_session_inner(
        SourceApp::Pi,
        source_id,
        SourceApp::Codex,
        Some(source_path.to_string_lossy().as_ref()),
    )?;
    assert!(codex_has_event_type(
        &codex_result.created_session_id,
        "user_message"
    )?);
    assert!(has_codex_function_call_pair(
        &codex_result.created_session_id,
        "call-pi-source"
    )?);

    let claude_result = session::import::import_session_inner(
        SourceApp::Pi,
        source_id,
        SourceApp::ClaudeCode,
        Some(source_path.to_string_lossy().as_ref()),
    )?;
    let imported_claude = session::timeline::get_session_inner(
        SourceApp::ClaudeCode,
        &claude_result.created_session_id,
        None,
    )?;
    assert!(imported_claude.messages.iter().any(|message| {
        message.blocks.iter().any(|block| {
            block.kind == "tool_result" && block.tool_call_id.as_deref() == Some("call-pi-source")
        })
    }));

    Ok(())
}

#[test]
#[ignore = "Requires CODEX_TRANSCRIPT_PATH and the OpenCode CLI"]
fn imports_codex_transcript_into_opencode_cli_in_temp_home() -> Result<()> {
    let transcript_path = env::var("CODEX_TRANSCRIPT_PATH")
        .context("CODEX_TRANSCRIPT_PATH must point to a Codex transcript JSONL file")?;
    let transcript_path = PathBuf::from(transcript_path);
    let source_session_id =
        session::catalog::derive_session_id(&transcript_path).ok_or_else(|| {
            anyhow!(
                "Unable to derive session id from {}",
                transcript_path.display()
            )
        })?;
    let temp_home = env::temp_dir().join(format!("reins-opencode-import-test-{}", Uuid::new_v4()));

    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let import_result = session::import::import_session_inner(
        SourceApp::Codex,
        &source_session_id,
        SourceApp::OpenCode,
        Some(transcript_path.to_string_lossy().as_ref()),
    )?;
    let imported_detail = session::timeline::get_session_inner(
        SourceApp::OpenCode,
        &import_result.created_session_id,
        None,
    )?;

    assert_eq!(imported_detail.summary.source_app, SourceApp::OpenCode);
    assert!(!imported_detail.messages.is_empty());

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
#[ignore = "Requires OPENCODE_SESSION_ID and a working Codex CLI login"]
fn imports_real_opencode_session_into_codex_and_resumes() -> Result<()> {
    let source_session_id = env::var("OPENCODE_SESSION_ID")
        .context("OPENCODE_SESSION_ID must be the OpenCode session ID (e.g. ses_xxx)")?;

    let session_file = format!("/tmp/{source_session_id}.opencode");

    let source_detail = session::timeline::get_session_inner(
        SourceApp::OpenCode,
        &source_session_id,
        Some(&session_file),
    )?;
    assert!(!source_detail.messages.is_empty());

    let import_result = session::import::import_session_inner(
        SourceApp::OpenCode,
        &source_session_id,
        SourceApp::Codex,
        Some(&session_file),
    )?;
    let imported_detail = session::timeline::get_session_inner(
        SourceApp::Codex,
        &import_result.created_session_id,
        None,
    )?;
    assert!(!imported_detail.messages.is_empty());

    let cwd = imported_detail
        .summary
        .cwd
        .clone()
        .unwrap_or(env::current_dir()?.display().to_string());
    let last_message_path =
        env::temp_dir().join(format!("reins-codex-resume-{}.txt", Uuid::new_v4()));
    let output = Command::new("codex")
        .arg("exec")
        .arg("resume")
        .arg("--skip-git-repo-check")
        .arg("--dangerously-bypass-approvals-and-sandbox")
        .arg("-o")
        .arg(&last_message_path)
        .arg(&import_result.created_session_id)
        .arg("Reply with exactly VERIFIED")
        .current_dir(&cwd)
        .output()
        .context("Failed to execute codex exec resume")?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "codex exec resume failed\nstdout:\n{}\nstderr:\n{}",
            stdout.trim(),
            stderr.trim()
        );
    }

    let last_message = fs::read_to_string(&last_message_path)
        .with_context(|| format!("Failed to read {}", last_message_path.display()))?;
    fs::remove_file(&last_message_path).ok();
    assert!(last_message.contains("VERIFIED"));
    assert!(codex_thread_exists(&import_result.created_session_id)?);

    Ok(())
}
