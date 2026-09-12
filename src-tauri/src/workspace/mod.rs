use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use anyhow::{Context, Result, anyhow, bail};
use dirs::home_dir;

pub(crate) mod commands;
mod config;
mod inspect;
mod mcps;
mod skills;
mod targets;
pub(crate) mod types;

use self::types::*;
pub(crate) use config::WorkspaceConfigStore;

// Keep the workspace namespace flat: commands, inspect and the test modules
// import the per-domain entry points through `super::` / `use super::*`.
pub(crate) use self::mcps::{
    apply_mcp_to_target_inner, create_workspace_mcp_inner, delete_workspace_mcp_inner,
    preview_mcp_target_inner, remove_mcp_from_target_inner, update_workspace_mcp_inner,
};
pub(crate) use self::skills::{
    build_sync_skill_options, build_sync_target_options, delete_skill_source_inner,
    delete_skill_sources_inner, discover_git_skills_inner, discover_local_skills_inner,
    filter_discovered_skills_inner, import_batch_git_skills_inner, import_discovered_skills_inner,
    preview_source_sync_conflicts_inner, refresh_git_skill_source_inner, remove_source_sync_inner,
    sync_source_to_targets_inner, update_skill_source_inner,
};
pub(crate) use self::targets::{
    create_workspace_project_inner, create_workspace_target_inner, delete_workspace_project_inner,
    delete_workspace_target_inner, update_workspace_project_inner, update_workspace_target_inner,
};

use self::mcps::read_existing_mcp_entries;
use self::skills::git_cache_last_fetched_at_ms;
#[cfg(windows)]
use self::targets::read_directory_link_target;
use self::targets::{builtin_target_defaults_map, project_agent_defaults};

#[cfg(test)]
use self::mcps::desired_opencode_mcp;
#[cfg(test)]
#[cfg_attr(windows, allow(unused_imports))]
use self::skills::inspect_skill_destination;
#[cfg(test)]
use self::skills::{
    SkillActionKind, SkillLinkSource, apply_include_patterns,
    build_sync_target_options_with_sources, classify_skill_destination,
    discover_skills_in_directory, git_cache_refresh_is_due, parse_batch_git_skill_import_sources,
    partition_excluded, remove_source_symlinks_from_targets_with_root,
};
#[cfg(test)]
use self::targets::read_skill_dir_link;

// Parse owner/repo from common git URL forms (https, ssh, scp-style, bare).
fn parse_git_owner_repo(repo: &str) -> Option<(String, String)> {
    let trimmed = repo.trim().trim_end_matches(".git");
    if trimmed.is_empty() {
        return None;
    }

    // ssh scp-style: git@host:owner/repo
    if let Some(rest) = trimmed.split_once(':') {
        // left looks like a user@host token (no scheme, no slash)
        let left = rest.0;
        if !left.contains("://") && !left.contains('/') {
            return split_owner_repo(rest.1);
        }
    }

    // scheme-style: https://host/owner/repo or ssh://git@host[:port]/owner/repo
    let after_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    split_owner_repo(after_scheme)
}

