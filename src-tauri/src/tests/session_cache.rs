use super::*;

const CACHE_DB_RELATIVE_PATH: &str = ".reins/session-summary-cache_v2.db";

fn cache_db_path(temp_home: &Path) -> PathBuf {
    temp_home.join(CACHE_DB_RELATIVE_PATH)
}

fn cache_row_count(temp_home: &Path) -> Result<i64> {
    let connection = Connection::open(cache_db_path(temp_home))?;
    Ok(
        connection.query_row("SELECT COUNT(*) FROM session_summary_cache", [], |row| {
            row.get(0)
        })?,
    )
}

// 直接改写缓存行里的标题：若后续列表读到篡改值，证明摘要来自持久缓存
// 而不是重新解析转录文件。
fn tamper_cached_title(temp_home: &Path, from: &str, to: &str) -> Result<()> {
    let connection = Connection::open(cache_db_path(temp_home))?;
    let updated = connection.execute(
        "UPDATE session_summary_cache SET summary = replace(summary, ?1, ?2)",
        params![from, to],
    )?;
    anyhow::ensure!(updated > 0, "no cache row matched {from}");
    Ok(())
}

fn write_codex_transcript(temp_home: &Path, session_id: &str, question: &str) -> Result<PathBuf> {
    let transcript_path = temp_home
        .join(".codex/sessions/2026/09/04")
        .join(format!("rollout-2026-09-04T09-00-00-{session_id}.jsonl"));

    write_jsonl(
        &transcript_path,
        &[
            json!({
                "timestamp": "2026-09-04T09:00:00.000Z",
                "type": "session_meta",
                "payload": {
                    "id": session_id,
                    "cwd": "/tmp/reins/workspace"
                }
            }),
            json!({
                "timestamp": "2026-09-04T09:00:01.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "text", "text": question }]
                }
            }),
        ],
    )?;

    Ok(transcript_path)
}

fn codex_titles() -> Result<Vec<String>> {
    Ok(session::reader(SourceApp::Codex)
        .list_entries()?
        .into_iter()
        .map(|entry| entry.summary.expect("list entries carry summaries").title)
        .collect())
}

// 模拟进程重启：清空后端内存缓存，保留持久缓存。
fn forget_in_memory_caches(source_app: SourceApp) -> Result<()> {
    session::reader(source_app).clear_cache()
}

// 子进程模式：由 persistence_survives_process_restart 通过环境变量唤起，
// 直接运行时（普通 cargo test）立即返回。
#[test]
fn persistence_child_helper() -> Result<()> {
    if env::var("AGENT_TOOLS_CACHE_CHILD").is_err() {
        return Ok(());
    }
    let path = Path::new("/tmp/some-transcript.jsonl");
    session::summary_cache::store(SourceApp::Codex, path, 100, &cached_summary());
    Ok(())
}

// 持久化的核心承诺是跨进程生效：子进程写入的缓存行，本进程必须能读到。
#[test]
fn persistence_survives_process_restart() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    // HOME 已被 TestEnvGuard 指向临时目录；子进程继承该环境变量。
    let output = Command::new(env::current_exe()?)
        // --exact 要求完整测试路径，短名会匹配到 0 个测试。
        .args(["tests::session_cache::persistence_child_helper", "--exact"])
        .env("AGENT_TOOLS_CACHE_CHILD", "1")
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "child helper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let path = Path::new("/tmp/some-transcript.jsonl");
    let loaded =
        session::summary_cache::load(SourceApp::Codex, path, 100).expect("cache hit after restart");
    assert_eq!(loaded.title, "persistent title");

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

fn cached_summary() -> SessionSummary {
    SessionSummary {
        source_app: SourceApp::Codex,
        source_session_id: "cached-session".to_string(),
        title: "persistent title".to_string(),
        cwd: None,
        git_branch: None,
        transcript_path: "/tmp/some-transcript.jsonl".to_string(),
        created_at: Some(1),
        updated_at: Some(2),
    }
}

