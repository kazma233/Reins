use super::*;

#[test]
#[ignore = "Explicit read-only validation of the installed Grok session store"]
fn grokbuild_local_store_readonly_acceptance() -> Result<()> {
    let _guard = TestEnvGuard::lock();
    let reader = session::reader(SourceApp::GrokBuild);
    let entries = reader.list_entries()?;
    let mut messages = 0;
    let mut events = 0;
    let mut agents = 0;
    let mut subagent_messages = 0;
    for entry in &entries {
        let overview = reader.parse_overview(&entry.path)?;
        assert_eq!(overview.summary.source_app, SourceApp::GrokBuild);
        messages += reader.parse_messages_page(&entry.path, 0, 1)?.total_count;
        events += reader.parse_events_page(&entry.path, 0, 1)?.total_count;
        agents += overview.agents.len();
        for agent in overview.agents.iter().filter(|agent| !agent.is_root) {
            subagent_messages += reader
                .parse_agent_messages(&entry.path, &agent.session_id)?
                .len();
        }
    }
    println!(
        "Grok read-only acceptance: sessions={}, messages={messages}, events={events}, agents={agents}, subagent_messages={subagent_messages}",
        entries.len()
    );
    Ok(())
}

fn bounded_home() -> PathBuf {
    env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()))
}

fn child_session_fixture(home: &Path, child: &str, attempt: &str, answer: &str) -> Result<PathBuf> {
    let dir = home.join(".grok/sessions/not-a-cwd").join(child);
    fs::create_dir_all(&dir)?;
    let path = dir.join("summary.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "info": {"id": child, "cwd": "/synthetic/project"},
            "chat_format_version": 1,
            "session_summary": "Synthetic child summary",
            "attempt_id": attempt,
            "session_kind": "subagent",
            "created_at": "2026-01-01T00:00:10Z",
            "updated_at": "2026-01-01T00:00:20Z"
        }))?,
    )?;
    write_jsonl(
        &dir.join("chat_history.jsonl"),
        &[
            json!({"type":"user","content":[{"type":"text","text":"Synthetic child request"}]}),
            json!({"type":"assistant","content":answer}),
        ],
    )?;
    Ok(path)
}

fn subagent_meta_fixture(
    home: &Path,
    parent: &str,
    child: &str,
    attempt: &str,
    description: &str,
) -> Result<()> {
    let dir = home
        .join(".grok/sessions/not-a-cwd")
        .join(parent)
        .join("subagents")
        .join(child);
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("meta.json"),
        serde_json::to_vec(&json!({
            "subagent_id": child,
            "attempt_id": attempt,
            "parent_session_id": parent,
            "child_session_id": child,
            "subagent_type": "general-purpose",
            "description": description,
            "status": "completed",
            "started_at": "2026-01-01T00:00:10Z",
            "completed_at": "2026-01-01T00:00:20Z"
        }))?,
    )?;
    Ok(())
}

fn subagent_fixture(
    home: &Path,
    parent: &str,
    child: &str,
    attempt: &str,
    answer: &str,
) -> Result<()> {
    child_session_fixture(home, child, attempt, answer)?;
    subagent_meta_fixture(home, parent, child, attempt, &format!("task {child}"))
}

fn marker_session_id(message: &SessionMessage) -> Option<String> {
    message.blocks.iter().find_map(|block| {
        let payload = block.payload.as_ref()?;
        if payload.get("type")?.as_str()? != "subagent_started" {
            return None;
        }
        payload.get("session_id")?.as_str().map(str::to_string)
    })
}

