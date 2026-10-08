use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
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