fn split_owner_repo(path: &str) -> Option<(String, String)> {
    let path = path.trim_start_matches('/');
    let mut parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    // last two segments are owner/repo
    if parts.len() < 2 {
        return None;
    }
    let repo = parts.pop()?;
    let owner = parts.pop()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

fn default_true() -> bool {
    true
}

fn default_config_template() -> String {
    r#"targets:
  codex:
    enabled: true
    skill_dir: ~/.agents/skills
    mcp:
      enabled: true
      config_path: ~/.codex/config.toml
      config_prefix: mcp_servers
      config_type: common

  claude:
    enabled: true
    skill_dir: ~/.claude/skills
    mcp:
      enabled: true
      config_path: ~/.claude.json
      config_prefix: mcpServers
      config_type: common

  opencode:
    enabled: true
    skill_dir: ~/.config/opencode/skills
    mcp:
      enabled: true
      config_path: ~/.config/opencode/opencode.json
      config_prefix: mcp
      config_type: opencode

  zcode:
    enabled: true
    skill_dir: ~/.zcode/skills
    mcp:
      enabled: true
      config_path: ~/.zcode/cli/config.json
      config_prefix: mcp.servers
      config_type: common

  # pi 暂不主动支持 MCP，省略 mcp 段即可；需要时补 mcp.config_path 和 config_prefix。
  pi:
    enabled: true
    skill_dir: ~/.pi/agent/skills

mcps: []

# projects:
#   my-app:
#     path: ~/code/my-app
#     agents:
#       claude:
#         enabled: true
#       codex:
#         enabled: true
#       opencode:
#         enabled: true
#       zcode:
#         enabled: true
"#
    .to_string()
}

// ---------------------------------------------------------------------------
// App state loading
// ---------------------------------------------------------------------------

pub(crate) fn workspace_state_inner(store: &WorkspaceConfigStore) -> Result<WorkspaceState> {
    let document = store.load_document()?;
    let inspection = document
        .validation
        .valid
        .then(|| inspect::inspect_app_state_document(&document.raw_content, store.config_path()))
        .transpose()?;

    Ok(WorkspaceState {
        document: Some(document),
        inspection,
    })
}

fn document_from_content(
    store: &WorkspaceConfigStore,
    config_path: &Path,
    exists: bool,
    raw_content: String,
) -> WorkspaceConfigDocument {
    if !exists {
        return WorkspaceConfigDocument {
            config_path: display_path(config_path),
            exists,
            raw_content,
            config: None,
            validation: ValidationReport {
                valid: false,
                errors: vec!["配置文件不存在。".to_string()],
                warnings: Vec::new(),
            },
        };
    }

    match parse_manager_config(&raw_content, config_path) {
        Ok(config) => WorkspaceConfigDocument {
            config_path: display_path(config_path),
            exists,
            raw_content,
            config: Some(config_to_view(store, &config)),
            validation: ValidationReport {
                valid: true,
                errors: Vec::new(),
                warnings: Vec::new(),
            },
        },
        Err(error) => WorkspaceConfigDocument {
            config_path: display_path(config_path),
            exists,
            raw_content,
            config: None,
            validation: ValidationReport {
                valid: false,
                errors: vec![error.to_string()],
                warnings: Vec::new(),
            },
        },
    }
}

// ---------------------------------------------------------------------------
// Path selectors (rfd wrappers)
// ---------------------------------------------------------------------------

pub(crate) fn select_project_path_inner(path: Option<&str>) -> Result<Option<WorkspaceSelection>> {
    pick_folder(path)
}

pub(crate) fn select_local_skill_source_directory_inner(
    path: Option<&str>,
) -> Result<Option<WorkspaceSelection>> {
    pick_folder(path)
}

pub(crate) fn select_target_skill_directory_inner(
    path: Option<&str>,
) -> Result<Option<WorkspaceSelection>> {
    pick_folder(path)
}

pub(crate) fn select_target_mcp_config_file_inner(
    path: Option<&str>,
) -> Result<Option<WorkspaceSelection>> {
    let dialog = match path.map(str::trim).filter(|v| !v.is_empty()) {
        Some(path) => {
            let expanded = expand_path(path, None)?;
            if expanded.is_file() {
                let file_name = expanded
                    .file_name()
                    .and_then(OsStr::to_str)
                    .unwrap_or_default();
                rfd::FileDialog::new()
                    .set_directory(expanded.parent().unwrap_or_else(|| Path::new(".")))
                    .set_file_name(file_name)
            } else {
                rfd::FileDialog::new()
                    .set_directory(expanded.parent().unwrap_or_else(|| Path::new(".")))
            }
        }
        None => rfd::FileDialog::new().set_directory(std::env::current_dir()?),
    };

    Ok(dialog.pick_file().map(|f| WorkspaceSelection {
        workspace_dir: display_path(&f),
    }))
}

fn pick_folder(initial: Option<&str>) -> Result<Option<WorkspaceSelection>> {
    let initial_dir = match initial.map(str::trim).filter(|v| !v.is_empty()) {
        Some(path) => expand_path(path, None)?,
        None => std::env::current_dir()?,
    };

    Ok(rfd::FileDialog::new()
        .set_directory(initial_dir)
        .pick_folder()
        .map(|selected| WorkspaceSelection {
            workspace_dir: display_path(&selected),
        }))
}

// ---------------------------------------------------------------------------
// Config view / parse
// ---------------------------------------------------------------------------

fn config_to_view(
    store: &WorkspaceConfigStore,
    config: &ResolvedManagerConfig,
) -> WorkspaceConfigView {
    WorkspaceConfigView {
        targets: config.targets.values().map(target_to_view).collect(),
        projects: config
            .projects
            .values()
            .map(|project| ProjectConfigView {
                id: project.id.clone(),
                path: display_path(&project.project_path),
                agents: project.agents.values().map(target_to_view).collect(),
            })
            .collect(),
        skill_sources: config
            .skill_sources
            .iter()
            .map(|(id, source)| match source {
                ResolvedSkillSourceDefinition::Local {
                    root_path,
                    include_name_patterns,
                    include_path_patterns,
                } => SkillSourceConfigView::Local {
                    id: id.clone(),
                    root_path: display_path(root_path),
                    include_name_patterns: include_name_patterns.clone(),
                    include_path_patterns: include_path_patterns.clone(),
                    label: format_local_skill_source_label(root_path),
                },
                ResolvedSkillSourceDefinition::Git {
                    repo,
                    r#ref,
                    include_name_patterns,
                    include_path_patterns,
                } => SkillSourceConfigView::Git {
                    id: id.clone(),
                    repo: repo.clone(),
                    r#ref: r#ref.clone(),
                    last_fetched_at: git_cache_last_fetched_at_ms(store, repo),
                    include_name_patterns: include_name_patterns.clone(),
                    include_path_patterns: include_path_patterns.clone(),
                    label: format_git_skill_source_label(repo, r#ref.as_deref()),
                },
            })
            .collect(),
        mcps: config
            .mcps
            .iter()
            .map(|mcp| McpConfigView {
                name: mcp.name.clone(),
                enabled: mcp.enabled,
                transport: mcp.transport,
                created_at: mcp.created_at,
                homepage: mcp.homepage.clone(),
                command: mcp.command.clone(),
                args: mcp.args.clone(),
                env: mcp.env.clone(),
                url: mcp.url.clone(),
                headers: mcp.headers.clone(),
                timeout: mcp.timeout,
            })
            .collect(),
    }
}

fn target_to_view(target: &ResolvedTargetConfig) -> TargetConfigView {
    TargetConfigView {
        id: target.id.clone(),
        enabled: target.enabled,
        skill_dir: display_path(&target.skill_dir),
        config_path: target.config_path.as_ref().map(|path| display_path(path)),
        mcp_config_prefix: target.mcp_config_prefix.clone(),
        mcp_config_type: target.mcp_config_type,
    }
}

fn parse_manager_config(raw_content: &str, config_path: &Path) -> Result<ResolvedManagerConfig> {
    let raw: RawManagerConfig = serde_yaml::from_str(raw_content)
        .with_context(|| format!("YAML 解析失败：{}", config_path.display()))?;
    let mut targets = BTreeMap::new();
    let mut skill_sources = BTreeMap::new();

    let builtin_defaults = builtin_target_defaults_map();

    for (id, raw_target) in raw.targets {
        let id = normalize_target_id(&id)?;
        let defaults = builtin_defaults.get(&id);
        let skill_dir_raw = raw_target.skill_dir.trim();
        let skill_dir = if !skill_dir_raw.is_empty() {
            expand_path(skill_dir_raw, config_path.parent())?
        } else if let Some(defaults) = defaults {
            defaults.skill_dir.clone()
        } else {
            bail!("目标 {} 的 skill_dir 不能为空。", id);
        };
        let config_file_path = match raw_target.mcp.config_path {
            Some(path) => Some(expand_path(&path, config_path.parent())?),
            None => defaults.and_then(|item| item.config_path.clone()),
        };
        let config_prefix = raw_target
            .mcp
            .config_prefix
            .or_else(|| defaults.map(|item| item.config_prefix.to_string()))
            .unwrap_or_default()
            .trim()
            .to_string();

        // 与 normalize_raw_target_input 的契约一致：configPrefix 只在
        // 真正有 MCP 配置文件可写时才必填（pi 这类 target 两者皆空）。
        if config_file_path.is_some() && config_prefix.is_empty() {
            bail!("目标 {} 的 mcp.config_prefix 不能为空。", id);
        }

        targets.insert(
            id.clone(),
            ResolvedTargetConfig {
                id: id.clone(),
                enabled: raw_target.enabled,
                is_project: false,
                skill_dir,
                config_path: config_file_path,
                mcp_config_prefix: config_prefix,
                mcp_config_type: raw_target.mcp.config_type.unwrap_or_else(|| {
                    defaults
                        .map(|item| item.config_type)
                        .unwrap_or(McpConfigType::Common)
                }),
            },
        );
    }

    for source in raw.skill_sources {
        let id = source.id.trim().to_string();

        if id.is_empty() {
            bail!("发现空的 skill source id。");
        }

        if skill_sources.contains_key(&id) {
            bail!("skill source id 重复：{id}");
        }

        let resolved = match source.source {
            RawSkillSourceDefinition::Local {
                root_path,
                include_name_patterns,
                include_path_patterns,
            } => ResolvedSkillSourceDefinition::Local {
                root_path: expand_path(&root_path, config_path.parent())?,
                include_name_patterns: normalize_patterns(&include_name_patterns),
                include_path_patterns: normalize_patterns(&include_path_patterns),
            },
            RawSkillSourceDefinition::Git {
                repo,
                r#ref,
                include_name_patterns,
                include_path_patterns,
            } => ResolvedSkillSourceDefinition::Git {
                repo,
                r#ref,
                include_name_patterns: normalize_patterns(&include_name_patterns),
                include_path_patterns: normalize_patterns(&include_path_patterns),
            },
        };

        skill_sources.insert(id, resolved);
    }

    let mut seen_mcp_names = HashSet::new();
    let mut mcps = Vec::with_capacity(raw.mcps.len());

    for mcp in raw.mcps {
        let name = mcp.name.trim().to_string();

        if name.is_empty() {
            bail!("发现空的 mcp name。");
        }

        if !seen_mcp_names.insert(name.clone()) {
            bail!("mcp name 重复：{name}");
        }

        match mcp.transport {
            McpTransport::Stdio => {
                if mcp.command.as_deref().unwrap_or("").trim().is_empty() {
                    bail!("stdio MCP {} 必须提供 command。", name);
                }
            }
            McpTransport::Http | McpTransport::Sse => {
                if mcp.url.as_deref().unwrap_or("").trim().is_empty() {
                    bail!("远程 MCP {} 必须提供 mcp 链接。", name);
                }
            }
        }

        mcps.push(ResolvedMcpConfig {
            name,
            enabled: mcp.enabled,
            transport: mcp.transport,
            created_at: mcp.created_at,
            homepage: mcp.homepage,
            command: mcp.command,
            args: mcp.args,
            env: mcp.env,
            url: mcp.url,
            headers: mcp.headers,
            timeout: mcp.timeout,
        });
    }

    let mut projects = BTreeMap::new();

    for (project_id, raw_project) in raw.projects {
        let project_id_trimmed = project_id.trim().to_string();
        if project_id_trimmed.is_empty() {
            bail!("发现空的 project id。");
        }
        let project_path = expand_path(&raw_project.path, config_path.parent())?;
        let agent_defaults = project_agent_defaults(&project_path);
        let mut agents = BTreeMap::new();
        for (agent_name, raw_agent) in raw_project.agents {
            let agent_id = normalize_target_id(&agent_name)?;
            let defaults = agent_defaults.iter().find(|d| d.id == agent_id);
            let skill_dir = defaults
                .map(|d| d.skill_dir.clone())
                .unwrap_or_else(|| project_path.join(&agent_name).join("skills"));
            agents.insert(
                agent_id.clone(),
                ResolvedTargetConfig {
                    id: agent_id,
                    enabled: raw_agent.enabled,
                    is_project: true,
                    skill_dir,
                    config_path: defaults.and_then(|d| d.config_path.clone()),
                    mcp_config_prefix: defaults
                        .map(|d| d.config_prefix.to_string())
                        .unwrap_or_default(),
                    mcp_config_type: defaults
                        .map(|d| d.config_type)
                        .unwrap_or(McpConfigType::Common),
                },
            );
        }
        projects.insert(
            project_id_trimmed.clone(),
            ResolvedProjectConfig {
                id: project_id_trimmed,
                project_path,
                agents,
            },
        );
    }

    Ok(ResolvedManagerConfig {
        targets,
        projects,
        skill_sources,
        mcps,
    })
}

fn format_local_skill_source_label(root_path: &Path) -> String {
    display_path(root_path)
}

// Windows 的规范化路径会附加扩展长度前缀；该前缀只服务于文件系统 API，
// 返回给界面会干扰阅读。
pub(super) fn display_path(path: &Path) -> String {
    #[cfg(windows)]
    {
        let path = path.to_string_lossy();
        if let Some(unc_path) = path.strip_prefix("\\\\?\\UNC\\") {
            return format!("\\\\{unc_path}");
        }
        return path.strip_prefix("\\\\?\\").unwrap_or(&path).to_string();
    }

    #[cfg(not(windows))]
    {
        path.display().to_string()
    }
}

fn format_git_skill_source_label(repo: &str, reference: Option<&str>) -> String {
    format!(
        "{} · {}",
        repo,
        reference
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("default")
    )
}

fn resolve_target_from_id<'a>(
    config: &'a ResolvedManagerConfig,
    composite_id: &str,
) -> Option<&'a ResolvedTargetConfig> {
    if let Some(target) = config.targets.get(&AgentTargetId(composite_id.to_string())) {
        return Some(target);
    }
    if let Some(colon_pos) = composite_id.find(':') {
        let project_id = &composite_id[..colon_pos];
        let agent_id = &composite_id[colon_pos + 1..];
        if let Some(project) = config.projects.get(project_id) {
            return project.agents.get(&AgentTargetId(agent_id.to_string()));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn expand_path(value: &str, base_dir: Option<&Path>) -> Result<PathBuf> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        bail!("路径不能为空。");
    }

    if let Some(stripped) = trimmed.strip_prefix("~/") {
        let home = home_dir().ok_or_else(|| anyhow!("无法解析 HOME 目录。"))?;
        return Ok(home.join(stripped));
    }

    let path = PathBuf::from(trimmed);

    if path.is_absolute() {
        return Ok(path);
    }

    if let Some(base_dir) = base_dir {
        return Ok(base_dir.join(path));
    }

    Ok(std::env::current_dir()?.join(path))
}

// ---------------------------------------------------------------------------
// Filesystem primitives
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn hide_child_console(command: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_child_console(_command: &mut Command) {}

fn run_command(command: &mut Command, context: &str) -> Result<()> {
    hide_child_console(command);

    let output = command
        .output()
        .with_context(|| format!("{context}：无法执行命令"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };

    bail!("{context}：{message}");
}

fn remove_existing_path(path: &Path) -> Result<()> {
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to inspect {}", path.display()));
        }
    };

    if metadata.file_type().is_symlink() {
        if fs::remove_file(path).is_err() {
            fs::remove_dir(path)?;
        }
        return Ok(());
    }

    if remove_platform_directory_link(path)? {
        return Ok(());
    }

    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }

    Ok(())
}