#[test]
fn codex_cold_start_reads_title_from_persistent_cache() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_id = "55555555-5555-4555-8555-555555555555";
    write_codex_transcript(&temp_home, session_id, "First question")?;

    // 首次列表：全量解析并把摘要写入持久缓存。
    assert_eq!(codex_titles()?, ["First question"]);
    assert_eq!(cache_row_count(&temp_home)?, 1);

    tamper_cached_title(&temp_home, "First question", "TAMPERED CACHE TITLE")?;
    forget_in_memory_caches(SourceApp::Codex)?;

    // 冷启动：文件未变更，摘要必须命中持久缓存而不是重新解析。
    assert_eq!(codex_titles()?, ["TAMPERED CACHE TITLE"]);

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn codex_changed_file_reparses_and_refreshes_cache() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_id = "66666666-6666-4666-8666-666666666666";
    let transcript_path = write_codex_transcript(&temp_home, session_id, "First question")?;
    let original_mtime = support::time::file_modified_timestamp_millis(&transcript_path)?;

    assert_eq!(codex_titles()?, ["First question"]);

    // 追加内容会推进文件 mtime（水位线），缓存行随之失效。
    std::thread::sleep(std::time::Duration::from_millis(25));
    write_jsonl(
        &transcript_path,
        &[
            json!({
                "timestamp": "2026-09-04T09:00:00.000Z",
                "type": "session_meta",
                "payload": {
                    "id": session_id,
                    "cwd": "/tmp/reins/workspace"
                }
            }),
            json!({
                "timestamp": "2026-09-04T09:00:02.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "text", "text": "Second question" }]
                }
            }),
        ],
    )?;
    assert_ne!(
        support::time::file_modified_timestamp_millis(&transcript_path)?,
        original_mtime
    );

    forget_in_memory_caches(SourceApp::Codex)?;
    assert_eq!(codex_titles()?, ["Second question"]);

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn claude_cold_start_reads_title_from_persistent_cache() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let session_file = temp_home
        .join(".claude/projects/demo-project")
        .join("session-123.jsonl");
    write_jsonl(
        &session_file,
        &[json!({
            "type": "user",
            "message": { "content": "First question" }
        })],
    )?;

    let titles = || -> Result<Vec<String>> {
        Ok(session::reader(SourceApp::ClaudeCode)
            .list_entries()?
            .into_iter()
            .map(|entry| entry.summary.expect("list entries carry summaries").title)
            .collect())
    };

    assert_eq!(titles()?, ["First question"]);
    tamper_cached_title(&temp_home, "First question", "TAMPERED CACHE TITLE")?;
    forget_in_memory_caches(SourceApp::ClaudeCode)?;

    assert_eq!(titles()?, ["TAMPERED CACHE TITLE"]);

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn pi_summary_cache_survives_memory_clear_and_invalidates_on_mtime_change() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = temp_home
        .join(".pi/agent/sessions/--tmp-pi-project--/2026-09-04T10-00-00-000Z_pi-cache.jsonl");
    write_jsonl(
        &path,
        &[
            json!({
                "type": "session",
                "version": 3,
                "id": "pi-cache",
                "timestamp": "2026-09-04T10:00:00.000Z",
                "cwd": "/tmp/pi-project"
            }),
            json!({
                "type": "message",
                "id": "m1",
                "parentId": null,
                "timestamp": "2026-09-04T10:00:01.000Z",
                "message": { "role": "user", "content": "First Pi question" }
            }),
        ],
    )?;

    assert_eq!(
        session::reader(SourceApp::Pi).parse_summary(&path)?.title,
        "First Pi question"
    );
    assert_eq!(cache_row_count(&temp_home)?, 1);
    tamper_cached_title(&temp_home, "First Pi question", "Cached Pi title")?;
    session::reader(SourceApp::Pi).clear_cache()?;
    assert_eq!(
        session::reader(SourceApp::Pi).parse_summary(&path)?.title,
        "Cached Pi title"
    );

    let old_mtime = support::time::file_modified_timestamp_millis(&path)?;
    std::thread::sleep(std::time::Duration::from_millis(25));
    write_jsonl(
        &path,
        &[
            json!({
                "type": "session",
                "version": 3,
                "id": "pi-cache",
                "timestamp": "2026-09-04T10:00:00.000Z",
                "cwd": "/tmp/pi-project"
            }),
            json!({
                "type": "message",
                "id": "m1",
                "parentId": null,
                "timestamp": "2026-09-04T10:00:01.000Z",
                "message": { "role": "user", "content": "Second Pi question" }
            }),
        ],
    )?;
    assert_ne!(
        support::time::file_modified_timestamp_millis(&path)?,
        old_mtime
    );
    session::reader(SourceApp::Pi).clear_cache()?;
    assert_eq!(
        session::reader(SourceApp::Pi).parse_summary(&path)?.title,
        "Second Pi question"
    );
    Ok(())
}

