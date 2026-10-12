use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::state::session_index::SessionIndexState;

use super::model::{DeletePlan, DeletePlanAction, SessionOverview};
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
    super::clear_all_caches()?;

    Ok(DeleteSessionResult {
        source_app,
        deleted_session_id: overview.summary.source_session_id,
        deleted_paths: overview.source_paths,
    })
}

// 删除目标 id 的统一推导:会话组有 agent 条目时逐个删,没有(单会话来源或
// 尚无 agent 视图)退回 root id;去重保持首见顺序。
pub(crate) fn delete_target_session_ids(overview: &SessionOverview) -> Vec<String> {
    let ids = if overview.agents.is_empty() {
        vec![overview.summary.source_session_id.clone()]
    } else {
        overview
            .agents
            .iter()
            .map(|agent| agent.session_id.clone())
            .collect()
    };

    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

// 删除预演:动作清单与说明文案都取自来源注册表,前端只渲染。不支持删除的
// 来源不解析 overview(其 overview 对预演没有价值,跳过更稳);可删来源的
// id 解析与 overview 解析失败沿 delete_session_inner 同样的错误传播。
pub(crate) fn get_delete_plan_inner(
    source_app: SourceApp,
    source_session_id: &str,
) -> Result<DeletePlan> {
    match sources::spec(source_app).delete {
        DeletePolicy::Deleter { plan, copy, .. } => {
            let overview = super::reader(source_app).parse_overview(source_session_id)?;
            let actions = plan(&overview)?;

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
}
