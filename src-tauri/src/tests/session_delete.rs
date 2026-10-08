use super::*;
use super::grokbuild::{fixture as grok_fixture, history as grok_history, subagent_fixture};

// 官方删除 CLI 用跨平台假可执行文件顶替(见 fake_cli_source 注释)。
#[test]
fn deleting_codex_family_invokes_official_cli() -> Result<()> {
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

    // 官方 CLI 只对 state 库里存在的 thread 生效；预检依赖 threads 行。
    let codex_root = session::codex::root()?;
    fs::create_dir_all(&codex_root)?;
    let state_db = codex_root.join("state_20260421.sqlite");
    let connection = Connection::open(&state_db)?;
    connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY)", [])?;
    connection.execute("INSERT INTO threads (id) VALUES (?1)", params![root_id])?;
    connection.execute("INSERT INTO threads (id) VALUES (?1)", params![child_id])?;

    let bin_dir = temp_home.join("bin");
    fs::create_dir_all(&bin_dir)?;
    install_fake_cli(&bin_dir, "codex")?;
    let original_path = prepend_to_path(&bin_dir);

    let result = session::delete::delete_session_inner(
        &state::session_index::SessionIndexState::default(),
        SourceApp::Codex,
        root_id,
        Some(root_path.to_string_lossy().as_ref()),
    )?;

    let deleted = fs::read_to_string(fake_cli_log_path(&bin_dir))?;
    let deleted_ids = deleted.lines().collect::<HashSet<_>>();

    unsafe { env::set_var("PATH", &original_path) };

    assert_eq!(result.deleted_session_id, root_id);
    assert!(
        result.deleted_paths.len() >= 2,
        "expected root and child paths, got {:?}",
        result.deleted_paths
    );
    assert!(deleted_ids.contains(root_id));
    assert!(deleted_ids.contains(child_id));

    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

#[test]
fn deleting_codex_skips_members_missing_from_state_db() -> Result<()> {
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

    // 会话文件存在但 state 库没有 threads 行（如官方已级联删除后的残留）：
    // 跳过 CLI 调用，而不是让 not found 报错中断。PATH 上没有 codex
    // 可执行文件，若误调 CLI 会因 spawn 失败让本测试报错。
    let codex_root = session::codex::root()?;
    fs::create_dir_all(&codex_root)?;
    let state_db = codex_root.join("state_20260421.sqlite");
    let connection = Connection::open(&state_db)?;
    connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY)", [])?;

    let original_path = env::var("PATH").unwrap_or_default();
    unsafe { env::set_var("PATH", temp_home.join("empty-bin").display().to_string()) };

    let result = session::delete::delete_session_inner(
        &state::session_index::SessionIndexState::default(),
        SourceApp::Codex,
        root_id,
        Some(root_path.to_string_lossy().as_ref()),
    )?;

    unsafe { env::set_var("PATH", original_path) };

    assert_eq!(result.deleted_session_id, root_id);

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

// 同 codex:假可执行文件 + PATH 前插顶替官方 CLI。
#[test]
fn deleting_opencode_family_uses_cli() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let root_id = "ses_root_session";
    let child_id = "ses_child_session";
    seed_opencode_family(root_id, child_id)?;

    let bin_dir = temp_home.join("bin");
    fs::create_dir_all(&bin_dir)?;
    install_fake_cli(&bin_dir, "opencode")?;
    let original_path = prepend_to_path(&bin_dir);

    let root_path = session::opencode::session_path(root_id);

    let result = session::delete::delete_session_inner(
        &state::session_index::SessionIndexState::default(),
        SourceApp::OpenCode,
        root_id,
        Some(root_path.to_string_lossy().as_ref()),
    )?;

    let deleted = fs::read_to_string(fake_cli_log_path(&bin_dir))?;
    let deleted_ids = deleted.lines().collect::<HashSet<_>>();

    assert_eq!(result.deleted_session_id, root_id);
    assert!(
        result.deleted_paths.len() >= 2,
        "expected root and child paths, got {:?}",
        result.deleted_paths
    );
    assert!(deleted_ids.contains(root_id));
    assert!(deleted_ids.contains(child_id));

    unsafe { env::set_var("PATH", &original_path) };
    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

fn seed_grok_search_index(home: &Path, session_ids: &[&str]) -> Result<()> {
    let db_path = home.join(".grok/sessions/session_search.sqlite");
    let connection = Connection::open(&db_path)?;
    connection.execute(
        "CREATE TABLE session_docs (
            session_id TEXT PRIMARY KEY,
            cwd TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            content_hash TEXT NOT NULL
        , last_indexed_offset INTEGER NOT NULL DEFAULT 0)",
        [],
    )?;
    for id in session_ids {
        connection.execute(
            "INSERT INTO session_docs (session_id, cwd, updated_at, title, content, content_hash) VALUES (?1, '/synthetic', 0, 'title', 'content', 'hash')",
            params![id],
        )?;
    }
    Ok(())
}

fn grok_search_rows(home: &Path, session_id: &str) -> Result<i64> {
    let db_path = home.join(".grok/sessions/session_search.sqlite");
    let connection = Connection::open(&db_path)?;
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM session_docs WHERE session_id = ?1",
        params![session_id],
        |row| row.get::<_, i64>(0),
    )?)
}

