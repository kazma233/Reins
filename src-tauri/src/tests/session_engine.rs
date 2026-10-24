use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use rusqlite::Connection;
use serde_json::json;
use uuid::Uuid;

use crate::agents::spec_by_source_app;
use crate::session::family_index::FamilyRow;
use crate::session::reader_engine::FamilySpec;
use crate::session::{SessionReader, claude_code, codex, dsh, opencode, zcode};

// 读取器引擎按根直接构造:整个测试不碰进程 env、不持 TestEnvGuard 的全局
// 锁,可与其他 reader 测试并行。这是 reader 级测试脱离 env 的目标形态。
#[test]
fn codex_engine_reads_without_touching_process_env() -> Result<()> {
    let dir = env::temp_dir().join(format!("reins-engine-codex-{}", Uuid::new_v4()));
    let sessions = dir.join("sessions");
    fs::create_dir_all(&sessions)?;

    let session_id = "33333333-3333-4333-8333-333333333333";
    let transcript = sessions.join(format!("rollout-2026-04-21T12-00-00-{session_id}.jsonl"));
    super::write_jsonl(
        &transcript,
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
                "timestamp": "2026-04-21T12:00:02.000Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "How do I list the current files?"}
                    ]
                }
            }),
        ],
    )?;

    let store_dir: PathBuf = dir.join("store");
    let engine = codex::engine_at(dir.clone(), store_dir);

    let entries = engine.list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].summary.as_ref().expect("summary").title,
        "How do I list the current files?"
    );

    let summary = engine.parse_summary(session_id)?;
    assert_eq!(summary.source_session_id, session_id);

    let page = engine.parse_messages_page(session_id, 0, 10)?;
    assert_eq!(page.total_count, 1);

    let family = engine.family_for_id(session_id)?;
    assert_eq!(family.root.member_path().as_ref(), transcript.as_path());

    fs::remove_dir_all(&dir).ok();
    Ok(())
}

#[test]
fn claude_engine_reads_without_touching_process_env() -> Result<()> {
    let dir = env::temp_dir().join(format!("reins-engine-claude-{}", Uuid::new_v4()));
    let projects = dir.join("projects/reins");
    fs::create_dir_all(&projects)?;

    let session_id = "33333333-3333-4333-8333-333333333333";
    let transcript = projects.join(format!("{session_id}.jsonl"));
    super::write_jsonl(
        &transcript,
        &[json!({
            "type": "user",
            "timestamp": "2026-04-21T12:00:00.000Z",
            "sessionId": session_id,
            "cwd": "/tmp/reins/workspace",
            "message": {
                "role": "user",
                "content": [{"type": "text", "text": "How do I list the current files?"}]
            }
        })],
    )?;

    let engine = claude_code::engine_at(dir.clone(), dir.join("store"));

    let entries = engine.list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].summary.as_ref().expect("summary").title,
        "How do I list the current files?"
    );

    let summary = engine.parse_summary(session_id)?;
    assert_eq!(summary.source_session_id, session_id);

    let family = engine.family_for_id(session_id)?;
    assert_eq!(family.root.member_path().as_ref(), transcript.as_path());

    fs::remove_dir_all(&dir).ok();
    Ok(())
}

// not-found 错误主语（display_label）与 AgentSpec.label（产品名单一来源）
// 的一致性：五家引擎 reader 的错误文案拼写必须与界面其他位置同源，新增
// 覆写时在这里暴露拼写分叉。
fn assert_display_label_matches_agent_spec<R: FamilySpec>(spec: R) {
    let expected = spec_by_source_app(spec.app())
        .unwrap_or_else(|| panic!("{:?} 未登记 agents 清单", spec.app()))
        .label;
    assert_eq!(spec.display_label(), expected);
}

#[test]
fn reader_display_labels_match_agent_spec_labels() {
    assert_display_label_matches_agent_spec(codex::CodexSpec);
    assert_display_label_matches_agent_spec(claude_code::ClaudeSpec);
    assert_display_label_matches_agent_spec(opencode::OpenCodeSpec);
    assert_display_label_matches_agent_spec(zcode::ZcodeSpec);
    assert_display_label_matches_agent_spec(dsh::DshSpec);
}

