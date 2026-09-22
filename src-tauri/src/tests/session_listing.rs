use super::*;

#[test]
fn list_sessions_filters_by_session_id_on_backend() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let target_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let other_id = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

    seed_codex_session(&temp_home, target_id)?;
    seed_codex_session(&temp_home, other_id)?;

    let page = session::catalog::list_sessions_inner(
        &state::session_index::SessionIndexState::default(),
        session::catalog::SourceSelection::One(SourceApp::Codex),
        0,
        20,
        "aaaa-4aaa",
        false,
        true,
    )?;

    assert_eq!(page.total_count, 1);
    assert_eq!(page.sessions.len(), 1);
    assert_eq!(page.sessions[0].source_session_id, target_id);

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn list_sessions_reverse_order_applies_before_paging() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let oldest_id = "11111111-1111-4111-8111-111111111111";
    let middle_id = "22222222-2222-4222-8222-222222222222";
    let newest_id = "33333333-3333-4333-8333-333333333333";

    seed_codex_session(&temp_home, oldest_id)?;
    seed_codex_session(&temp_home, middle_id)?;
    seed_codex_session(&temp_home, newest_id)?;

    let default_page = session::catalog::list_sessions_inner(
        &state::session_index::SessionIndexState::default(),
        session::catalog::SourceSelection::One(SourceApp::Codex),
        0,
        20,
        "",
        false,
        true,
    )?;
    let reversed_page = session::catalog::list_sessions_inner(
        &state::session_index::SessionIndexState::default(),
        session::catalog::SourceSelection::One(SourceApp::Codex),
        0,
        2,
        "",
        true,
        true,
    )?;

    let expected_reversed = default_page
        .sessions
        .iter()
        .rev()
        .take(2)
        .map(|session| session.source_session_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        reversed_page
            .sessions
            .iter()
            .map(|session| session.source_session_id.as_str())
            .collect::<Vec<_>>(),
        expected_reversed
    );

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn claude_session_listing_uses_transcript_timestamps() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let older_id = "11111111-1111-4111-8111-111111111111";
    let newer_id = "22222222-2222-4222-8222-222222222222";
    let project_dir = temp_home.join(".claude/projects/demo-project");
    let older_path = project_dir.join(format!("{older_id}.jsonl"));
    let newer_path = project_dir.join(format!("{newer_id}.jsonl"));
    let older_updated_at = support::time::parse_timestamp("2026-04-21T12:30:00.000Z").unwrap();
    let newer_updated_at = support::time::parse_timestamp("2026-04-22T09:45:00.000Z").unwrap();

    write_jsonl(
        &newer_path,
        &[
            json!({
                "timestamp": "2026-04-22T09:00:00.000Z",
                "sessionId": newer_id,
                "type": "user",
                "message": {
                    "content": "Newer task"
                }
            }),
            json!({
                "timestamp": "2026-04-22T09:45:00.000Z",
                "sessionId": newer_id,
                "type": "assistant",
                "message": {
                    "content": "Newer answer"
                }
            }),
        ],
    )?;
    write_jsonl(
        &older_path,
        &[
            json!({
                "timestamp": "2026-04-21T12:00:00.000Z",
                "sessionId": older_id,
                "type": "user",
                "message": {
                    "content": "Older task"
                }
            }),
            json!({
                "timestamp": "2026-04-21T12:30:00.000Z",
                "sessionId": older_id,
                "type": "assistant",
                "message": {
                    "content": "Older answer"
                }
            }),
        ],
    )?;

    let page = session::catalog::list_sessions_inner(
        &state::session_index::SessionIndexState::default(),
        session::catalog::SourceSelection::One(SourceApp::ClaudeCode),
        0,
        20,
        "",
        false,
        true,
    )?;

    assert_eq!(page.total_count, 2);
    assert_eq!(page.sessions[0].source_session_id, newer_id);
    assert_eq!(page.sessions[0].updated_at, Some(newer_updated_at));
    assert_eq!(page.sessions[1].source_session_id, older_id);
    assert_eq!(page.sessions[1].updated_at, Some(older_updated_at));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