// 官方删除 CLI(codex/grok/opencode)统一用假可执行文件顶替:src/bin/reins-fake-cli.rs
// 是跨平台的替身,拷进各测试自己的 bin 目录并按平台命名(unix 上 fs::copy 保留
// 可执行位;Windows 上 CreateProcess 只按 .exe 解析 PATH,sh 脚本与 .cmd 都顶替不了)。
fn fake_cli_source() -> Result<PathBuf> {
    if let Some(path) = option_env!("CARGO_BIN_EXE_reins-fake-cli") {
        return Ok(PathBuf::from(path));
    }
    // 单元测试没有 CARGO_BIN_EXE_* 时按标准 cargo 布局从自身位置推导:
    // 测试进程在 <target>/<profile>/deps/ 下,bin 产物在上一级。
    // 注意 `cargo test --lib` 不构建 bin 目标,项目脚本均为全量 `cargo test`。
    let exe_name = if cfg!(windows) {
        "reins-fake-cli.exe"
    } else {
        "reins-fake-cli"
    };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().and_then(Path::parent).map(|dir| dir.join(exe_name)))
        .ok_or_else(|| anyhow::anyhow!("定位 reins-fake-cli 构建产物失败"))
}

fn install_fake_cli(bin_dir: &Path, name: &str) -> Result<()> {
    let file_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    fs::copy(fake_cli_source()?, bin_dir.join(file_name))?;
    Ok(())
}

// 假 CLI 把删除的会话 id 记在自身所在目录。
fn fake_cli_log_path(bin_dir: &Path) -> PathBuf {
    bin_dir.join("fake-cli.log")
}

// PATH 前插(分隔符跨平台),返回原值供恢复。
fn prepend_to_path(bin_dir: &Path) -> std::ffi::OsString {
    let original = env::var_os("PATH").unwrap_or_default();
    let joined = env::join_paths(
        std::iter::once(bin_dir.to_path_buf()).chain(env::split_paths(&original)),
    )
    .expect("PATH 拼接失败");
    unsafe { env::set_var("PATH", &joined) };
    original
}

// 官方删除 CLI 用假可执行文件顶替(见 fake_cli_source 注释)。
#[test]
fn deleting_grokbuild_family_removes_member_dirs_and_search_rows() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let parent_path = grok_fixture(&temp_home, "grok-parent")?;
    grok_history(&parent_path)?;
    subagent_fixture(&temp_home, "grok-parent", "grok-child", "at1.delete", "Child answer")?;
    // prompt_history.jsonl 是 cwd 级共享文件，删除会话时必须保留。
    let bucket = temp_home.join(".grok/sessions/not-a-cwd");
    fs::write(bucket.join("prompt_history.jsonl"), "{}\n")?;
    seed_grok_search_index(
        &temp_home,
        &["grok-parent", "grok-child", "unrelated-session"],
    )?;
    let bin_dir = temp_home.join("bin");
    fs::create_dir_all(&bin_dir)?;
    install_fake_cli(&bin_dir, "grok")?;
    let original_path = prepend_to_path(&bin_dir);

    let result = session::delete::delete_session_inner(
        &state::session_index::SessionIndexState::default(),
        SourceApp::GrokBuild,
        "grok-parent",
        Some(parent_path.to_string_lossy().as_ref()),
    )?;

    // root 交给官方命令:假 CLI 忠实模拟官方行为(记录参数 + 删除 root 目录)。
    let official_calls = fs::read_to_string(fake_cli_log_path(&bin_dir))?;
    assert_eq!(official_calls.lines().collect::<Vec<_>>(), ["grok-parent"]);

    assert_eq!(result.deleted_session_id, "grok-parent");
    assert!(!bucket.join("grok-parent").exists());
    assert!(!bucket.join("grok-child").exists());
    assert!(bucket.join("prompt_history.jsonl").exists());
    assert_eq!(grok_search_rows(&temp_home, "grok-parent")?, 1);
    assert_eq!(grok_search_rows(&temp_home, "grok-child")?, 0);
    assert_eq!(grok_search_rows(&temp_home, "unrelated-session")?, 1);

    unsafe { env::set_var("PATH", &original_path) };
    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}

// 同上:假可执行文件顶替官方 CLI。
#[test]
fn deleting_grokbuild_session_prunes_empty_cwd_bucket() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let parent_path = grok_fixture(&temp_home, "grok-solo")?;
    grok_history(&parent_path)?;
    let bucket = temp_home.join(".grok/sessions/not-a-cwd");
    // 官方命令删除 root 目录，假 CLI 模拟同样的文件效果。
    let bin_dir = temp_home.join("bin");
    fs::create_dir_all(&bin_dir)?;
    install_fake_cli(&bin_dir, "grok")?;
    let original_path = prepend_to_path(&bin_dir);

    session::delete::delete_session_inner(
        &state::session_index::SessionIndexState::default(),
        SourceApp::GrokBuild,
        "grok-solo",
        Some(parent_path.to_string_lossy().as_ref()),
    )?;

    assert!(!bucket.join("grok-solo").exists());
    // bucket 内已无共享文件，空目录应被清理。
    assert!(!bucket.exists());

    unsafe { env::set_var("PATH", &original_path) };
    fs::remove_dir_all(&temp_home).ok();
    Ok(())
}