#[test]
fn grokbuild_folds_confirmed_subagents_into_parent_family() -> Result<()> {
    let home = bounded_home();
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(&home);
    let parent_path = fixture(&home, "parent")?;
    history(&parent_path)?;
    subagent_fixture(&home, "parent", "child-a", "at1.a", "Child A answer")?;
    subagent_fixture(&home, "parent", "child-b", "at1.b", "Child B answer")?;

    let reader = session::reader(SourceApp::GrokBuild);
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    let summary = entries[0].summary.as_ref().unwrap();
    assert_eq!(summary.source_session_id, "parent");
    assert!(
        summary.title.ends_with("(+2 subagents)"),
        "{}",
        summary.title
    );
    assert_eq!(entries[0].path, parent_path.canonicalize()?);

    let family_path = &entries[0].path;
    let overview = reader.parse_overview(family_path)?;
    assert_eq!(overview.agents.len(), 3);
    assert!(overview.agents[0].is_root);
    assert_eq!(overview.agents[0].label, "主 Agent");
    let labels = overview
        .agents
        .iter()
        .filter(|agent| !agent.is_root)
        .map(|agent| agent.label.clone())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"task child-a(子)".to_string()));
    assert!(labels.contains(&"task child-b(子)".to_string()));

    // 主时间线只有父会话消息 + 子代理入口，不扫描子会话正文。
    let page = reader.parse_messages_page(family_path, 0, 200)?;
    assert_eq!(page.total_count, 6 + 2);
    assert_eq!(
        page.messages
            .iter()
            .filter_map(marker_session_id)
            .collect::<Vec<_>>(),
        vec!["child-a".to_string(), "child-b".to_string()]
    );
    let first = reader.parse_messages_page(family_path, 0, 4)?;
    assert!(
        first
            .messages
            .iter()
            .all(|message| marker_session_id(message).is_none())
    );
    let last = reader.parse_messages_page(family_path, first.next_offset.unwrap(), 8)?;
    assert_eq!(
        last.messages
            .iter()
            .filter(|message| marker_session_id(message).is_some())
            .count(),
        2
    );

    // 子代理弹窗数据：marker + 子会话正文，消息按子会话命名空间隔离。
    let child = reader.parse_agent_messages(family_path, "child-a")?;
    assert_eq!(child.len(), 3);
    assert_eq!(marker_session_id(&child[0]).as_deref(), Some("child-a"));
    assert!(
        child
            .iter()
            .all(|message| message.session_id.as_deref() == Some("child-a"))
    );
    assert_eq!(child[2].blocks[0].text.as_deref(), Some("Child A answer"));
    assert!(
        reader
            .parse_agent_messages(family_path, "not-a-child")
            .is_err()
    );

    // 折叠后的子会话仍可按 id 解析到路径，并归到同一个 family。
    assert_eq!(
        reader.resolve_path("child-a")?,
        home.join(".grok/sessions/not-a-cwd/child-a/summary.json")
            .canonicalize()?
    );
    assert_eq!(
        reader
            .parse_summary(&reader.resolve_path("child-a")?)?
            .source_session_id,
        "parent"
    );

    // 新的 meta 与子会话在刷新后立即可见。
    subagent_fixture(&home, "parent", "child-c", "at1.c", "Child C answer")?;
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0]
            .summary
            .as_ref()
            .unwrap()
            .title
            .ends_with("(+3 subagents)")
    );
    Ok(())
}

#[test]
fn grokbuild_missing_child_keeps_parent_readable() -> Result<()> {
    let home = bounded_home();
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(&home);
    let parent_path = fixture(&home, "parent")?;
    history(&parent_path)?;
    subagent_meta_fixture(&home, "parent", "missing", "at1.m", "task missing")?;

    let reader = session::reader(SourceApp::GrokBuild);
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0]
            .summary
            .as_ref()
            .unwrap()
            .title
            .ends_with("(+1 subagents)")
    );
    let overview = reader.parse_overview(&entries[0].path)?;
    assert_eq!(overview.agents.len(), 2);
    assert_eq!(overview.agents[1].session_id, "missing");

    // 父会话照常可读，缺失子会话保留入口但打开时明确报错。
    let page = reader.parse_messages_page(&entries[0].path, 0, 200)?;
    assert!(
        page.messages
            .iter()
            .any(|message| marker_session_id(message).as_deref() == Some("missing"))
    );
    let error = reader
        .parse_agent_messages(&entries[0].path, "missing")
        .unwrap_err()
        .to_string();
    assert!(error.contains("not readable"), "{error}");
    // 导出路径必须报错，而不是输出缺正文的假会话。
    assert!(reader.parse_detail(&entries[0].path).is_err());
    Ok(())
}