// 合并视图：codex 与 claude 两个来源按 updated_at 全局排序、跨来源分页。
#[test]
fn merged_listing_interleaves_sources_and_paginates_across_sources() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let codex_id = "11111111-1111-4111-8111-111111111111";
    let claude_newest_id = "22222222-2222-4222-8222-222222222222";
    let claude_oldest_id = "33333333-3333-4333-8333-333333333333";

    // codex 会话夹在两个 claude 会话之间，验证合并排序真正跨来源交错。
    // codex 的 updated_at 语义是文件 mtime（产品既有行为），所以显式设置
    // mtime 到 claude 两个会话之间，而不是依赖写入时刻。
    let codex_transcript = temp_home
        .join(".codex/sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T10-00-00-{codex_id}.jsonl"));
    write_jsonl(
        &codex_transcript,
        &[
            json!({
                "timestamp": "2026-04-21T10:00:00.000Z",
                "type": "session_meta",
                "payload": { "id": codex_id, "cwd": "/tmp/reins/workspace" }
            }),
            json!({
                "timestamp": "2026-04-21T10:30:00.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "text", "text": "Middle codex task" }]
                }
            }),
        ],
    )?;
    // 2026-04-21T17:10Z，落在 claude 两个会话（04-20、04-22）之间。
    let codex_mtime =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1_776_767_400_000);
    fs::File::options()
        .write(true)
        .open(&codex_transcript)?
        .set_times(fs::FileTimes::new().set_modified(codex_mtime))?;
    let project_dir = temp_home.join(".claude/projects/demo-project");
    write_jsonl(
        &project_dir.join(format!("{claude_newest_id}.jsonl")),
        &[json!({
            "timestamp": "2026-04-22T09:45:00.000Z",
            "sessionId": claude_newest_id,
            "type": "user",
            "message": { "content": "Newest claude task" }
        })],
    )?;
    write_jsonl(
        &project_dir.join(format!("{claude_oldest_id}.jsonl")),
        &[json!({
            "timestamp": "2026-04-20T09:45:00.000Z",
            "sessionId": claude_oldest_id,
            "type": "user",
            "message": { "content": "Oldest claude task" }
        })],
    )?;

    let state = state::session_index::SessionIndexState::default();

    let first_page = session::catalog::list_sessions_inner(
        &state,
        session::catalog::SourceSelection::All,
        0,
        2,
        "",
        false,
        true,
    )?;
    assert_eq!(first_page.source_app, None);
    assert_eq!(first_page.total_count, 3);
    assert_eq!(
        first_page
            .sessions
            .iter()
            .map(|session| session.source_session_id.as_str())
            .collect::<Vec<_>>(),
        [claude_newest_id, codex_id]
    );
    assert!(first_page.has_more);
    assert_eq!(first_page.next_offset, Some(2));

    let second_page = session::catalog::list_sessions_inner(
        &state,
        session::catalog::SourceSelection::All,
        2,
        2,
        "",
        false,
        true,
    )?;
    assert_eq!(
        second_page
            .sessions
            .iter()
            .map(|session| session.source_session_id.as_str())
            .collect::<Vec<_>>(),
        [claude_oldest_id]
    );
    assert!(!second_page.has_more);

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

// 合并视图只包含根目录存在的来源；搜索在合并结果上过滤。
#[test]
fn merged_listing_skips_unavailable_sources_and_filters_by_query() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    // 只准备 claude，不创建 .codex 目录。
    let claude_id = "44444444-4444-4444-8444-444444444444";
    let project_dir = temp_home.join(".claude/projects/demo-project");
    write_jsonl(
        &project_dir.join(format!("{claude_id}.jsonl")),
        &[json!({
            "timestamp": "2026-04-22T09:45:00.000Z",
            "sessionId": claude_id,
            "type": "user",
            "message": { "content": "Claude only task" }
        })],
    )?;

    let state = state::session_index::SessionIndexState::default();

    let page = session::catalog::list_sessions_inner(
        &state,
        session::catalog::SourceSelection::All,
        0,
        20,
        "",
        false,
        true,
    )?;
    assert_eq!(page.total_count, 1);
    assert_eq!(page.sessions[0].source_app, SourceApp::ClaudeCode);

    let filtered = session::catalog::list_sessions_inner(
        &state,
        session::catalog::SourceSelection::All,
        0,
        20,
        "claude only",
        false,
        true,
    )?;
    assert_eq!(filtered.total_count, 1);
    assert_eq!(filtered.sessions[0].source_session_id, claude_id);

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

// 刷新合并视图时 selected_source 为 None（前端映射回"全部"）。
#[test]
fn refresh_with_all_selection_returns_merged_page() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let codex_id = "55555555-5555-4555-8555-555555555555";
    seed_codex_session(&temp_home, codex_id)?;

    let result = session::catalog::refresh_sessions_inner(
        &state::session_index::SessionIndexState::default(),
        session::catalog::SourceSelection::All,
        0,
        20,
        "",
        false,
    )?;
    assert_eq!(result.selected_source, None);
    assert_eq!(result.page.source_app, None);
    assert_eq!(result.page.total_count, 1);

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn slice_page_handles_offsets_beyond_range_without_panicking() -> Result<()> {
    let items = vec!["a", "b", "c"];

    let (page, start, next_offset, total_count) = support::paging::slice_page(&items, 10, 2);

    assert!(page.is_empty());
    assert_eq!(start, items.len());
    assert_eq!(next_offset, None);
    assert_eq!(total_count, items.len());

    Ok(())
}
