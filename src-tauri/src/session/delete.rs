use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::family_index::{Family, FamilyRow};
use crate::state::session_index::SessionIndexState;

use super::model::{DeletePlan, DeletePlanAction};
use super::sources::{self, DELETE_UNSUPPORTED_LABEL, DeletePolicy};
use super::{DeleteSessionResult, SourceApp};

// 官方 CLI 删除的统一执行机械:怎么跑、失败怎么把 exit code 与标准输出/错误
// 原样带回,只在这里说一次;用哪个二进制是各家知识,由调用方构造好传入。
// what 是命令的人类描述,进两段错误文案。
pub(crate) fn run_official_cli(what: &str, command: &mut Command) -> Result<()> {
    let output = command
        .output()
        .with_context(|| format!("Failed to execute {what}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let exit_code = output
        .status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "terminated by signal".to_string());
    bail!(
        "{what} failed. exit_code: {exit_code}\nstdout:\n{}\nstderr:\n{}",
        stdout.trim(),
        stderr.trim()
    )
}

// 从 start 沿父链清理删空后的目录,root 本身不清理:root 是来源的会话扫描
// 根(pi 的会话根可由用户显式配置,claude 的 projects/ 是 CLI 的目录骨架),
// 删掉会让下一次扫描落空。
pub(crate) fn prune_empty_parents(root: PathBuf, start: Option<&Path>) {
    let Some(mut current) = start.map(Path::to_path_buf) else {
        return;
    };

    while current.starts_with(&root) && current != root {
        let is_empty = fs::read_dir(&current)
            .ok()
            .and_then(|mut entries| entries.next())
            .is_none();

        if !is_empty || fs::remove_dir(&current).is_err() {
            break;
        }

        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
}

pub(crate) fn delete_session_inner(
    state: &SessionIndexState,
    source_app: SourceApp,
    source_session_id: &str,
) -> Result<DeleteSessionResult> {
    let overview = super::reader(source_app).parse_overview(source_session_id)?;

    super::delete_session(source_app, source_session_id)?;
    state.clear()?;
    // 重新扫盘即可:被删文件的目录指纹已变,索引会重建;持久层按路径自失效,
    // 清空它只会让下一次加载重新解析全部留存转录。
    super::rescan_all_caches()?;

    Ok(DeleteSessionResult {
        source_app,
        deleted_session_id: overview.summary.source_session_id,
        deleted_paths: overview.source_paths,
    })
}

// 删除预演的结构化输入:family 的非渲染折叠。plan 的成员级推导(id 清单、
// 成员文件/目录、root 排除)只认这里,不再从 overview 的展示名册与路径
// 字符串反推。
pub(crate) struct DeletePlanInput {
    /// family 根可被解析的 id(执行侧官方命令与 sidecar 路径都用它)。
    pub(crate) root_id: String,
    /// root 打头,成员序;root 行 is_root=true。
    pub(crate) members: Vec<PlanMember>,
}

pub(crate) struct PlanMember {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
    pub(crate) is_root: bool,
}

impl DeletePlanInput {
    /// 全部 Deleter 共用的唯一折叠:root 行打头,is_root 同时认行自身声明。
    /// 成员全集保留、不去重——同 id 的续跑段各有自己的文件,plan 的文件
    /// 清单不能丢路径;唯一例外是「声明为 root 且与 root 同 id」的行,那是
    /// claude 分组把 root 另存一份后残留在 members 里的重复,root 文件已由
    /// 打头的 root 行携带。
    pub(crate) fn from_family<Row: FamilyRow>(family: &Family<Row>) -> Self {
        let root_id = family.root.member_id().to_string();
        let mut members = Vec::with_capacity(family.members.len() + 1);
        members.push(PlanMember {
            id: root_id.clone(),
            path: family.root.member_path().as_ref().to_path_buf(),
            is_root: true,
        });
        for row in &family.members {
            if row.member_is_root() && row.member_id() == root_id {
                continue;
            }
            members.push(PlanMember {
                id: row.member_id().to_string(),
                path: row.member_path().as_ref().to_path_buf(),
                is_root: row.member_is_root(),
            });
        }
        Self { root_id, members }
    }

    /// 删除目标 id 的统一推导:全体成员 id 全局保序去重(root 先见)。
    /// 与展示名册 family_agents 的 Root 标签去重是同一条规则的两个消费方
    /// ——名册保留同 id 的回退条目用于展示,删除面只删一次。
    pub(crate) fn session_ids(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.members
            .iter()
            .map(|member| member.id.clone())
            .filter(|id| seen.insert(id.clone()))
            .collect()
    }
}

// 删除预演:动作清单与说明文案都取自来源注册表,前端只渲染。plan 的输入
// 是 family 的结构化折叠(delete_plan_input),不经渲染 DTO;不支持删除的
// 来源不解析 family(其 family 对预演没有价值,跳过更稳)。
pub(crate) fn get_delete_plan_inner(
    source_app: SourceApp,
    source_session_id: &str,
) -> Result<DeletePlan> {
    match sources::spec(source_app).delete {
        DeletePolicy::Deleter { plan, copy, .. } => {
            let input = super::reader(source_app).delete_plan_input(source_session_id)?;
            let actions = plan(&input)?;

            Ok(DeletePlan {
                source_app,
                supported: true,
                reason: None,
                description: copy.description.to_string(),
                details: copy
                    .details
                    .iter()
                    .map(|detail| detail.to_string())
                    .collect(),
                command_label: copy.command_label.to_string(),
                actions,
            })
        }
        DeletePolicy::Unsupported { notice, .. } => Ok(DeletePlan {
            source_app,
            supported: false,
            reason: Some(notice.to_string()),
            description: notice.to_string(),
            details: Vec::new(),
            command_label: DELETE_UNSUPPORTED_LABEL.to_string(),
            actions: Vec::<DeletePlanAction>::new(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // root 不被清理是这份实现存在的理由(pi 显式配置的会话根、claude 的
    // projects 目录骨架都靠它活),直测钉住。
    #[test]
    fn prune_empty_parents_stops_at_the_root() {
        let root = std::env::temp_dir().join(format!("reins-prune-{}", Uuid::new_v4()));
        let nested = root.join("a/b");
        fs::create_dir_all(&nested).unwrap();

        prune_empty_parents(root.clone(), Some(&nested));
        assert!(!nested.exists());
        assert!(!root.join("a").exists());
        assert!(root.exists(), "扫描根本身不能被 prune 掉");

        // start 就是 root:同样不动 root。
        prune_empty_parents(root.clone(), Some(&root));
        assert!(root.exists());

        fs::remove_dir_all(&root).ok();
    }

    struct FoldRow {
        id: &'static str,
        is_root: bool,
    }

    impl FamilyRow for FoldRow {
        fn member_path(&self) -> std::borrow::Cow<'_, Path> {
            std::borrow::Cow::Owned(PathBuf::from(format!("/x/{}.jsonl", self.id)))
        }

        fn family_root_id(&self) -> &str {
            self.id
        }

        fn member_id(&self) -> &str {
            self.id
        }

        fn member_created_at(&self) -> Option<i64> {
            None
        }

        fn member_updated_at(&self) -> Option<i64> {
            None
        }

        fn member_tie_breaker(&self) -> std::borrow::Cow<'_, str> {
            std::borrow::Cow::Borrowed(self.id)
        }

        fn member_is_root(&self) -> bool {
            self.is_root
        }
    }

    // 折叠契约:root 行打头且 is_root=true;成员全集保留(同 id 的续跑段
    // 各有自己的文件,plan 的文件清单不能丢路径),唯一例外是 claude 分组
    // 残留在 members 里的重复 root 行(声明 root 且同 id);session_ids
    // 全局保序去重。
    #[test]
    fn from_family_keeps_all_members_and_dedups_session_ids() {
        let family = Family {
            root: FoldRow {
                id: "root",
                is_root: true,
            },
            members: vec![
                // codex resume 段形态:同 id、不声明 root。
                FoldRow {
                    id: "root",
                    is_root: false,
                },
                // claude 分组残留形态:同 id、声明 root。
                FoldRow {
                    id: "root",
                    is_root: true,
                },
                FoldRow {
                    id: "child",
                    is_root: false,
                },
            ],
        };

        let input = DeletePlanInput::from_family(&family);

        assert_eq!(input.root_id, "root");
        assert_eq!(input.members.len(), 3);
        assert!(input.members[0].is_root);
        assert!(!input.members[1].is_root);
        assert_eq!(
            input
                .members
                .iter()
                .map(|member| member.path.display().to_string())
                .collect::<Vec<_>>(),
            vec!["/x/root.jsonl", "/x/root.jsonl", "/x/child.jsonl"]
        );
        assert_eq!(
            input.session_ids(),
            vec!["root".to_string(), "child".to_string()]
        );
    }
}
