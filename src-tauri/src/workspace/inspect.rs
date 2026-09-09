use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value as JsonValue;

use super::{
    AgentTargetId, McpConfigType, McpInspection, McpTargetInspection, PathInspection,
    ProjectInspection, ResolvedManagerConfig, ResolvedMcpConfig, ResolvedTargetConfig,
    TargetInspection, WorkspaceInspection, parse_manager_config, read_existing_mcp_entries,
};

type McpEntryCache = HashMap<
    (AgentTargetId, PathBuf, String, McpConfigType),
    std::result::Result<Arc<std::collections::BTreeMap<String, JsonValue>>, String>,
>;

pub(crate) fn inspect_app_state_document(
    raw_content: &str,
    config_path: &Path,
) -> Result<WorkspaceInspection> {
    let config = parse_manager_config(raw_content, config_path)?;
    let warnings = Vec::new();

    let targets = config
        .targets
        .values()
        .map(|target| TargetInspection {
            id: target.id.clone(),
            enabled: target.enabled,
            skill_dir: PathInspection {
                path: super::display_path(&target.skill_dir),
                exists: target.skill_dir.exists(),
            },
            config_path: target.config_path.as_ref().map(|path| PathInspection {
                path: super::display_path(path),
                exists: path.exists(),
            }),
        })
        .collect();

    let mut mcp_entry_cache = HashMap::new();
    let mcps = config
        .mcps
        .iter()
        .map(|mcp| inspect_mcp(mcp, &config, &mut mcp_entry_cache))
        .collect::<Result<Vec<_>>>()?;

    let projects = config
        .projects
        .values()
        .map(|project| ProjectInspection {
            id: project.id.clone(),
            path: PathInspection {
                path: super::display_path(&project.project_path),
                exists: project.project_path.exists(),
            },
            agents: project
                .agents
                .values()
                .map(|target| TargetInspection {
                    id: target.id.clone(),
                    enabled: target.enabled,
                    skill_dir: PathInspection {
                        path: super::display_path(&target.skill_dir),
                        exists: target.skill_dir.exists(),
                    },
                    config_path: target.config_path.as_ref().map(|path| PathInspection {
                        path: super::display_path(path),
                        exists: path.exists(),
                    }),
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
) -> Result<McpInspection> {
    let mut targets = Vec::new();

    for target in config.targets.values() {
        targets.push(inspect_mcp_target(
            mcp,
            target,
            None,
            None,
            mcp_entry_cache,
        )?);
    }

    for project in config.projects.values() {
        for (agent_id, target) in &project.agents {
            let composite_id = AgentTargetId(format!("{}:{}", project.id, agent_id));
            targets.push(inspect_mcp_target(
                mcp,
                target,
                Some(composite_id),
                Some(&project.id),
                mcp_entry_cache,
            )?);
        }
    }

    Ok(McpInspection {
        name: mcp.name.clone(),
        enabled: mcp.enabled,
        targets,
    })
}

fn inspect_mcp_target(
    mcp: &ResolvedMcpConfig,
    target: &ResolvedTargetConfig,
    composite_id: Option<AgentTargetId>,
    project_id: Option<&str>,
    mcp_entry_cache: &mut McpEntryCache,
) -> Result<McpTargetInspection> {
    let config_path = target.config_path.as_ref();

    let (state, detail, config_exists) = if !target.enabled {
        (
            "disabled".to_string(),
            "当前 target 已禁用。".to_string(),
            config_path.is_some_and(|path| path.exists()),
        )
    } else if let Some(config_path) = config_path {
        match read_existing_mcp_entries_cached(mcp_entry_cache, target, config_path) {
            Ok(existing) => {
                let state = if !mcp.enabled {
                    "disabled".to_string()
                } else if existing.contains_key(&mcp.name) {
                    "present".to_string()
                } else {
                    "missing".to_string()
                };
                let detail = match state.as_str() {
                    "disabled" => "当前 yaml 中该 MCP 已禁用。".to_string(),
                    "present" => "目标配置里已经存在同名条目。".to_string(),
                    _ => "目标配置里还没有同名条目。".to_string(),
                };
                (state, detail, config_path.exists())
            }
            Err(error) => (
                "error".to_string(),
                format!("读取目标配置失败：{error}"),
                config_path.exists(),
            ),
        }
    } else {
        (
            "unconfigured".to_string(),
            "当前 target 还没有 MCP 配置文件路径。".to_string(),
            false,
        )
    };

    Ok(McpTargetInspection {
        target_id: composite_id.unwrap_or_else(|| target.id.clone()),
        project_id: project_id.map(|s| s.to_string()),
        config_path: config_path.map(|path| super::display_path(path)),
        config_exists,
        state,
        detail,
    })
}

fn read_existing_mcp_entries_cached(
    cache: &mut McpEntryCache,
    target: &ResolvedTargetConfig,
    config_path: &Path,
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

    let result = read_existing_mcp_entries(target, config_path)
        .map(Arc::new)
        .map_err(|error| error.to_string());
    cache.insert(key, result.clone());
    result.map_err(anyhow::Error::msg)
}