#[test]
fn grokbuild_orphan_and_nested_children_stay_standalone() -> Result<()> {
    let home = bounded_home();
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(&home);
    fixture(&home, "root")?;
    child_session_fixture(&home, "nested-child", "at1.n", "Nested child answer")?;
    subagent_meta_fixture(&home, "root", "nested-child", "at1.n", "task nested-child")?;
    // 嵌套子代理（父自身也是子代理）尚未验证：保留为独立条目。
    child_session_fixture(&home, "grand-child", "at1.g", "Grand child answer")?;
    subagent_meta_fixture(
        &home,
        "nested-child",
        "grand-child",
        "at1.g",
        "task grand-child",
    )?;
    // 没有任何 meta 的普通会话不应被折叠。
    child_session_fixture(&home, "orphan", "at1.o", "Orphan answer")?;

    let reader = session::reader(SourceApp::GrokBuild);
    let entries = reader.list_entries()?;
    assert_eq!(entries.len(), 3);
    let root = entries
        .iter()
        .find(|entry| entry.summary.as_ref().unwrap().source_session_id == "root")
        .unwrap();
    assert!(
        root.summary
            .as_ref()
            .unwrap()
            .title
            .ends_with("(+1 subagents)")
    );
    for id in ["grand-child", "orphan"] {
        let entry = entries
            .iter()
            .find(|entry| entry.summary.as_ref().unwrap().source_session_id == id)
            .unwrap_or_else(|| panic!("{id} should stay standalone"));
        assert!(!entry.summary.as_ref().unwrap().title.contains("subagents"));
    }

    let overview = reader.parse_overview(&root.path)?;
    assert_eq!(overview.agents.len(), 2);
    assert!(
        reader
            .parse_agent_messages(&root.path, "nested-child")
            .is_ok()
    );
    // 未折叠的嵌套/孤立子会话不属于父会话的成员。
    assert!(
        reader
            .parse_agent_messages(&root.path, "grand-child")
            .is_err()
    );
    assert!(reader.parse_agent_messages(&root.path, "orphan").is_err());
    Ok(())
}

#[test]
fn grokbuild_rejects_duplicate_child_ownership() -> Result<()> {
    // 同一个子会话被两个父会话声明。
    let home = bounded_home();
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(&home);
    fixture(&home, "parent-a")?;
    fixture(&home, "parent-b")?;
    child_session_fixture(&home, "shared", "at1.s", "Shared answer")?;
    subagent_meta_fixture(&home, "parent-a", "shared", "at1.s", "task a")?;
    subagent_meta_fixture(&home, "parent-b", "shared", "at1.s", "task b")?;
    let error = session::reader(SourceApp::GrokBuild)
        .list_entries()
        .unwrap_err()
        .to_string();
    assert!(error.contains("Conflicting"), "{error}");
    Ok(())
}

#[test]
fn grokbuild_rejects_attempt_mismatch() -> Result<()> {
    // 子会话的 attempt 与 meta 不一致。
    let home = bounded_home();
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(&home);
    fixture(&home, "parent")?;
    child_session_fixture(&home, "child", "at1.real", "Child answer")?;
    subagent_meta_fixture(&home, "parent", "child", "at1.other", "task child")?;
    let error = session::reader(SourceApp::GrokBuild)
        .list_entries()
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not match"), "{error}");
    Ok(())
}

#[test]
fn grokbuild_rejects_self_claimed_subagent() -> Result<()> {
    // 自己声明为自己的子代理。
    let home = bounded_home();
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(&home);
    fixture(&home, "self")?;
    subagent_meta_fixture(&home, "self", "self", "at1.self", "task self")?;
    assert!(
        session::reader(SourceApp::GrokBuild)
            .list_entries()
            .is_err()
    );
    Ok(())
}

fn fixture(home: &Path, id: &str) -> Result<PathBuf> {
    let path = home
        .join(".grok/sessions/not-a-cwd")
        .join(id)
        .join("summary.json");
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "info": {"id": id, "cwd": "/synthetic/project's folder"},
            "chat_format_version": 1, "session_summary": "Synthetic summary",
            "generated_title": "Synthetic title", "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-02T00:00:00Z", "num_messages": 0
        }))?,
    )?;
    Ok(path)
}

fn history(path: &Path) -> Result<()> {
    write_jsonl(
        &path.with_file_name("chat_history.jsonl"),
        &[
            json!({"type":"system", "content":"Synthetic system"}),
            json!({"type":"user", "synthetic_reason":"system_reminder", "content":[{"type":"text","text":"Do not display"}]}),
            json!({"type":"user", "content":[{"type":"text","text":"Synthetic request"},{"type":"image","url":"https://example.invalid/image.png"}]}),
            json!({"type":"reasoning", "summary":[{"type":"summary_text","text":"Synthetic thought"}], "encrypted_content":"NEVER_EXPOSE_SYNTHETIC"}),
            json!({"type":"assistant", "content":"Synthetic answer", "tool_calls":[{"id":"ok","name":"read","arguments":"{\"path\":\"fixture.txt\"}"},{"id":"bad","name":"run","arguments":"{}"}]}),
            json!({"type":"tool_result", "tool_call_id":"ok", "content":"Synthetic success"}),
            json!({"type":"tool_result", "tool_call_id":"bad", "content":"Synthetic failure"}),
        ],
    )?;
    write_jsonl(
        &path.with_file_name("updates.jsonl"),
        &[
            json!({"params":{"update":{"sessionUpdate":"agent_message_chunk","content":{"text":"Synthetic answer"}}}}),
            json!({"params":{"update":{"sessionUpdate":"tool_call_update","toolCallId":"bad","status":"failed"}}}),
        ],
    )?;
    write_jsonl(
        &path.with_file_name("events.jsonl"),
        &[
            json!({"type":"turn_started","ts":"2026-01-01T00:00:00Z"}),
            json!({"type":"tool_completed","tool_call_id":"ok","outcome":"success"}),
            json!({"type":"turn_ended","outcome":"completed"}),
        ],
    )
}

