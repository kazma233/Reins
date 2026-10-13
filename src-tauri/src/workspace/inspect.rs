use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value as JsonValue;

use super::addressing::TargetRef;
use super::mcp_formats::mcp_format_writer;
use super::types::{
    AgentTargetId, McpConfigType, McpInspection, McpTargetInspection, McpTargetState,
    ProjectInspection, ResolvedManagerConfig, ResolvedMcpConfig, ResolvedTargetConfig,
    SkillDirLinkView, TargetInspection, WorkspaceInspection,
};
use super::{parse_manager_config, read_existing_mcp_entries};
use crate::support::environment::Environment;

type McpEntryCache = HashMap<
    (AgentTargetId, PathBuf, String, McpConfigType),
    std::result::Result<Arc<std::collections::BTreeMap<String, JsonValue>>, String>,
>;

pub(crate) fn inspect_app_state_document(
    raw_content: &str,
    config_path: &Path,
    env: &Environment,
) -> Result<WorkspaceInspection> {
    let config = parse_manager_config(raw_content, config_path, env)?;

    // 落点归属只查一次：两个视图（全局 target 与项目 agent）都从这张表取。
    let links = config.skill_dir_links()?;
    let link_of = |id: &str| {
        links
            .iter()
            .find(|(reference, _, _)| reference.composite_id() == id)
            .and_then(|(_, _, link)| link.link_target.as_ref().map(|target| (link, target)))
    };
    // 落点是软链接的 target 不参与分发（判定在寻址层），这里把「为什么不参与」
    // 一并说清楚：面板已按行标注，warnings 让启动与其它消费方也能看到。
    let warnings = links
        .iter()
        .filter(|(_, _, link)| link.is_alias())
        .map(|(reference, _, _)| {
            format!(
                "{} 的 skills 目录是软链接，内容由链接另一端负责，不参与技能分发。",
                reference.composite_id()
            )
        })
        .collect();

    let targets = config
        .targets
        .values()
        .map(|target| TargetInspection {
            id: target.id.clone(),
            skill_dir_exists: target.skill_dir.exists(),
            skill_dir_link: link_of(&target.id.0).map(|(link, target)| SkillDirLinkView {
                linked_target_id: link.linked_target_id.clone(),
                link_target: super::display_path(target),
            }),
        })
        .collect();

    let mut mcp_entry_cache = HashMap::new();
    let mcps = config
        .mcps
        .iter()
        .map(|mcp| inspect_mcp(mcp, &config, &mut mcp_entry_cache, env))
        .collect::<Result<Vec<_>>>()?;

    let projects = config
        .projects
        .values()
        .map(|project| ProjectInspection {
            id: project.id.clone(),
            agents: project
                .agents
                .values()
                .map(|target| {
                    let composite_id = TargetRef::Project {
                        project_id: project.id.clone(),
                        agent_id: target.id.clone(),
                    }
                    .composite_id();
                    TargetInspection {
                        id: target.id.clone(),
                        skill_dir_exists: target.skill_dir.exists(),
                        skill_dir_link: link_of(&composite_id).map(|(link, link_target)| {
                            SkillDirLinkView {
                                linked_target_id: link.linked_target_id.clone(),
                                link_target: super::display_path(link_target),
                            }
                        }),
                    }
                })
                .collect(),
        })
        .collect();

    Ok(WorkspaceInspection {
        config_path: super::display_path(config_path),
        targets,
        mcps,
        projects,
        warnings,
    })
}

fn inspect_mcp(
    mcp: &ResolvedMcpConfig,
    config: &ResolvedManagerConfig,
    mcp_entry_cache: &mut McpEntryCache,
    env: &Environment,
) -> Result<McpInspection> {
    // 一次遍历全部 target：复合 id 由寻址层给出，项目归属从 id 拆回来，
    // 本地不再拼串（此前这里与其它两处各拼一份 `project:agent`）。
    let mut targets = Vec::new();
    // 遍历一次全部 target：展示用的复合 id 与项目归属都由 TargetRef 给出，
    // 这里不再自己拼 `project:agent`。
    for (reference, target) in config.target_refs() {
        targets.push(inspect_mcp_target(
            mcp,
            target,
            &reference,
            mcp_entry_cache,
            env,
        )?);
    }

    Ok(McpInspection {
        name: mcp.name.clone(),
        targets,
    })
}

fn inspect_mcp_target(
    mcp: &ResolvedMcpConfig,
    target: &ResolvedTargetConfig,
    reference: &TargetRef,
    mcp_entry_cache: &mut McpEntryCache,
    env: &Environment,
) -> Result<McpTargetInspection> {
    let config_path = target.config_path.as_ref();

    // 落点状态只有「有没有找到同名条目」两态：agent 与 MCP 都没有启停开关，
    // 配置在盘上就是生效的。
    let (state, detail) = if let Some(config_path) = config_path {
        match read_existing_mcp_entries_cached(mcp_entry_cache, target, config_path, env) {
            Ok(existing) => {
                // 条目 key 的归类由各 config type 的 writer 声明（dsh 为清洗后
                // 的 serverName，其余与 Reins 的 mcp name 一致）。
                let lookup_name =
                    mcp_format_writer(target.mcp_config_type).entry_key_for(&mcp.name);
                let state = if existing.contains_key(&lookup_name) {
                    McpTargetState::Present
                } else {
                    McpTargetState::Missing
                };
                let detail = match state {
                    McpTargetState::Present => "目标配置里已经存在同名条目。".to_string(),
                    _ => "目标配置里还没有同名条目。".to_string(),
                };
                (state, detail)
            }
            Err(error) => (McpTargetState::Error, format!("读取目标配置失败：{error}")),
        }
    } else {
        (
            McpTargetState::Unconfigured,
            "当前 target 还没有 MCP 配置文件路径。".to_string(),
        )
    };

    Ok(McpTargetInspection {
        target_id: AgentTargetId(reference.composite_id()),
        project_id: match reference {
            TargetRef::Global(_) => None,
            TargetRef::Project { project_id, .. } => Some(project_id.clone()),
        },
        state,
        detail,
    })
}

fn read_existing_mcp_entries_cached(
    cache: &mut McpEntryCache,
    target: &ResolvedTargetConfig,
    config_path: &Path,
    env: &Environment,
) -> Result<Arc<std::collections::BTreeMap<String, JsonValue>>> {
    let key = (
        target.id.clone(),
        config_path.to_path_buf(),
        target.mcp_config_prefix.clone(),
        target.mcp_config_type,
    );

    if let Some(cached) = cache.get(&key) {
        return cached.clone().map_err(anyhow::Error::msg);
    }

    let result = read_existing_mcp_entries(target, config_path, env)
        .map(Arc::new)
        .map_err(|error| error.to_string());
    cache.insert(key, result.clone());
    result.map_err(anyhow::Error::msg)
}