// engine_at 的意图是完全脱离进程 env：夹具只写自定义根，全程不设置
// HOME。SQL 来源的数据查询必须从实例 scan_root 派生 db 路径。
#[test]
fn opencode_engine_reads_custom_root_without_env() -> Result<()> {
    let dir = env::temp_dir().join(format!("reins-engine-opencode-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir)?;
    super::seed_opencode_family_at(&dir, "ses_engine_root", "ses_engine_child")?;

    let engine = opencode::engine_at(dir.clone(), dir.join("store"));

    let entries = engine.list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].summary.as_ref().expect("summary").title,
        "Root task (+1 subagents)"
    );

    let page = engine.parse_messages_page("ses_engine_root", 0, 10)?;
    assert!(page.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("Root question"))
    }));

    fs::remove_dir_all(&dir).ok();
    Ok(())
}

#[test]
fn zcode_engine_reads_custom_root_without_env() -> Result<()> {
    let dir = env::temp_dir().join(format!("reins-engine-zcode-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir)?;
    let db_path = dir.join("db.sqlite");
    super::zcode::seed_zcode_family_at(&db_path, "ses_engine_root", "ses_engine_child")?;

    let engine = zcode::engine_at(db_path.clone(), dir.join("store"));

    let entries = engine.list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].summary.as_ref().expect("summary").title,
        "Root task (+1 subagents)"
    );

    let page = engine.parse_messages_page("ses_engine_root", 0, 10)?;
    assert!(page.messages.iter().any(|message| {
        message
            .blocks
            .iter()
            .any(|block| block.text.as_deref() == Some("帮我看下这个项目"))
    }));

    fs::remove_dir_all(&dir).ok();
    Ok(())
}

// 来源根目录存在但数据还没落地时按"还没有会话"处理,不把 no such table 或
// metadata 读取错误抛给用户。db 文件、数据表、sessions 子目录分属三种缺失形态。

