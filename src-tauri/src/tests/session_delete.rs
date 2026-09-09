use super::*;

#[test]
fn deleting_codex_family_removes_all_member_files() -> Result<()> {
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
        &[json!({
            "timestamp": "2026-04-21T12:00:00.000Z",
            "type": "session_meta",
            "payload": { "id": root_id, "cwd": "/tmp/workspace" }
        })],
    )?;
    write_jsonl(
        &child_path,
        &[json!({
            "timestamp": "2026-04-21T15:00:00.000Z",
            "type": "session_meta",
            "payload": {
                "id": child_id,
                "cwd": "/tmp/workspace",
                "source": { "subagent": { "thread_spawn": { "parent_thread_id": root_id } } }
            }
        })],
    )?;

    let result = session::delete::delete_session_inner(
        &state::session_index::SessionIndexState::default(),
        SourceApp::Codex,
        root_id,
        Some(root_path.to_string_lossy().as_ref()),
    )?;

    assert_eq!(result.deleted_session_id, root_id);
    assert_eq!(result.deleted_paths.len(), 2);
    assert!(!root_path.exists());
    assert!(!child_path.exists());

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn deleting_codex_family_clears_all_state_databases() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let root_id = "66666666-6666-4666-8666-666666666666";
    let root_path = temp_home
        .join(".codex/sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T12-00-00-{root_id}.jsonl"));

    write_jsonl(
        &root_path,
        &[json!({
            "timestamp": "2026-04-21T12:00:00.000Z",
            "type": "session_meta",
            "payload": { "id": root_id, "cwd": "/tmp/workspace" }
        })],
    )?;

    let codex_root = session::codex::root()?;
    fs::create_dir_all(&codex_root)?;
    let state_db_paths = [
        codex_root.join("state_20260421.sqlite"),
        codex_root.join("state_20260422.sqlite"),
    ];

    for db_path in &state_db_paths {
        let connection = Connection::open(db_path)?;
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY)", [])?;
        connection.execute(
            "CREATE TABLE thread_spawn_edges (parent_thread_id TEXT, child_thread_id TEXT)",
            [],
        )?;
        connection.execute("INSERT INTO threads (id) VALUES (?1)", params![root_id])?;
        connection.execute(
            "INSERT INTO thread_spawn_edges (parent_thread_id, child_thread_id) VALUES (?1, ?1)",
            params![root_id],
        )?;
    }

    session::delete::delete_session_inner(
        &state::session_index::SessionIndexState::default(),
        SourceApp::Codex,
        root_id,
        Some(root_path.to_string_lossy().as_ref()),
    )?;

    for db_path in &state_db_paths {
        let connection = Connection::open(db_path)?;
        let thread_count = connection.query_row(
            "SELECT COUNT(*) FROM threads WHERE id = ?1",
            params![root_id],
            |row| row.get::<_, i64>(0),
        )?;
        let edge_count = connection.query_row(
            "SELECT COUNT(*) FROM thread_spawn_edges WHERE parent_thread_id = ?1 OR child_thread_id = ?1",
            params![root_id],
            |row| row.get::<_, i64>(0),
        )?;

        assert_eq!(
            thread_count,
            0,
            "expected thread row cleared in {}",
            db_path.display()
        );
        assert_eq!(
            edge_count,
            0,
            "expected edge rows cleared in {}",
            db_path.display()
        );
    }

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn deleting_claude_family_removes_transcripts_and_side_dirs() -> Result<()> {
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
    let side_dir = temp_home
        .join(".claude/projects")
        .join(root_id)
        .join("subagents");
    let file_history_dir = temp_home.join(".claude/file-history").join(root_id);
    let session_env_dir = temp_home.join(".claude/session-env").join(root_id);

    write_jsonl(
        &root_path,
        &[json!({
            "timestamp": "2026-04-21T12:00:00.000Z",
            "sessionId": root_id,
            "type": "user",
            "message": { "content": "Main task" }
        })],
    )?;
    write_jsonl(
        &child_path,
        &[json!({
            "timestamp": "2026-04-21T15:00:00.000Z",
            "sessionId": root_id,
            "type": "user",
            "message": { "content": "Child task" }
        })],
    )?;
    fs::write(
        &child_meta_path,
        r#"{"agentType":"Explore","description":"Find fetch_rss scheduling code"}"#,
    )?;
    fs::create_dir_all(&side_dir)?;
    fs::write(side_dir.join(format!("agent-{child_id}.jsonl")), "{}")?;
    fs::create_dir_all(&file_history_dir)?;
    fs::write(file_history_dir.join("sample@v1"), "{}")?;
    fs::create_dir_all(&session_env_dir)?;
    fs::write(session_env_dir.join("env.json"), "{}")?;

    let result = session::delete::delete_session_inner(
        &state::session_index::SessionIndexState::default(),
        SourceApp::ClaudeCode,
        root_id,
        Some(root_path.to_string_lossy().as_ref()),
    )?;

    assert_eq!(result.deleted_session_id, root_id);
    assert_eq!(result.deleted_paths.len(), 2);
    assert!(!root_path.exists());
    assert!(!child_path.exists());
    assert!(!child_meta_path.exists());
    assert!(!side_dir.exists());
    assert!(!file_history_dir.exists());
    assert!(!session_env_dir.exists());

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn deleting_pi_session_removes_only_the_selected_file() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let selected = temp_home
        .join(".pi/agent/sessions/--tmp-pi-project--/2026-09-04T10-00-00-000Z_pi-delete.jsonl");
    let other = temp_home
        .join(".pi/agent/sessions/--tmp-pi-project--/2026-09-04T10-00-00-000Z_pi-other.jsonl");
    let header = |id: &str| {
        json!({
            "type": "session", "version": 3, "id": id,
            "timestamp": "2026-09-04T10:00:00.000Z", "cwd": "/tmp/pi-project"
        })
    };
    write_jsonl(&selected, &[header("pi-delete")])?;
    write_jsonl(&other, &[header("pi-other")])?;

    let result = session::delete::delete_session_inner(
        &state::session_index::SessionIndexState::default(),
        SourceApp::Pi,
        "pi-delete",
        Some(selected.to_string_lossy().as_ref()),
    )?;
    assert_eq!(result.deleted_session_id, "pi-delete");
    assert!(!selected.exists());
    assert!(other.exists());
    Ok(())
}