#[test]
fn grokbuild_summary_only_and_format_validation() -> Result<()> {
    let home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(home.as_path());
    let path = fixture(home.as_path(), "empty")?;
    let reader = session::reader(SourceApp::GrokBuild);
    let detail = reader.parse_detail(&path)?;
    assert!(detail.messages.is_empty() && detail.events.is_empty());
    assert_eq!(detail.summary.title, "Synthetic title");
    assert_eq!(
        detail.summary.cwd.as_deref(),
        Some("/synthetic/project's folder")
    );
    assert_eq!(reader.resolve_path("empty")?, path.canonicalize()?);
    assert!(reader.resolve_path("../../etc/passwd").is_err());
    let mut value: Value = serde_json::from_slice(&fs::read(&path)?)?;
    value["generated_title"] = json!("");
    fs::write(&path, serde_json::to_vec(&value)?)?;
    assert_eq!(reader.parse_summary(&path)?.title, "Synthetic summary");
    value["session_summary"] = json!("");
    fs::write(&path, serde_json::to_vec(&value)?)?;
    assert_eq!(reader.parse_summary(&path)?.title, "empty");
    value["chat_format_version"] = json!(2);
    fs::write(&path, serde_json::to_vec(&value)?)?;
    assert!(
        reader
            .parse_detail(&path)
            .unwrap_err()
            .to_string()
            .contains("chat_format_version")
    );
    Ok(())
}

#[test]
fn grokbuild_normalizes_messages_without_stream_duplicates_or_encrypted_content() -> Result<()> {
    let home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(home.as_path());
    let path = fixture(home.as_path(), "messages")?;
    history(&path)?;
    let reader = session::reader(SourceApp::GrokBuild);
    let detail = reader.parse_detail(&path)?;
    assert_eq!(detail.messages.len(), 6);
    assert_eq!(detail.messages[1].blocks[1].kind, "image");
    assert_eq!(
        detail.messages[2].blocks[0].text.as_deref(),
        Some("Synthetic thought")
    );
    assert_eq!(
        detail.messages[3].blocks[1].payload.as_ref().unwrap()["input"]["path"],
        "fixture.txt"
    );
    assert_eq!(detail.messages[4].blocks[0].is_error, Some(false));
    assert_eq!(detail.messages[5].blocks[0].is_error, Some(true));
    let serialized = serde_json::to_string(&detail)?;
    assert!(
        !serialized.contains("NEVER_EXPOSE")
            && !serialized.contains("encrypted_content")
            && !serialized.contains("Do not display")
    );
    assert_eq!(serialized.matches("Synthetic answer").count(), 1);
    let first = reader.parse_messages_page(&path, 0, 2)?;
    let second = reader.parse_messages_page(&path, first.next_offset.unwrap(), 4)?;
    assert_eq!(first.total_count, 6);
    assert!(!second.has_more);
    assert_ne!(first.messages[1].id, second.messages[0].id);
    let first = reader.parse_events_page(&path, 0, 2)?;
    let second = reader.parse_events_page(&path, first.next_offset.unwrap(), 2)?;
    assert_eq!(first.total_count, 3);
    assert_eq!(second.events.len(), 1);
    assert!(!second.has_more);
    assert_eq!(reader.parse_events_page(&path, 99, 2)?.offset, 3);
    write_jsonl(
        &path.with_file_name("updates.jsonl"),
        &[
            json!({"params":{"update":{"sessionUpdate":"tool_call_update","toolCallId":"bad","status":"completed"}}}),
        ],
    )?;
    assert_eq!(
        reader.parse_detail(&path)?.messages[5].blocks[0].is_error,
        Some(false)
    );
    write_jsonl(
        &path.with_file_name("chat_history.jsonl"),
        &[json!({"type":"future_record","content":"not a message"})],
    )?;
    assert!(reader.parse_messages_page(&path, 0, 2).is_err());
    Ok(())
}

