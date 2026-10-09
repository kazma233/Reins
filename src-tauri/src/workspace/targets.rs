// Target/project domain: config.yaml CRUD for targets and projects, the
// builtin per-agent path defaults, and directory-link skill_dir detection.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

use anyhow::{Context, Result, anyhow, bail};

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
        // create 只接受内置工具，MCP 格式由 AgentSpec 派生，不信任客户端
        // 传值。配置节点路径默认留空(用户未显式提供 config_path 时不假设
        // 想要 MCP,避免默认值诱发校验噪音);用户显式提供 config_path 时
        // 从 defaults 取同工具的 configPrefix,与解析侧同口径。
        let target_id = normalize_target_id(&input.target_id)?;
        let config_type = builtin_create_config_type(&target_id)?;
        let defaults_prefix = input
            .config_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .and_then(|_| {
                builtin_target_defaults_map()
                    .get(&target_id)
                    .map(|defaults| defaults.config_prefix.to_string())
            });
        let (target_id, target_config) = normalize_raw_target_input(
            input,
            Some(config_type),
            defaults_prefix,
            config.config_path(),
        )?;

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

        // 存量 config_type 与 config_prefix 原样保留(省略仍省略、显式仍
        // 显式),客户端不再传这两个字段;有效类型回落内置 defaults 的口径
        // 与解析侧一致。
        let stored = raw_config
            .targets
            .get(current_target_id.as_str())
            .and_then(|target| target.mcp.config_prefix.clone());
        let stored_config_type = raw_config
            .targets
            .get(current_target_id.as_str())
            .and_then(|target| target.mcp.config_type);
        let (next_target_id, next_target) =
            normalize_raw_target_input(input, stored_config_type, stored, config.config_path())?;

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
    // 落盘的 config_type：create 传派生值，update 传存量原值。
    persisted_config_type: Option<McpConfigType>,
    // MCP 配置节点路径：create 走内置 defaults 派生，update 走存量原值;
    // 客户端不再传,有效值由调用方按场景选好传入。
    persisted_config_prefix: Option<String>,
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
    let config_prefix = persisted_config_prefix.unwrap_or_default();

    // MCP 配置文件和 configPrefix 必须成对出现：只有前缀没有路径无处可写，
    // 只有路径没有前缀无法定位写入节点。不需要 MCP 分发的 target
    // 允许两者都为空，此时只做 skill 分发。dsh 按 name+serverName 定位
    // 条目（writer 声明 prefix 不必填），允许"有路径 + 空 prefix"。
    // 校验用的有效类型与解析侧同口径：存量值优先，其次内置 defaults。
    let effective_config_type = persisted_config_type
        .or_else(|| {
            builtin_target_defaults_map()
                .get(&target_id)
                .map(|defaults| defaults.config_type)
        })
        .unwrap_or(McpConfigType::Common);
    let prefix_required =
        super::mcp_formats::mcp_format_writer(effective_config_type).prefix_required();
    match normalized_config_path.as_deref() {
        Some(_) if config_prefix.is_empty() && prefix_required => {
            bail!("target {} 的 MCP configPrefix 不能为空。", target_id);
        }
        // 路径缺失时,只有声明 prefix 必填的格式(common/grok/opencode)
        // 才算异常;dsh 这类 prefix 不必填的格式允许单独无 MCP 配置
        // (用户只做 skills 分发,没有 MCP 需要清理)。
        None if !config_prefix.is_empty() && prefix_required => {
            bail!(
                "target {} 填写了 configPrefix,必须同时提供 MCP 配置文件路径。",
                target_id
            );
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
                config_type: persisted_config_type,
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

// create 只允许为内置工具建 target，并从同一份 defaults 派生 MCP 格式；
// 存量自定义 target 不迁移，照常解析、渲染、编辑、删除（update 不走此校验）。
fn builtin_create_config_type(target_id: &AgentTargetId) -> Result<McpConfigType> {
    builtin_target_defaults_map()
        .get(target_id)
        .map(|defaults| defaults.config_type)
        .ok_or_else(|| {
            let allowed = crate::agents::AGENTS
                .iter()
                .filter_map(|spec| spec.target_id)
                .collect::<Vec<_>>()
                .join("、");
            anyhow!("仅支持创建内置工具的 target：{allowed}。")
        })
}

// ---------------------------------------------------------------------------
// Target defaults (codex/claude/opencode builtin paths)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct TargetDefaults {
    pub(crate) id: AgentTargetId,
    pub(crate) skill_dir: PathBuf,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) config_prefix: &'static str,
    pub(crate) config_type: McpConfigType,
}