// The fake `opencode` executable is a shell script and the PATH splice uses
// the Unix ':' separator, so the real CLI would run on Windows instead.
#[cfg(unix)]
#[test]
fn deleting_opencode_family_uses_cli_and_cleans_diff_files() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let root_id = "ses_root_session";
    let child_id = "ses_child_session";
    seed_opencode_family(root_id, child_id)?;

    let bin_dir = temp_home.join("bin");
    fs::create_dir_all(&bin_dir)?;
    let log_path = temp_home.join("opencode-delete.log");
    let script_path = bin_dir.join("opencode");
    fs::write(
        &script_path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"session\" ] && [ \"$2\" = \"delete\" ]; then\n  printf '%s\\n' \"$3\" >> '{}'\n  exit 0\nfi\nexit 1\n",
            log_path.display()
        ),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&script_path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions)?;
    }

    let original_path = env::var("PATH").unwrap_or_default();
    unsafe { env::set_var("PATH", format!("{}:{}", bin_dir.display(), original_path)) };

    let root_diff = temp_home
        .join(".local/share/opencode/storage/session_diff")
        .join(format!("{root_id}.json"));
    let child_diff = temp_home
        .join(".local/share/opencode/storage/session_diff")
        .join(format!("{child_id}.json"));
    if let Some(parent) = root_diff.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&root_diff, "{}")?;
    fs::write(&child_diff, "{}")?;

    let root_path = temp_home
        .join(".local/share/opencode/session")
        .join(format!("{root_id}.opencode"));

    let result = session::delete::delete_session_inner(
        &state::session_index::SessionIndexState::default(),
        SourceApp::OpenCode,
        root_id,
        Some(root_path.to_string_lossy().as_ref()),
    )?;

    let deleted = fs::read_to_string(&log_path)?;
    let deleted_ids = deleted.lines().collect::<HashSet<_>>();

    assert_eq!(result.deleted_session_id, root_id);
    assert!(
        result.deleted_paths.len() >= 2,
        "expected root and child paths, got {:?}",
        result.deleted_paths
    );
    assert!(deleted_ids.contains(root_id));
    assert!(deleted_ids.contains(child_id));
    assert!(!root_diff.exists());
    assert!(!child_diff.exists());

    unsafe { env::set_var("PATH", original_path) };
    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}