#[test]
fn grokbuild_truncated_tool_arguments_stay_readable() -> Result<()> {
    // 模型输出中断会落盘截断的 arguments，会话必须照常可读并保留原文。
    let home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(home.as_path());
    let path = fixture(home.as_path(), "truncated")?;
    write_jsonl(
        &path.with_file_name("chat_history.jsonl"),
        &[
            json!({"type":"user","content":[{"type":"text","text":"Synthetic request"}]}),
            json!({"type":"assistant","content":"","tool_calls":[{"id":"trunc","name":"use_tool","arguments":"{\"tool_name\": \"create_comment\", \"description\": \"**问题原因"}]}),
            json!({"type":"tool_result","tool_call_id":"trunc","content":"Failed to parse arguments for tool `use_tool`"}),
        ],
    )?;
    let detail = session::reader(SourceApp::GrokBuild).parse_detail(&path)?;
    let tool = &detail.messages[1].blocks[0];
    assert_eq!(tool.kind, "tool_use");
    assert_eq!(tool.tool_name.as_deref(), Some("use_tool"));
    assert_eq!(
        tool.payload.as_ref().unwrap()["input"]["raw"],
        "{\"tool_name\": \"create_comment\", \"description\": \"**问题原因"
    );
    Ok(())
}

#[test]
fn grokbuild_listing_reads_only_summary_and_refreshes_index() -> Result<()> {
    let home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(home.as_path());
    let path = fixture(home.as_path(), "listed")?;
    fs::write(path.with_file_name("chat_history.jsonl"), "{invalid")?;
    fs::write(path.with_file_name("events.jsonl"), "{invalid")?;
    fs::write(path.with_file_name("updates.jsonl"), "{invalid")?;
    let state = state::session_index::SessionIndexState::default();
    let page = session::catalog::list_sessions_inner(
        &state,
        session::catalog::SourceSelection::One(SourceApp::GrokBuild),
        0,
        20,
        "",
        false,
        true,
    )?;
    assert_eq!(page.total_count, 1);
    assert_eq!(page.sessions[0].title, "Synthetic title");
    let mut value: Value = serde_json::from_slice(&fs::read(&path)?)?;
    value["generated_title"] = json!("Updated title");
    fs::write(&path, serde_json::to_vec(&value)?)?;
    fixture(home.as_path(), "second")?;
    let page = session::catalog::list_sessions_inner(
        &state,
        session::catalog::SourceSelection::All,
        0,
        20,
        "",
        false,
        true,
    )?;
    assert_eq!(page.total_count, 2);
    assert!(page.sessions.iter().any(|s| s.title == "Updated title"));
    assert!(
        session::catalog::detect_sources_inner(&state)?
            .iter()
            .any(|s| s.app == SourceApp::GrokBuild && s.session_count == 2)
    );
    Ok(())
}

