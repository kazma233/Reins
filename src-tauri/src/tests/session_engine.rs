use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::session::{SessionReader, codex};

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

    let summary = engine.parse_summary(&transcript)?;
    assert_eq!(summary.source_session_id, session_id);

    let page = engine.parse_messages_page(&transcript, 0, 10)?;
    assert_eq!(page.total_count, 1);

    let resolved = engine.resolve_path(session_id)?;
    assert_eq!(resolved, transcript);

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

    let engine = crate::session::claude_code::engine_at(dir.clone(), dir.join("store"));

    let entries = engine.list_entries()?;
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].summary.as_ref().expect("summary").title,
        "How do I list the current files?"
    );

    let summary = engine.parse_summary(&transcript)?;
    assert_eq!(summary.source_session_id, session_id);

    let resolved = engine.resolve_path(session_id)?;
    assert_eq!(resolved, transcript);

    fs::remove_dir_all(&dir).ok();
    Ok(())
}