#[cfg(test)]
use crate::support::fs::grok_home_path;

#[cfg(test)]
#[test]
fn grokbuild_home_uses_environment_before_home_default() {
    let home = PathBuf::from("/synthetic-home");
    let custom = PathBuf::from("/custom-grok");
    assert_eq!(
        grok_home_path(None, Some(home.clone())),
        Some(home.join(".grok"))
    );
    assert_eq!(
        grok_home_path(Some(custom.clone().into_os_string()), Some(home)),
        Some(custom)
    );
}

pub(crate) fn target_presets_inner() -> Vec<TargetPreset> {
    let defaults = builtin_target_defaults_map();
    crate::agents::AGENTS
        .iter()
        .filter_map(|spec| {
            let target_id = spec.target_id?;
            let defaults = defaults.get(&AgentTargetId(target_id.to_string()))?;
            Some(TargetPreset {
                target_id: defaults.id.clone(),
                label: spec.label.to_string(),
                enabled: true,
                skill_dir: display_path(&defaults.skill_dir),
                config_path: defaults.config_path.as_deref().map(display_path),
                mcp_config_prefix: defaults.config_prefix.to_string(),
            })
        })
        .collect()
}

// 全局 defaults 从 agents 清单派生：路径 = GlobalRoot 解析结果 + spec 的
// 相对布局；根目录解析失败（无 HOME）时 skill_dir 落空串、config_path 落
// None，与逐条手写时代的口径一致。
pub(crate) fn builtin_target_defaults() -> Vec<TargetDefaults> {
    crate::agents::AGENTS
        .iter()
        .filter_map(|spec| {
            let target_id = spec.target_id?;
            let root = spec.global.root.resolve();
            Some(TargetDefaults {
                id: AgentTargetId(target_id.to_string()),
                skill_dir: root
                    .as_ref()
                    .map(|base| base.join(spec.global.skill_dir))
                    .unwrap_or_default(),
                config_path: spec
                    .global
                    .mcp_config_path
                    .and_then(|rel| root.as_ref().map(|base| base.join(rel))),
                config_prefix: spec.mcp.prefix,
                config_type: spec.mcp.config_type,
            })
        })
        .collect()
}

pub(super) fn builtin_target_defaults_map() -> HashMap<AgentTargetId, TargetDefaults> {
    builtin_target_defaults()
        .into_iter()
        .map(|item| (item.id.clone(), item))
        .collect()
}

// 项目级 defaults 同样从 agents 清单派生；项目与全局非同构（spec 的
// project 布局如实表达差异，如 dsh 项目级无 MCP 配置入口）。
pub(crate) fn project_agent_defaults(project_path: &Path) -> Vec<TargetDefaults> {
    crate::agents::AGENTS
        .iter()
        .filter_map(|spec| {
            let target_id = spec.target_id?;
            let project = spec.project.as_ref()?;
            Some(TargetDefaults {
                id: AgentTargetId(target_id.to_string()),
                skill_dir: project_path.join(project.skill_dir),
                config_path: project.mcp_config_path.map(|rel| project_path.join(rel)),
                config_prefix: spec.mcp.prefix,
                config_type: spec.mcp.config_type,
            })
        })
        .collect()
}
