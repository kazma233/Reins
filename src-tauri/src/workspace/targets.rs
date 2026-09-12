// Target/project domain: config.yaml CRUD for targets and projects, the
// builtin per-agent path defaults, and directory-link skill_dir detection.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

use anyhow::{Context, Result, anyhow, bail};
use dirs::home_dir;

use super::WorkspaceConfigStore;
use super::display_path;
use super::normalize_target_id;
use super::types::*;

// ---------------------------------------------------------------------------
// Directory-link skill_dir detection (project targets linked to another agent)
// ---------------------------------------------------------------------------

pub(super) fn is_linked_skill_dir(path: &Path) -> Result<bool> {
    Ok(read_skill_dir_link(path)?.is_some())
}

pub(super) fn read_skill_dir_link(path: &Path) -> Result<Option<PathBuf>> {
    read_directory_link_target(path)
}

pub(super) fn read_directory_link_target(path: &Path) -> Result<Option<PathBuf>> {
    match path.symlink_metadata() {
        Ok(meta) if meta.file_type().is_symlink() => {
            let raw = fs::read_link(path)
                .with_context(|| format!("读取 {} 的软链接失败", path.display()))?;
            Ok(Some(resolve_symlink_target_path(path, &raw)))
        }
        Ok(meta) => read_platform_directory_link_target(path, &meta),
        // Windows 的断链 junction 仍可能保留 reparse point，但
        // symlink_metadata 会报 NotFound；直接读 reparse 数据可保留原目标路径。
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(windows)]
            {
                read_windows_junction_target(path)
            }
            #[cfg(not(windows))]
            {
                Ok(None)
            }
        }
        Err(err) => Err(anyhow!("读取 {} 的元数据失败：{err}", path.display())),
    }
}

#[cfg(windows)]
fn read_platform_directory_link_target(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<Option<PathBuf>> {
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        return Ok(None);
    }

    read_windows_junction_target(path)
}

#[cfg(windows)]
fn read_windows_junction_target(path: &Path) -> Result<Option<PathBuf>> {
    match junction::get_target(path) {
        Ok(target) => {
            let normalized = target.canonicalize().unwrap_or(target);
            Ok(Some(normalized))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) if error.to_string().contains("not a reparse tag mount point") => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to read junction target {}", path.display()))
        }
    }
}

#[cfg(not(windows))]
fn read_platform_directory_link_target(
    _path: &Path,
    _metadata: &std::fs::Metadata,
) -> Result<Option<PathBuf>> {
    Ok(None)
}

pub(super) fn resolve_symlink_target_path(destination: &Path, link_target: &Path) -> PathBuf {
    let resolved = if link_target.is_absolute() {
        link_target.to_path_buf()
    } else {
        destination
            .parent()
            .map(|parent| parent.join(link_target))
            .unwrap_or_else(|| link_target.to_path_buf())
    };

    resolved.canonicalize().unwrap_or(resolved)
}

pub(crate) fn create_workspace_target_inner(
    store: &WorkspaceConfigStore,
    input: RawTargetInput,
) -> Result<WorkspaceTargetMutationResult> {
    store.locked(|config| {
        let mut raw_config = config.parse_raw()?;
        let (target_id, target_config) = normalize_raw_target_input(input, config.config_path())?;

        if raw_config.targets.contains_key(target_id.as_str()) {
            bail!("target 已存在：{}", target_id);
        }

        raw_config
            .targets
            .insert(target_id.to_string(), target_config);

        config.write_raw(&raw_config)?;

        Ok(WorkspaceTargetMutationResult {
            target_id,
            updated_paths: vec![display_path(config.config_path())],
        })
    })
}

pub(crate) fn update_workspace_target_inner(
    store: &WorkspaceConfigStore,
    current_target_id: &str,
    input: RawTargetInput,
) -> Result<WorkspaceTargetMutationResult> {
    let current_target_id = normalize_target_id(current_target_id)?;

    store.locked(|config| {
        let mut raw_config = config.parse_raw()?;

        if !raw_config.targets.contains_key(current_target_id.as_str()) {
            bail!("target 不存在：{}", current_target_id);
        }

        let (next_target_id, next_target) =
            normalize_raw_target_input(input, config.config_path())?;

        if next_target_id != current_target_id
            && raw_config.targets.contains_key(next_target_id.as_str())
        {
            bail!("target 已存在：{}", next_target_id);
        }

        raw_config.targets.remove(current_target_id.as_str());
        raw_config
            .targets
            .insert(next_target_id.to_string(), next_target);

        config.write_raw(&raw_config)?;

        Ok(WorkspaceTargetMutationResult {
            target_id: next_target_id,
            updated_paths: vec![display_path(config.config_path())],
        })
    })
}

