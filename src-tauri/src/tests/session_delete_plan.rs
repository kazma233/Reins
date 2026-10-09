use super::grokbuild::{fixture as grok_fixture, history as grok_history, subagent_fixture};
use super::*;
use crate::session::model::DeletePlanAction;

// 删除预演是前端确认框的新契约:动作序列(含顺序)必须与迁移前的
// deleteCommandPreview 输出一致,这里逐来源钉住。

#[test]
fn claude_code_delete_plan_lists_members_meta_then_sidecars() -> Result<()> {
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

    let plan = session::delete::get_delete_plan_inner(SourceApp::ClaudeCode, root_id)?;

    assert!(plan.supported);
    assert_eq!(
        plan.actions,
        vec![
            DeletePlanAction::RemoveFile {
                path: root_path.display().to_string()
            },
            DeletePlanAction::RemoveFile {
                path: child_path.display().to_string()
            },
            DeletePlanAction::RemoveFile {
                path: child_path.with_extension("meta.json").display().to_string()
            },
            DeletePlanAction::RemoveDirectory {
                path: format!("~/.claude/projects/{root_id}")
            },
            DeletePlanAction::RemoveDirectory {
                path: format!("~/.claude/session-env/{root_id}")
            },
            DeletePlanAction::RemoveDirectory {
                path: format!("~/.claude/file-history/{root_id}")
            },
        ]
    );

    Ok(())
}

#[test]
fn codex_delete_plan_runs_official_cli_per_session_id() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let root_id = "44444444-4444-4444-8444-444444444444";
    let child_id = "55555555-5555-4555-8555-555555555555";
    seed_codex_session(&temp_home, root_id)?;
    let child_path = temp_home
        .join(".codex/sessions/2026/04/21")
        .join(format!("rollout-2026-04-21T15-00-00-{child_id}.jsonl"));
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

    let plan = session::delete::get_delete_plan_inner(SourceApp::Codex, root_id)?;

    assert!(plan.supported);
    assert_eq!(
        plan.actions,
        vec![
            DeletePlanAction::RunCli {
                program: "codex".to_string(),
                args: vec![
                    "delete".to_string(),
                    "--force".to_string(),
                    root_id.to_string()
                ]
            },
            DeletePlanAction::RunCli {
                program: "codex".to_string(),
                args: vec![
                    "delete".to_string(),
                    "--force".to_string(),
                    child_id.to_string()
                ]
            },
        ]
    );

    Ok(())
}

#[test]
fn opencode_delete_plan_runs_official_cli_per_session_id() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let root_id = "ses_plan_root";
    let child_id = "ses_plan_child";
    seed_opencode_family(root_id, child_id)?;

    let plan = session::delete::get_delete_plan_inner(SourceApp::OpenCode, root_id)?;

    assert!(plan.supported);
    assert_eq!(
        plan.actions,
        vec![
            DeletePlanAction::RunCli {
                program: "opencode".to_string(),
                args: vec![
                    "session".to_string(),
                    "delete".to_string(),
                    root_id.to_string()
                ]
            },
            DeletePlanAction::RunCli {
                program: "opencode".to_string(),
                args: vec![
                    "session".to_string(),
                    "delete".to_string(),
                    child_id.to_string()
                ]
            },
        ]
    );

    Ok(())
}

#[test]
fn grokbuild_delete_plan_mixes_cli_dirs_and_sqlite() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);

    let parent_path = grok_fixture(&temp_home, "grok-parent")?;
    grok_history(&parent_path)?;
    subagent_fixture(
        &temp_home,
        "grok-parent",
        "grok-child",
        "at1.delete",
        "Child answer",
    )?;

    let plan = session::delete::get_delete_plan_inner(SourceApp::GrokBuild, "grok-parent")?;

    // sourcePaths 来自 canonicalize 后的索引扫描,期望值同样走 canonicalize。
    let child_dir =
        support::fs::canonicalize(&temp_home.join(".grok/sessions/not-a-cwd/grok-child"))?
            .display()
            .to_string();

    assert!(plan.supported);
    assert_eq!(
        plan.actions,
        vec![
            DeletePlanAction::RunCli {
                program: "grok".to_string(),
                args: vec![
                    "sessions".to_string(),
                    "delete".to_string(),
                    "grok-parent".to_string()
                ]
            },
            DeletePlanAction::RemoveDirectory { path: child_dir },
            DeletePlanAction::Sqlite {
                db_path: "~/.grok/sessions/session_search.sqlite".to_string(),
                sql: "DELETE FROM session_docs WHERE session_id IN ('grok-child');".to_string()
            },
        ]
    );

    Ok(())
}

#[test]
fn pi_delete_plan_removes_the_transcript_file() -> Result<()> {
    let temp_home = env::temp_dir().join(format!("reins-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_home)?;
    let _guard = TestEnvGuard::set_home(&temp_home);
    let selected = temp_home
        .join(".pi/agent/sessions/--tmp-pi-project--/2026-09-04T10-00-00-000Z_pi-plan.jsonl");
    write_jsonl(
        &selected,
        &[json!({
            "type": "session", "version": 3, "id": "pi-plan",
            "timestamp": "2026-09-04T10:00:00.000Z", "cwd": "/tmp/pi-project"
        })],
    )?;

    let plan = session::delete::get_delete_plan_inner(SourceApp::Pi, "pi-plan")?;

    assert!(plan.supported);
    assert_eq!(
        plan.actions,
        vec![DeletePlanAction::RemoveFile {
            path: selected.display().to_string()
        }]
    );

    Ok(())
}

// 不支持删除的来源不需要转录夹具:Unsupported 分支不解析 overview。
#[test]
fn unsupported_sources_plan_hides_delete_with_notice() -> Result<()> {
    let zcode_plan = session::delete::get_delete_plan_inner(SourceApp::Zcode, "unused")?;
    assert!(!zcode_plan.supported);
    assert_eq!(
        zcode_plan.reason.as_deref(),
        Some("暂不支持删除 ZCode 会话。")
    );
    assert_eq!(zcode_plan.description, "暂不支持删除 ZCode 会话。");
    assert!(zcode_plan.details.is_empty());
    assert_eq!(zcode_plan.command_label, "不支持删除");
    assert!(zcode_plan.actions.is_empty());

    let dsh_plan = session::delete::get_delete_plan_inner(SourceApp::Dsh, "unused")?;
    assert!(!dsh_plan.supported);
    assert_eq!(
        dsh_plan.reason.as_deref(),
        Some("暂不支持删除 DeepSeek Harness 会话。")
    );
    assert_eq!(dsh_plan.description, "暂不支持删除 DeepSeek Harness 会话。");
    assert!(dsh_plan.details.is_empty());
    assert_eq!(dsh_plan.command_label, "不支持删除");
    assert!(dsh_plan.actions.is_empty());

    Ok(())
}