#[test]
fn opencode_engine_without_session_v2_table_lists_empty() -> Result<()> {
    let dir = env::temp_dir().join(format!("reins-engine-opencode-empty-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir)?;
    // db 已存在但建表还没发生:session_v2 随 opencode 首次会话写入才创建。
    Connection::open(dir.join("opencode.db"))?.execute_batch("CREATE TABLE other (id TEXT);")?;

    let engine = opencode::engine_at(dir.clone(), dir.join("store"));
    assert!(engine.list_entries()?.is_empty());

    fs::remove_dir_all(&dir).ok();
    Ok(())
}

#[test]
fn opencode_engine_without_db_lists_empty_without_creating_it() -> Result<()> {
    let dir = env::temp_dir().join(format!("reins-engine-opencode-nodb-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir)?;

    let engine = opencode::engine_at(dir.clone(), dir.join("store"));
    assert!(engine.list_entries()?.is_empty());
    // 读取路径用读写模式打开 sqlite,不先判存在就会凭空建出空库。
    assert!(!dir.join("opencode.db").exists());

    fs::remove_dir_all(&dir).ok();
    Ok(())
}

#[test]
fn codex_engine_without_sessions_dir_lists_empty() -> Result<()> {
    let dir = env::temp_dir().join(format!("reins-engine-codex-empty-{}", Uuid::new_v4()));
    // 只建 codex 根目录(~/.codex 在登录/配置阶段就有),sessions 子目录还没有。
    fs::create_dir_all(&dir)?;

    let engine = codex::engine_at(dir.clone(), dir.join("store"));
    assert!(engine.list_entries()?.is_empty());

    fs::remove_dir_all(&dir).ok();
    Ok(())
}

#[test]
fn zcode_engine_without_session_table_lists_empty() -> Result<()> {
    let dir = env::temp_dir().join(format!("reins-engine-zcode-empty-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir)?;
    let db_path = dir.join("db.sqlite");
    Connection::open(&db_path)?.execute_batch("CREATE TABLE other (id TEXT);")?;

    let engine = zcode::engine_at(db_path.clone(), dir.join("store"));
    assert!(engine.list_entries()?.is_empty());

    fs::remove_dir_all(&dir).ok();
    Ok(())
}

// 贡献表增量:opencode 活跃写入(回写旧消息 tokens、追加新消息、删除消息)
// 后,下一次扫描按行级 (id, time_updated) 幂等更新聚合;增量结果必须与
// 全新 store 的全量提取一致。
#[test]
fn opencode_contrib_updates_incrementally_on_db_writes() -> Result<()> {
    let dir = env::temp_dir().join(format!("reins-opencode-contrib-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir)?;
    super::seed_opencode_family_at(&dir, "ses_contrib", "ses_contrib_child")?;
    let db_path = dir.join("opencode.db");

    {
        let connection = Connection::open(&db_path)?;
        super::seed_opencode_message_row(
            &connection,
            "ses_contrib",
            "msg-a1",
            1_744_366_402_000,
            2,
            "assistant",
            &json!({
                "time": { "created": 1_744_366_402_000_i64 },
                "model": { "providerID": "p1", "id": "model-x" },
                "tokens": { "input": 100, "output": 40, "reasoning": 10,
                            "cache": { "read": 5, "write": 2 } },
            }),
        )?;
    }

    let engine = opencode::engine_at(dir.clone(), dir.join("store"));

    // 首次扫描全量提取;usage 口径:input 不含缓存读,output 并 reasoning。
    let first = engine.list_entries()?;
    let first_summary = first[0].summary.as_ref().expect("summary");
    let first_usage = first_summary.token_usage.as_ref().expect("usage");
    assert_eq!(first_usage.input_tokens, 100);
    assert_eq!(first_usage.output_tokens, 50);
    assert_eq!(
        first_summary
            .models
            .iter()
            .map(|model| model.model.clone())
            .collect::<Vec<_>>(),
        vec!["model-x".to_string()]
    );

    // 跨毫秒确保 db mtime(行缓存 freshness)与行级 stamp 都能观察到变化。
    std::thread::sleep(std::time::Duration::from_millis(20));
    {
        let connection = Connection::open(&db_path)?;
        // 回写旧消息:tokens 变化伴随 time_updated 推进。
        connection.execute(
            "UPDATE session_message
             SET time_updated = time_updated + 1000,
                 data = json_set(data, '$.tokens.input', 300)
             WHERE id = 'msg-a1'",
            [],
        )?;
        // 追加新消息:不同模型,另一份 usage。
        super::seed_opencode_message_row(
            &connection,
            "ses_contrib",
            "msg-a2",
            1_744_366_403_000,
            3,
            "assistant",
            &json!({
                "time": { "created": 1_744_366_403_000_i64 },
                "model": { "providerID": "p1", "id": "model-y" },
                "tokens": { "input": 50, "output": 20, "reasoning": 0,
                            "cache": { "read": 0, "write": 0 } },
            }),
        )?;
        // 删除子会话的 assistant 消息:贡献随之消失。
        connection.execute(
            "DELETE FROM session_message WHERE session_id = 'ses_contrib_child' AND type = 'assistant'",
            [],
        )?;
    }

    // 增量扫描(共享 store 的贡献表 diff)与全新 store 的全量提取对照。
    let incremental = engine.list_entries()?;
    let fresh = opencode::engine_at(dir.clone(), dir.join("store_fresh"));
    let full = fresh.list_entries()?;

    assert_eq!(incremental.len(), full.len());
    let incremental_usage = incremental[0]
        .summary
        .as_ref()
        .expect("summary")
        .token_usage
        .as_ref()
        .expect("usage");
    let full_usage = full[0]
        .summary
        .as_ref()
        .expect("summary")
        .token_usage
        .as_ref()
        .expect("usage");
    assert_eq!(incremental_usage, full_usage);
    // 回写后的期望值:300 + 50,旧值不得重复累计。
    assert_eq!(incremental_usage.input_tokens, 350);

    let incremental_models: Vec<String> = incremental[0]
        .summary
        .as_ref()
        .expect("summary")
        .models
        .iter()
        .map(|model| model.model.clone())
        .collect();
    let full_models: Vec<String> = full[0]
        .summary
        .as_ref()
        .expect("summary")
        .models
        .iter()
        .map(|model| model.model.clone())
        .collect();
    assert_eq!(incremental_models, full_models);

    fs::remove_dir_all(&dir).ok();
    Ok(())
}

// 重定向后的会话根:读取器走 agents 清单的 GlobalRoot(重定向优先、HOME
// 兜底),与 workspace/providers 同一条路径来源——设了变量的用户在三个域
// 看到同一份数据。codex 侧的行为证据:CODEX_HOME 重定向时官方 CLI 把
// sessions/年/月/日 树建在重定向目录下(2026-10-12 实测);claude 侧见
// global-root-env-redirect ADR;dsh 侧本机无 CLI,实现按同型处理,
// sessions 是否随迁标 unverified。
#[test]
fn codex_reader_follows_codex_home_redirect() -> Result<()> {
    let home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    let redirect = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&redirect)?;
    let _guard = crate::test_support::TestEnvGuard::set_home(&home);
    unsafe { env::set_var("CODEX_HOME", &redirect) };

    // 会话种在重定向根下;旧实现读 HOME/.codex,列表会是空。
    let session_id = "44444444-4444-4444-8444-444444444444";
    let transcript = redirect
        .join("sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T12-00-00-{session_id}.jsonl"));
    super::write_jsonl(
        &transcript,
        &[json!({
            "timestamp": "2026-04-21T12:00:00.000Z",
            "type": "session_meta",
            "payload": { "id": session_id, "cwd": "/tmp/workspace" }
        })],
    )?;

    // 重定向变量即整个根:官方 CLI 实测把 sessions/年/月/日 树建在变量
    // 目录下,.codex 只是 HOME 兜底时的缺省名。
    assert_eq!(crate::session::codex::root()?, redirect);
    let entries = crate::session::reader(crate::session::model::SourceApp::Codex).list_entries()?;
    assert_eq!(entries.len(), 1, "CODEX_HOME 下的会话应被列出");
    assert_eq!(entries[0].source_session_id, session_id);

    fs::remove_dir_all(&home).ok();
    fs::remove_dir_all(&redirect).ok();
    Ok(())
}

#[test]
fn claude_reader_follows_claude_config_dir_redirect() -> Result<()> {
    let home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    let redirect = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&redirect)?;
    let _guard = crate::test_support::TestEnvGuard::set_home(&home);
    unsafe { env::set_var("CLAUDE_CONFIG_DIR", &redirect) };

    let transcript = redirect
        .join("projects/demo-project")
        .join("redirected-session.jsonl");
    super::write_jsonl(
        &transcript,
        &[json!({
            "timestamp": "2026-04-21T12:00:00.000Z",
            "sessionId": "redirected-session",
            "type": "user",
            "message": { "content": "Main task" }
        })],
    )?;

    let entries =
        crate::session::reader(crate::session::model::SourceApp::ClaudeCode).list_entries()?;
    assert_eq!(entries.len(), 1, "CLAUDE_CONFIG_DIR 下的会话应被列出");
    assert_eq!(entries[0].source_session_id, "redirected-session");

    fs::remove_dir_all(&home).ok();
    fs::remove_dir_all(&redirect).ok();
    Ok(())
}

#[test]
fn dsh_root_resolves_dsh_home_redirect() -> Result<()> {
    let home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    let redirect = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&redirect)?;
    let _guard = crate::test_support::TestEnvGuard::set_home(&home);
    unsafe { env::set_var("DSH_HOME", &redirect) };

    // 重定向变量即整个根,与 workspace/providers 的 DSH_HOME 语义同一份。
    // dsh 侧官方行为已实测(0.2.0-rc.2):DSH_HOME 重定向时 boot profile
    // 数秒内把 sessions/ 建在变量目录下,会话存储整体随迁。
    assert_eq!(crate::session::dsh::root()?, redirect);
    assert_eq!(
        crate::session::dsh::sessions_root()?,
        redirect.join("sessions")
    );

    fs::remove_dir_all(&home).ok();
    fs::remove_dir_all(&redirect).ok();
    Ok(())
}

#[test]
fn debug_redirect_roots() {
    let home = env::temp_dir().join("reins-debug-home");
    let redirect = env::temp_dir().join("reins-debug-redirect");
    fs::create_dir_all(&home).unwrap();
    let _guard = crate::test_support::TestEnvGuard::set_home(&home);
    unsafe { env::set_var("CODEX_HOME", &redirect) };
    unsafe { env::set_var("DSH_HOME", &redirect) };
    println!("codex root = {:?}", crate::session::codex::root());
    println!("dsh root = {:?}", crate::session::dsh::root());
    let env = crate::support::environment::Environment::from_env();
    println!("env.codex_home = {:?}", env.codex_home);
    println!("env.dsh_home = {:?}", env.dsh_home);
    unsafe { env::remove_var("CODEX_HOME") };
    unsafe { env::remove_var("DSH_HOME") };
}