#[cfg(windows)]
fn remove_platform_directory_link(path: &Path) -> Result<bool> {
    if read_directory_link_target(path)?.is_none() {
        return Ok(false);
    }

    junction::delete(path)
        .with_context(|| format!("Failed to delete junction {}", path.display()))?;
    fs::remove_dir(path)
        .with_context(|| format!("Failed to remove junction {}", path.display()))?;
    Ok(true)
}

#[cfg(not(windows))]
fn remove_platform_directory_link(_path: &Path) -> Result<bool> {
    Ok(false)
}

fn remove_empty_parent_dirs(root: &Path, start: Option<&Path>) {
    let mut current = start;
    while let Some(dir) = current {
        if dir == root {
            break;
        }

        match fs::read_dir(dir) {
            Ok(mut entries) => {
                if entries.next().is_some() || fs::remove_dir(dir).is_err() {
                    break;
                }
                current = dir.parent();
            }
            Err(_) => break,
        }
    }
}

#[cfg(unix)]
fn create_directory_symlink(source: &Path, destination: &Path) -> Result<()> {
    std::os::unix::fs::symlink(source, destination)
        .with_context(|| format!("Failed to create symlink {}", destination.display()))
}

#[cfg(windows)]
fn create_directory_symlink(source: &Path, destination: &Path) -> Result<()> {
    junction::create(source, destination)
        .with_context(|| format!("Failed to create junction {}", destination.display()))
}

// ---------------------------------------------------------------------------
// Normalize helpers
// ---------------------------------------------------------------------------

fn normalize_patterns(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .map(|pattern| pattern.trim().to_string())
        .filter(|pattern| !pattern.is_empty())
        .collect()
}

fn normalize_target_id(value: &str) -> Result<AgentTargetId> {
    let trimmed = value.trim().to_lowercase().replace('_', "-");
    let trimmed = match trimmed.as_str() {
        "open-code" => "opencode".to_string(),
        other => other.to_string(),
    };

    if trimmed.is_empty() {
        bail!("target id 不能为空。");
    }

    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        bail!("target id 只能包含小写字母、数字和 - ：{}", value.trim());
    }

    Ok(AgentTargetId(trimmed))
}

fn current_timestamp_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "../tests/workspace.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/workspace_config.rs"]
mod config_tests;