#[test]
fn pi_timeline_cache_uses_file_mtime_as_its_freshness_boundary() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = temp_home.join(
        ".pi/agent/sessions/--tmp-pi-project--/2026-09-04T10-00-00-000Z_pi-timeline-cache.jsonl",
    );
    let header = json!({
        "type": "session",
        "version": 3,
        "id": "pi-timeline-cache",
        "timestamp": "2026-09-04T10:00:00.000Z",
        "cwd": "/tmp/pi-project"
    });
    let write_session = |text: &str| {
        write_jsonl(
            &path,
            &[
                header.clone(),
                json!({
                    "type": "message",
                    "id": "message-1",
                    "parentId": null,
                    "timestamp": "2026-09-04T10:00:01.000Z",
                    "message": { "role": "user", "content": text }
                }),
            ],
        )
    };

    write_session("first timeline")?;
    let original_mtime =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1_800_000_000_000);
    fs::File::options()
        .write(true)
        .open(&path)?
        .set_times(fs::FileTimes::new().set_modified(original_mtime))?;

    let first = session::reader(SourceApp::Pi).parse_detail(&path)?;
    assert!(first.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("first timeline"))
    }));

    write_session("second timeline")?;
    fs::File::options()
        .write(true)
        .open(&path)?
        .set_times(fs::FileTimes::new().set_modified(original_mtime))?;

    let cached = session::reader(SourceApp::Pi).parse_detail(&path)?;
    assert!(cached.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("first timeline"))
    }));
    assert!(!cached.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("second timeline"))
    }));

    let changed_mtime = original_mtime + std::time::Duration::from_millis(1);
    fs::File::options()
        .write(true)
        .open(&path)?
        .set_times(fs::FileTimes::new().set_modified(changed_mtime))?;

    let refreshed = session::reader(SourceApp::Pi).parse_detail(&path)?;
    assert!(refreshed.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("second timeline"))
    }));
    Ok(())
}

#[test]
fn clear_session_caches_empties_persistent_cache() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    write_codex_transcript(
        &temp_home,
        "77777777-7777-4777-8777-777777777777",
        "First question",
    )?;
    assert_eq!(codex_titles()?, ["First question"]);
    assert_eq!(cache_row_count(&temp_home)?, 1);

    // 用户触发的刷新是全量重建的逃生通道：持久缓存必须一并清空。
    session::catalog::clear_session_caches_inner(
        &state::session_index::SessionIndexState::default(),
    )?;
    assert_eq!(cache_row_count(&temp_home)?, 0);

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn clear_session_caches_empties_pi_persistent_cache() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let path = temp_home.join(
        ".pi/agent/sessions/--tmp-pi-project--/2026-09-04T10-00-00-000Z_pi-global-clear.jsonl",
    );
    write_jsonl(
        &path,
        &[
            json!({
                "type": "session", "version": 3, "id": "pi-global-clear",
                "timestamp": "2026-09-04T10:00:00.000Z", "cwd": "/tmp/pi-project"
            }),
            json!({
                "type": "message", "id": "m1", "parentId": null,
                "timestamp": "2026-09-04T10:00:01.000Z",
                "message": { "role": "user", "content": "clear Pi cache" }
            }),
        ],
    )?;
    session::reader(SourceApp::Pi).parse_summary(&path)?;
    assert_eq!(cache_row_count(&temp_home)?, 1);
    session::catalog::clear_session_caches_inner(
        &state::session_index::SessionIndexState::default(),
    )?;
    assert_eq!(cache_row_count(&temp_home)?, 0);
    Ok(())
}
