use super::*;
use crate::session::SessionReader;

// engine_at 的持久缓存目录：每个测试独立目录，避免跨测试串扰。
fn temp_store() -> PathBuf {
    env::temp_dir().join(format!("reins-store-{}", Uuid::new_v4()))
}

// The family index must apply the dual-write rule: the family key (the
// root's source id) resolves to the root transcript, and each member's own
// id resolves into the same family. Guards the or_insert (first-claim-wins)
// semantics shared by claude_code and codex ahead of the engine extraction.
// Path-level detail is no longer addressable through the reader interface,
// so resolution is asserted via the summary's transcript path (family root).

#[test]
fn claude_resolves_family_key_to_root_and_member_id_to_member() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-claude-{}", Uuid::new_v4()));
    let root_id = "root-session";
    let child_id = "child-session";
    let project_dir = root.join("projects").join("demo-project");
    let root_path = project_dir.join(format!("{root_id}.jsonl"));
    let child_path = project_dir
        .join("subagents")
        .join(format!("agent-{child_id}.jsonl"));

    // sessionId inside the subagent transcript stays the root id — that is
    // what groups the family; the child's own id comes from its filename.
    // Without first-claim-wins the later member would steal the family key.
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

    let reader = session::claude_code::engine_at(root, temp_store());
    assert_eq!(
        reader.parse_summary(root_id)?.transcript_path,
        root_path.display().to_string()
    );
    // 子会话按自身 id 直查仍归属同一 family(root 转录)。
    assert_eq!(
        reader.parse_summary(child_id)?.transcript_path,
        root_path.display().to_string()
    );

    Ok(())
}

#[test]
fn codex_resolves_family_key_to_root_and_member_id_to_member() -> Result<()> {
    let root = env::temp_dir().join(format!("reins-codex-{}", Uuid::new_v4()));
    let root_id = "44444444-4444-4444-8444-444444444444";
    let child_id = "55555555-5555-4555-8555-555555555555";
    let root_path = root
        .join("sessions")
        .join("2026")
        .join("04")
        .join("21")
        .join(format!("rollout-2026-04-21T12-00-00-{root_id}.jsonl"));
    let child_path = root
        .join("sessions")
        .join("2026")
        .join("04")
        .join("21")
        .join(format!("rollout-2026-04-21T15-00-00-{child_id}.jsonl"));

    write_jsonl(
        &root_path,
        &[json!({
            "timestamp": "2026-04-21T12:00:00.000Z",
            "type": "session_meta",
            "payload": { "id": root_id, "cwd": "/tmp/reins/workspace" }
        })],
    )?;
    write_jsonl(
        &child_path,
        &[json!({
            "timestamp": "2026-04-21T15:00:00.000Z",
            "type": "session_meta",
            "payload": {
                "id": child_id,
                "cwd": "/tmp/reins/workspace",
                "source": {
                    "subagent": {
                        "thread_spawn": { "parent_thread_id": root_id }
                    }
                }
            }
        })],
    )?;

    let reader = session::codex::engine_at(root, temp_store());
    assert_eq!(
        reader.parse_summary(root_id)?.transcript_path,
        root_path.display().to_string()
    );
    assert_eq!(
        reader.parse_summary(child_id)?.transcript_path,
        root_path.display().to_string()
    );

    Ok(())
}