#[test]
fn grokbuild_rejects_external_paths_and_mismatched_ids() -> Result<()> {
    let home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(home.as_path());
    let path = fixture(home.as_path(), "safe")?;
    let outside = home.as_path().join("summary.json");
    fs::copy(&path, &outside)?;
    assert!(
        session::reader(SourceApp::GrokBuild)
            .parse_detail(&outside)
            .is_err()
    );
    assert!(
        session::timeline::get_session_inner(SourceApp::GrokBuild, "other", path.to_str()).is_err()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, path.with_file_name("chat_history.jsonl"))?;
        assert!(
            session::reader(SourceApp::GrokBuild)
                .parse_detail(&path)
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn grokbuild_custom_home_is_used_for_listing_and_resolution() -> Result<()> {
    let home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(&home);
    fixture(&home, "custom")?;
    let custom = home.join("custom-grok");
    fs::rename(home.join(".grok"), &custom)?;
    unsafe { env::set_var("GROK_HOME", &custom) };
    let reader = session::reader(SourceApp::GrokBuild);
    assert_eq!(reader.list_entries()?.len(), 1);
    assert_eq!(
        reader.resolve_path("custom")?,
        custom
            .join("sessions/not-a-cwd/custom/summary.json")
            .canonicalize()?
    );
    Ok(())
}

#[test]
fn grokbuild_large_events_page_and_auxiliary_changes_are_fresh() -> Result<()> {
    let home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(&home);
    let path = fixture(&home, "large")?;
    let reader = session::reader(SourceApp::GrokBuild);
    let mut file = std::io::BufWriter::new(File::create(path.with_file_name("events.jsonl"))?);
    for _ in 0..185_000 {
        writeln!(
            file,
            "{{\"type\":\"phase_changed\",\"ts\":\"2026-01-01T00:00:00Z\"}}"
        )?;
    }
    file.flush()?;
    let page = reader.parse_events_page(&path, 184_998, 40)?;
    assert_eq!(page.total_count, 185_000);
    assert_eq!(page.events.len(), 2);
    assert!(!page.has_more);
    assert_eq!(page.events[0].id, "grok-event-184998");
    write_jsonl(
        &path.with_file_name("events.jsonl"),
        &[
            json!({"type":"future_event", "outcome":"completed", "encrypted_content":"NEVER_EXPOSE_SYNTHETIC"}),
        ],
    )?;
    let page = reader.parse_events_page(&path, 0, 40)?;
    assert_eq!(page.total_count, 1);
    assert_eq!(page.events[0].kind, "future_event");
    assert!(!serde_json::to_string(&page)?.contains("encrypted_content"));
    write_jsonl(
        &path.with_file_name("chat_history.jsonl"),
        &[json!({"type":"assistant", "content":"First"})],
    )?;
    assert_eq!(reader.parse_messages_page(&path, 0, 40)?.total_count, 1);
    write_jsonl(
        &path.with_file_name("chat_history.jsonl"),
        &[json!({"type":"assistant", "content":"Second"})],
    )?;
    assert_eq!(
        reader.parse_messages_page(&path, 0, 40)?.messages[0].blocks[0]
            .text
            .as_deref(),
        Some("Second")
    );
    Ok(())
}

#[test]
fn grokbuild_import_and_delete_boundaries_and_export_roundtrip() -> Result<()> {
    let home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&home)?;
    let _guard = TestEnvGuard::set_home(home.as_path());
    let path = fixture(home.as_path(), "export")?;
    history(&path)?;
    let preview = session::import::preview_import_inner(
        SourceApp::GrokBuild,
        "export",
        SourceApp::GrokBuild,
        path.to_str(),
    )?;
    assert!(!preview.supported && preview.created_paths.is_empty());
    assert!(
        session::import::import_session_inner(
            SourceApp::GrokBuild,
            "export",
            SourceApp::GrokBuild,
            path.to_str()
        )
        .is_err()
    );
    assert!(
        session::delete::delete_session_inner(
            &state::session_index::SessionIndexState::default(),
            SourceApp::GrokBuild,
            "export",
            path.to_str()
        )
        .is_err()
    );
    assert!(path.exists());
    for target in [SourceApp::Codex, SourceApp::OpenCode] {
        let preview = session::import::preview_import_inner(
            SourceApp::GrokBuild,
            "export",
            target,
            path.to_str(),
        )?;
        assert!(preview.supported);
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.contains("not tool failure flags"))
        );
    }
    assert!(session::exporter(SourceApp::GrokBuild).is_err());
    for target in [SourceApp::Pi, SourceApp::ClaudeCode] {
        let result = session::import::import_session_inner(
            SourceApp::GrokBuild,
            "export",
            target,
            path.to_str(),
        )?;
        let preview = session::import::preview_import_inner(
            target,
            &result.created_session_id,
            SourceApp::GrokBuild,
            None,
        )?;
        assert!(!preview.supported && preview.created_paths.is_empty());
        assert!(
            session::import::import_session_inner(
                target,
                &result.created_session_id,
                SourceApp::GrokBuild,
                None
            )
            .is_err()
        );
        let detail =
            session::timeline::get_session_inner(target, &result.created_session_id, None)?;
        let blocks = detail
            .messages
            .iter()
            .flat_map(|m| &m.blocks)
            .collect::<Vec<_>>();
        assert!(
            blocks
                .iter()
                .any(|b| b.kind == "thinking" && b.text.as_deref() == Some("Synthetic thought"))
        );
        assert!(
            blocks
                .iter()
                .any(|b| b.text.as_deref() == Some("Synthetic answer"))
        );
        assert!(blocks.iter().any(|b| b.kind == "tool_result"
            && b.tool_call_id.as_deref() == Some("bad")
            && b.is_error == Some(true)));
    }
    Ok(())
}