pub(crate) fn delete_workspace_target_inner(
    store: &WorkspaceConfigStore,
    target_id: &str,
) -> Result<WorkspaceTargetMutationResult> {
    let target_id = normalize_target_id(target_id)?;

    store.locked(|config| {
        let mut raw_config = config.parse_raw()?;

        if raw_config.targets.remove(target_id.as_str()).is_none() {
            bail!("target 不存在：{}", target_id);
        }

        config.write_raw(&raw_config)?;

        Ok(WorkspaceTargetMutationResult {
            target_id,
            updated_paths: vec![display_path(config.config_path())],
        })
    })
}

pub(crate) fn create_workspace_project_inner(
    store: &WorkspaceConfigStore,
    input: ProjectMutationInput,
) -> Result<WorkspaceTargetMutationResult> {
    store.locked(|config| {
        let mut raw_config = config.parse_raw()?;

        let project_id = input.project_id.trim().to_string();
        if project_id.is_empty() {
            bail!("project id 不能为空。");
        }
        if raw_config.projects.contains_key(&project_id) {
            bail!("project id 已存在：{}", project_id);
        }
        let mut agents = BTreeMap::new();
        for agent_name in &input.agents {
            let id = normalize_target_id(agent_name)?;
            agents.insert(id.0, RawProjectAgentConfig { enabled: true });
        }
        raw_config.projects.insert(
            project_id.clone(),
            RawProjectConfig {
                path: input.path,
                agents,
            },
        );

        config.write_raw(&raw_config)?;

        Ok(WorkspaceTargetMutationResult {
            target_id: AgentTargetId(project_id),
            updated_paths: vec![display_path(config.config_path())],
        })
    })
}

pub(crate) fn update_workspace_project_inner(
    store: &WorkspaceConfigStore,
    input: ProjectMutationInput,
) -> Result<WorkspaceTargetMutationResult> {
    store.locked(|config| {
        let mut raw_config = config.parse_raw()?;

        let original_id = input.project_id.trim().to_string();
        if !raw_config.projects.contains_key(&original_id) {
            bail!("project 不存在：{}", original_id);
        }
        let mut agents = BTreeMap::new();
        for agent_name in &input.agents {
            let id = normalize_target_id(agent_name)?;
            agents.insert(id.0, RawProjectAgentConfig { enabled: true });
        }
        raw_config.projects.insert(
            original_id.clone(),
            RawProjectConfig {
                path: input.path,
                agents,
            },
        );

        config.write_raw(&raw_config)?;

        Ok(WorkspaceTargetMutationResult {
            target_id: AgentTargetId(original_id),
            updated_paths: vec![display_path(config.config_path())],
        })
    })
}

pub(crate) fn delete_workspace_project_inner(
    store: &WorkspaceConfigStore,
    project_id: &str,
) -> Result<WorkspaceTargetMutationResult> {
    store.locked(|config| {
        let mut raw_config = config.parse_raw()?;

        let project_id = project_id.trim().to_string();
        if raw_config.projects.remove(&project_id).is_none() {
            bail!("project 不存在：{}", project_id);
        }

        config.write_raw(&raw_config)?;

        Ok(WorkspaceTargetMutationResult {
            target_id: AgentTargetId(project_id),
            updated_paths: vec![config.config_path().display().to_string()],
        })
    })
}

fn normalize_raw_target_input(
    input: RawTargetInput,
    _config_path: &Path,
) -> Result<(AgentTargetId, RawTargetConfig)> {
    let target_id = normalize_target_id(&input.target_id)?;
    let skill_dir = normalize_target_skill_dir(&input.skill_dir)?;

    let normalized_config_path = input
        .config_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let config_prefix = input.mcp_config_prefix.trim().to_string();

    // MCP 配置文件和 configPrefix 必须成对出现：只有前缀没有路径无处可写，
    // 只有路径没有前缀无法定位写入节点。pi 这类不主动支持 MCP 的 target
    // 允许两者都为空，此时只做 skill 分发。
    match normalized_config_path.as_deref() {
        Some(_) if config_prefix.is_empty() => {
            bail!("target {} 的 MCP configPrefix 不能为空。", target_id);
        }
        None if !config_prefix.is_empty() => {
            bail!("target {} 填写了 configPrefix，必须同时提供 MCP 配置文件路径。", target_id);
        }
        _ => {}
    }

    Ok((
        target_id,
        RawTargetConfig {
            enabled: input.enabled,
            skill_dir,
            mcp: RawTargetMcpConfig {
                config_path: normalized_config_path,
                config_prefix: (!config_prefix.is_empty()).then_some(config_prefix),
                config_type: Some(input.mcp_config_type),
            },
        },
    ))
}

