use std::collections::HashSet;

use anyhow::Result;

use crate::state::session_index::SessionIndexState;

use super::model::{DeletePlan, DeletePlanAction, SessionOverview};
use super::sources::{self, DELETE_UNSUPPORTED_LABEL, DeletePolicy};
use super::{DeleteSessionResult, SourceApp};

pub(crate) fn delete_session_inner(
    state: &SessionIndexState,
    source_app: SourceApp,
    source_session_id: &str,
    transcript_path: Option<&str>,
) -> Result<DeleteSessionResult> {
    let path = if let Some(path) = transcript_path {
        std::path::PathBuf::from(path)
    } else {
        super::reader(source_app).resolve_path(source_session_id)?
    };
    let overview = super::reader(source_app).parse_overview(&path)?;

    super::delete_session(source_app, &path)?;
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
// 路径解析与 overview 解析失败沿 delete_session_inner 同样的错误传播。
pub(crate) fn get_delete_plan_inner(
    source_app: SourceApp,
    source_session_id: &str,
    transcript_path: Option<&str>,
) -> Result<DeletePlan> {
    match sources::spec(source_app).delete {
        DeletePolicy::Deleter { plan, copy, .. } => {
            let path = if let Some(path) = transcript_path {
                std::path::PathBuf::from(path)
            } else {
                super::reader(source_app).resolve_path(source_session_id)?
            };
            let overview = super::reader(source_app).parse_overview(&path)?;
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
