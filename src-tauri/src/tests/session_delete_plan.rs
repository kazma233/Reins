use super::*;
use crate::session::delete::{DeletePlanInput, PlanMember};
use crate::session::model::DeletePlanAction;

// 删除预演是前端确认框的契约:动作序列(含顺序)逐来源钉住。plan 是
// DeletePlanInput 上的纯推导,直接手构输入直测;family → input 的折叠接缝
// 由文末的端到端用例(真后端 → get_delete_plan_inner)与 delete.rs 内的
// from_family 单测共同覆盖。

fn input(root_id: &str, members: Vec<PlanMember>) -> DeletePlanInput {
    DeletePlanInput {
        root_id: root_id.to_string(),
        members,
    }
}

fn member(id: &str, path: &str, is_root: bool) -> PlanMember {
    PlanMember {
        id: id.to_string(),
        path: PathBuf::from(path),
        is_root,
    }
}

#[test]
fn claude_code_delete_plan_lists_members_meta_then_sidecars() {
    let input = input(
        "root-session",
        vec![
            member(
                "root-session",
                "/home/u/.claude/projects/demo-project/root-session.jsonl",
                true,
            ),
            member(
                "child-session",
                "/home/u/.claude/projects/demo-project/subagents/agent-child-session.jsonl",
                false,
            ),
        ],
    );

    let actions = session::claude_code::delete_plan(&input).unwrap();

    assert_eq!(
        actions,
        vec![
            DeletePlanAction::RemoveFile {
                path: "/home/u/.claude/projects/demo-project/root-session.jsonl".to_string()
            },
            DeletePlanAction::RemoveFile {
                path: "/home/u/.claude/projects/demo-project/subagents/agent-child-session.jsonl"
                    .to_string()
            },
            // 非 root 成员的 .meta.json 与执行侧同口径(所有非 root 成员),
            // 预演按完整意图列出。
            DeletePlanAction::RemoveFile {
                path:
                    "/home/u/.claude/projects/demo-project/subagents/agent-child-session.meta.json"
                        .to_string()
            },
            DeletePlanAction::RemoveDirectory {
                path: "~/.claude/projects/root-session".to_string()
            },
            DeletePlanAction::RemoveDirectory {
                path: "~/.claude/session-env/root-session".to_string()
            },
            DeletePlanAction::RemoveDirectory {
                path: "~/.claude/file-history/root-session".to_string()
            },
        ]
    );
}

#[test]
fn codex_delete_plan_runs_official_cli_per_session_id() {
    let input = input(
        "44444444-4444-4444-8444-444444444444",
        vec![
            member(
                "44444444-4444-4444-8444-444444444444",
                "/x/root.jsonl",
                true,
            ),
            member(
                "55555555-5555-4555-8555-555555555555",
                "/x/child.jsonl",
                false,
            ),
            // resume 段与原始段同 id:id 面只删一次。
            member(
                "44444444-4444-4444-8444-444444444444",
                "/x/root-resume.jsonl",
                false,
            ),
        ],
    );

    let actions = session::codex::delete_plan(&input).unwrap();

    assert_eq!(
        actions,
        vec![
            DeletePlanAction::RunCli {
                program: "codex".to_string(),
                args: vec![
                    "delete".to_string(),
                    "--force".to_string(),
                    "44444444-4444-4444-8444-444444444444".to_string()
                ]
            },
            DeletePlanAction::RunCli {
                program: "codex".to_string(),
                args: vec![
                    "delete".to_string(),
                    "--force".to_string(),
                    "55555555-5555-4555-8555-555555555555".to_string()
                ]
            },
        ]
    );
}

#[test]
fn opencode_delete_plan_runs_official_cli_per_session_id() {
    let input = input(
        "ses_plan_root",
        vec![
            member("ses_plan_root", "/x/root", true),
            member("ses_plan_child", "/x/child", false),
        ],
    );

    let actions = session::opencode::delete_plan(&input).unwrap();

    assert_eq!(
        actions,
        vec![
            DeletePlanAction::RunCli {
                program: "opencode".to_string(),
                args: vec![
                    "session".to_string(),
                    "delete".to_string(),
                    "ses_plan_root".to_string()
                ]
            },
            DeletePlanAction::RunCli {
                program: "opencode".to_string(),
                args: vec![
                    "session".to_string(),
                    "delete".to_string(),
                    "ses_plan_child".to_string()
                ]
            },
        ]
    );
}

#[test]
fn grokbuild_delete_plan_mixes_cli_dirs_and_sqlite() {
    let input = input(
        "grok-parent",
        vec![
            member(
                "grok-parent",
                "/h/.grok/sessions/not-a-cwd/grok-parent/summary.json",
                true,
            ),
            member(
                "grok-child",
                "/h/.grok/sessions/not-a-cwd/grok-child/summary.json",
                false,
            ),
        ],
    );

    let actions = session::grokbuild::delete_plan(&input).unwrap();

    assert_eq!(
        actions,
        vec![
            DeletePlanAction::RunCli {
                program: "grok".to_string(),
                args: vec![
                    "sessions".to_string(),
                    "delete".to_string(),
                    "grok-parent".to_string()
                ]
            },
            // root 自己的目录由官方命令负责,不进动作清单。
            DeletePlanAction::RemoveDirectory {
                path: "/h/.grok/sessions/not-a-cwd/grok-child".to_string()
            },
            DeletePlanAction::Sqlite {
                db_path: "~/.grok/sessions/session_search.sqlite".to_string(),
                sql: "DELETE FROM session_docs WHERE session_id IN ('grok-child');".to_string()
            },
        ]
    );
}

#[test]
fn pi_delete_plan_removes_the_transcript_file() {
    let input = input(
        "pi-plan",
        vec![member(
            "pi-plan",
            "/h/.pi/agent/sessions/--tmp-pi-project--/2026-09-04T10-00-00-000Z_pi-plan.jsonl",
            true,
        )],
    );

    let actions = session::pi::delete_plan(&input).unwrap();

    assert_eq!(
        actions,
        vec![DeletePlanAction::RemoveFile {
            path: "/h/.pi/agent/sessions/--tmp-pi-project--/2026-09-04T10-00-00-000Z_pi-plan.jsonl"
                .to_string()
        }]
    );
}

// 端到端接缝:真后端(引擎 family → delete_plan_input 折叠)→ plan。夹具
// 的期望路径随构造方式拼写,Windows 上是反斜杠,比较前统一分隔符。
fn display(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn normalized(actions: Vec<DeletePlanAction>) -> Vec<DeletePlanAction> {
    actions
        .into_iter()
        .map(|action| match action {
            DeletePlanAction::RemoveFile { path } => DeletePlanAction::RemoveFile {
                path: path.replace('\\', "/"),
            },
            DeletePlanAction::RemoveDirectory { path } => DeletePlanAction::RemoveDirectory {
                path: path.replace('\\', "/"),
            },
            other => other,
        })
        .collect()
}

#[test]
fn claude_delete_plan_folds_real_family_and_lists_members_meta_sidecars() -> Result<()> {
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
        normalized(plan.actions),
        vec![
            DeletePlanAction::RemoveFile {
                path: display(&root_path)
            },
            DeletePlanAction::RemoveFile {
                path: display(&child_path)
            },
            DeletePlanAction::RemoveFile {
                path: display(&child_path.with_extension("meta.json"))
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

// 不支持删除的来源不需要转录夹具:Unsupported 分支不解析 family。
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