fn normalize_target_skill_dir(value: &str) -> Result<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        bail!("target skills 目录不能为空。");
    }

    Ok(trimmed.to_string())
}

// ---------------------------------------------------------------------------
// Target defaults (codex/claude/opencode builtin paths)
// ---------------------------------------------------------------------------

pub(super) struct TargetDefaults {
    pub(super) id: AgentTargetId,
    pub(super) skill_dir: PathBuf,
    pub(super) config_path: Option<PathBuf>,
    pub(super) config_prefix: &'static str,
    pub(super) config_type: McpConfigType,
}

fn builtin_target_defaults() -> Vec<TargetDefaults> {
    vec![
        TargetDefaults {
            id: AgentTargetId("codex".to_string()),
            skill_dir: home_dir()
                .map(|h| h.join(".agents/skills"))
                .unwrap_or_default(),
            config_path: home_dir().map(|h| h.join(".codex/config.toml")),
            config_prefix: "mcp_servers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("claude".to_string()),
            skill_dir: home_dir()
                .map(|h| h.join(".claude/skills"))
                .unwrap_or_default(),
            config_path: home_dir().map(|h| h.join(".claude.json")),
            config_prefix: "mcpServers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("opencode".to_string()),
            skill_dir: home_dir()
                .map(|h| h.join(".config/opencode/skills"))
                .unwrap_or_default(),
            config_path: home_dir().map(|h| h.join(".config/opencode/opencode.json")),
            config_prefix: "mcp",
            config_type: McpConfigType::OpenCode,
        },
        TargetDefaults {
            id: AgentTargetId("zcode".to_string()),
            skill_dir: home_dir()
                .map(|h| h.join(".zcode/skills"))
                .unwrap_or_default(),
            config_path: home_dir().map(|h| h.join(".zcode/cli/config.json")),
            config_prefix: "mcp.servers",
            config_type: McpConfigType::Common,
        },
        // pi 不主动支持 MCP：默认只分发 skill，不写任何 MCP 配置文件。
        TargetDefaults {
            id: AgentTargetId("pi".to_string()),
            skill_dir: home_dir()
                .map(|h| h.join(".pi/agent/skills"))
                .unwrap_or_default(),
            config_path: None,
            config_prefix: "",
            config_type: McpConfigType::Common,
        },
    ]
}

pub(super) fn builtin_target_defaults_map() -> HashMap<AgentTargetId, TargetDefaults> {
    builtin_target_defaults()
        .into_iter()
        .map(|item| (item.id.clone(), item))
        .collect()
}

pub(super) fn project_agent_defaults(project_path: &Path) -> Vec<TargetDefaults> {
    vec![
        TargetDefaults {
            id: AgentTargetId("claude".to_string()),
            skill_dir: project_path.join(".claude/skills"),
            config_path: Some(project_path.join(".mcp.json")),
            config_prefix: "mcpServers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("codex".to_string()),
            skill_dir: project_path.join(".agents/skills"),
            config_path: Some(project_path.join(".codex/config.toml")),
            config_prefix: "mcp_servers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("opencode".to_string()),
            skill_dir: project_path.join(".opencode/skills"),
            config_path: Some(project_path.join("opencode.json")),
            config_prefix: "mcp",
            config_type: McpConfigType::OpenCode,
        },
        TargetDefaults {
            id: AgentTargetId("zcode".to_string()),
            skill_dir: project_path.join(".zcode/skills"),
            config_path: Some(project_path.join(".zcode/config.json")),
            config_prefix: "mcp.servers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("pi".to_string()),
            skill_dir: project_path.join(".pi/skills"),
            config_path: None,
            config_prefix: "",
            config_type: McpConfigType::Common,
        },
    ]
}
