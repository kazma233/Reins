// Skill/sync domain: discover skills from local/git sources, import them into
// config, and install/remove directory links across targets.

use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use walkdir::WalkDir;

use crate::state::skill_discovery::SkillDiscoveryState;

use super::WorkspaceConfigStore;
use super::targets::{
    is_linked_skill_dir, read_directory_link_target, read_skill_dir_link,
    resolve_symlink_target_path,
};
use super::types::*;
use super::{
    create_directory_symlink, display_path, expand_path, normalize_patterns, parse_manager_config,
    remove_empty_parent_dirs, remove_existing_path, resolve_target_from_id, run_command,
    workspace_state_inner,
};

// Discover all skills in a source root, without applying include filters.
// The sync dialog lists every skill and tags each with its match state.
fn discover_source_skills_raw(
    store: &WorkspaceConfigStore,
    source: &ResolvedSkillSourceDefinition,
) -> Result<(PathBuf, Vec<DiscoveredSkill>)> {
    match source {
        ResolvedSkillSourceDefinition::Local { root_path, .. } => {
            let skills = discover_skills_in_directory(root_path)?;
            Ok((root_path.clone(), skills))
        }
        ResolvedSkillSourceDefinition::Git { repo, r#ref, .. } => {
            let cache = clone_git_repo_to_cache(store, repo, r#ref.as_deref())?;
            let skills = discover_skills_in_directory(&cache)?;
            Ok((cache, skills))
        }
    }
}

// Borrow the (name, path) include patterns from a resolved source.
fn include_patterns_of(source: &ResolvedSkillSourceDefinition) -> (Vec<String>, Vec<String>) {
    match source {
        ResolvedSkillSourceDefinition::Local {
            include_name_patterns,
            include_path_patterns,
            ..
        } => (include_name_patterns.clone(), include_path_patterns.clone()),
        ResolvedSkillSourceDefinition::Git {
            include_name_patterns,
            include_path_patterns,
            ..
        } => (include_name_patterns.clone(), include_path_patterns.clone()),
    }
}

// Build a list of all targets (global + project agents) for the sync-dialog UI.
// Also detects targets whose skill_dir is a directory link pointing at another managed
// target's skill_dir, so the UI can disable them and show the link target.
pub(crate) fn build_sync_target_options(
    store: &WorkspaceConfigStore,
) -> Result<Vec<SyncTargetOption>> {
    let config = store.parse()?;
    let sources = build_skill_link_sources(store, &config)?;
    build_sync_target_options_with_sources(&config, &sources)
}

#[derive(Clone, Debug)]
pub(super) struct SkillLinkSource {
    pub(super) id: String,
    pub(super) root_path: PathBuf,
    pub(super) include_name_patterns: Vec<String>,
    pub(super) include_path_patterns: Vec<String>,
}

fn build_skill_link_sources(
    store: &WorkspaceConfigStore,
    config: &ResolvedManagerConfig,
) -> Result<Vec<SkillLinkSource>> {
    config
        .skill_sources
        .iter()
        .map(|(id, source)| {
            let root_path = resolve_source_root(store, source)?;
            let (include_name_patterns, include_path_patterns) = include_patterns_of(source);
            Ok(SkillLinkSource {
                id: id.clone(),
                root_path: normalize_link_path(&root_path),
                include_name_patterns,
                include_path_patterns,
            })
        })
        .collect()
}

// 与 source 根路径解析分离，保证关联分类不会 clone/pull git source。
pub(super) fn build_sync_target_options_with_sources(
    config: &ResolvedManagerConfig,
    sources: &[SkillLinkSource],
) -> Result<Vec<SyncTargetOption>> {
    // Flatten all targets into (id, skill_dir) pairs.
    let mut flat: Vec<(String, &Path)> = Vec::new();
    for target in config.targets.values() {
        flat.push((target.id.to_string(), target.skill_dir.as_path()));
    }
    for project in config.projects.values() {
        for (agent_id, target) in &project.agents {
            let composite_id = format!("{}:{}", project.id, agent_id);
            flat.push((composite_id, target.skill_dir.as_path()));
        }
    }

    // Map each managed skill_dir (canonicalized) back to its target id, so a
    // directory link can be resolved to "which managed target it points at".
    let mut dir_to_target: HashMap<PathBuf, String> = HashMap::new();
    for (id, skill_dir) in &flat {
        let canonical = skill_dir
            .canonicalize()
            .unwrap_or_else(|_| skill_dir.to_path_buf());
        dir_to_target.entry(canonical).or_insert_with(|| id.clone());
    }

    let mut options = Vec::with_capacity(flat.len());
    for (id, skill_dir) in flat {
        // Resolve a directory-link target only if it points at another managed
        // target; links to external directories stay selectable.
        let linked_target_id =
            read_skill_dir_link(skill_dir)?.and_then(|linked| dir_to_target.get(&linked).cloned());
        let enabled = resolve_target_from_id(&config, &id)
            .map(|t| t.enabled)
            .unwrap_or(false);
        // 整目录继承另一个受管 target 时，不重复报告同一批物理链接。
        let links = if linked_target_id.is_some() {
            Vec::new()
        } else {
            scan_target_skill_links(skill_dir, sources)?
        };
        options.push(SyncTargetOption {
            id: id.clone(),
            label: id,
            skill_dir: display_path(skill_dir),
            enabled,
            linked_target_id,
            links,
        });
    }

    Ok(options)
}

fn scan_target_skill_links(
    skill_dir: &Path,
    sources: &[SkillLinkSource],
) -> Result<Vec<SkillLinkAssociation>> {
    let entries = match fs::read_dir(skill_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("读取 target skills 目录失败：{}", skill_dir.display()));
        }
    };

    let mut links = Vec::new();
    for entry in entries {
        let entry = entry
            .with_context(|| format!("读取 target skills 目录项失败：{}", skill_dir.display()))?;
        let destination = entry.path();
        let Some(source_path) = read_directory_link_target(&destination)? else {
            continue;
        };

        links.push(classify_skill_link(&destination, &source_path, sources));
    }

    links.sort_by(|left, right| {
        left.skill_name
            .cmp(&right.skill_name)
            .then_with(|| left.destination_path.cmp(&right.destination_path))
    });
    Ok(links)
}

fn classify_skill_link(
    destination: &Path,
    source_path: &Path,
    sources: &[SkillLinkSource],
) -> SkillLinkAssociation {
    let source_path = normalize_link_path(source_path);
    let skill_name = destination
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("skill")
        .to_string();
    let source_skill_name = source_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(&skill_name);
    let matches = sources
        .iter()
        .filter_map(|source| {
            relative_to_source_root(&source_path, &source.root_path)
                .map(|relative_path| (source, relative_path))
        })
        .collect::<Vec<_>>();
    let matched_source_ids = matches
        .iter()
        .map(|(source, _)| source.id.clone())
        .collect::<Vec<_>>();

    let state = if matches.is_empty() {
        SkillLinkState::Unmanaged
    } else if !source_path.join("SKILL.md").is_file() {
        SkillLinkState::SourceMissing
    } else if matches.iter().any(|(source, relative_path)| {
        matches_include(
            source_skill_name,
            &relative_path.to_string_lossy(),
            &source.include_name_patterns,
            &source.include_path_patterns,
        )
    }) {
        SkillLinkState::Linked
    } else {
        SkillLinkState::Excluded
    };

    SkillLinkAssociation {
        skill_name,
        destination_path: display_path(destination),
        source_path: display_path(&source_path),
        matched_source_ids,
        state,
    }
}

fn normalize_link_path(path: &Path) -> PathBuf {
    let mut missing_components = Vec::<OsString>::new();
    let mut current = Some(path);

    while let Some(candidate) = current {
        if let Ok(normalized) = candidate.canonicalize() {
            return missing_components
                .iter()
                .rev()
                .fold(normalized, |path, component| path.join(component));
        }

        let Some(component) = candidate.file_name() else {
            break;
        };
        missing_components.push(component.to_os_string());
        current = candidate.parent();
    }

    path.to_path_buf()
}

fn relative_to_source_root(path: &Path, source_root: &Path) -> Option<PathBuf> {
    #[cfg(not(windows))]
    {
        path.strip_prefix(source_root).ok().map(Path::to_path_buf)
    }

    #[cfg(windows)]
    {
        let mut path_components = path.components();
        for source_component in source_root.components() {
            let path_component = path_components.next()?;
            if !windows_path_component_eq(path_component, source_component) {
                return None;
            }
        }
        Some(path_components.as_path().to_path_buf())
    }
}

#[cfg(windows)]
fn windows_path_component_eq(left: Component<'_>, right: Component<'_>) -> bool {
    let left = left.as_os_str().to_string_lossy();
    let right = right.as_os_str().to_string_lossy();
    let left = left.strip_prefix(r"\\?\").unwrap_or(&left);
    let right = right.strip_prefix(r"\\?\").unwrap_or(&right);

    left.eq_ignore_ascii_case(right)
}

// Build a list of discovered skills for the sync-dialog UI, tagging each with
// whether it matches the source's include patterns so the UI can group matched
// (default-checked) vs unmatched (collapsed) skills.
pub(crate) fn build_sync_skill_options(
    store: &WorkspaceConfigStore,
    source_id: &str,
) -> Result<SyncSkillOptionsResult> {
    let config = store.parse()?;
    let source_definition = source_definition_from_config(&config, source_id)?;
    let (source_root, skills) = discover_source_skills_raw(store, source_definition)?;
    let (name_patterns, path_patterns) = include_patterns_of(source_definition);

    Ok(SyncSkillOptionsResult {
        skills: skills
            .into_iter()
            .map(|s| {
                let matched =
                    matches_include(&s.name, &s.relative_path, &name_patterns, &path_patterns);
                SyncSkillOption {
                    name: s.name,
                    relative_path: s.relative_path,
                    matched,
                }
            })
            .collect(),
        source_root: display_path(&source_root),
    })
}

// Unified sync: discover source → remove old directory links from all targets →
// install selected skills to selected targets via directory link.
// No longer reads/writes a skills registry in config; operates purely on links.
pub(crate) fn sync_source_to_targets_inner(
    store: &WorkspaceConfigStore,
    input: SourceSyncInput,
) -> Result<SourceSyncResult> {
    let config = store.parse()?;
    let source_definition = source_definition_from_config(&config, &input.source_id)?;

    // 1. Reuse the dialog snapshot when present; it was already refreshed when
    // the sync dialog opened, so confirmation should not fetch and scan again.
    let (source_root, discovered) = source_sync_snapshot(store, source_definition, &input)?;

    // 2. Build a set of selected skill relative paths.
    let selected: HashSet<&str> = input.skill_paths.iter().map(|s| s.as_str()).collect();
    let selected_skills: Vec<_> = discovered
        .iter()
        .filter(|s| selected.contains(s.relative_path.as_str()))
        .collect();

    let conflicts =
        source_sync_conflicts(&config, &source_root, &selected_skills, &input.target_ids)?;
    if !input.overwrite_existing && !conflicts.is_empty() {
        bail!(
            "同步会覆盖非本来源路径：\n{}",
            conflicts
                .iter()
                .map(|conflict| format!(
                    "{} / {} -> {}",
                    conflict.target_id, conflict.skill_name, conflict.destination_path
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    // 3. Remove old directory links for this source from the selected targets only,
    //    so the sync operation never touches targets the user didn't pick.
    let target_id_filter: HashSet<&str> = input.target_ids.iter().map(|s| s.as_str()).collect();
    let removed = remove_source_symlinks_from_targets_with_root(
        &config,
        &source_root,
        Some(&target_id_filter),
    )?;

    // 4. Create directory links for selected skills to selected targets.
    let mut applied = Vec::new();
    let mut warnings = Vec::new();

    for target_id in &input.target_ids {
        let Some(target) = resolve_target_from_id(&config, target_id) else {
            warnings.push(format!("目标 {} 不存在，已跳过。", target_id));
            continue;
        };
        if !target.enabled {
            warnings.push(format!("目标 {} 已禁用，已跳过。", target_id));
            continue;
        }
        if target.is_project && is_linked_skill_dir(&target.skill_dir)? {
            continue;
        }

        for skill in &selected_skills {
            let source_dir = source_skill_dir(&source_root, &skill.relative_path)?;
            let dest = target.skill_dir.join(&skill.name);

            let action = classify_skill_destination(&source_dir, &dest)?;
            match action.kind {
                SkillActionKind::Create | SkillActionKind::Replace => {
                    if dest.exists() || dest.symlink_metadata().is_ok() {
                        remove_existing_path(&dest)?;
                    }
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent)
                            .with_context(|| format!("创建目标目录失败：{}", parent.display()))?;
                    }
                    create_directory_symlink(&source_dir, &dest)?;
                    applied.push(SkillSyncItem {
                        skill_name: skill.name.clone(),
                        target_id: AgentTargetId(target_id.clone()),
                        source_path: display_path(&source_dir),
                        destination_path: display_path(&dest),
                        action: action.kind.as_str().to_string(),
                        detail: action.detail,
                    });
                }
                // Already linked to this source — nothing to do.
                SkillActionKind::Unchanged => {}
            }
        }
    }

    Ok(SourceSyncResult {
        removed,
        applied,
        warnings,
    })
}

fn source_sync_snapshot(
    store: &WorkspaceConfigStore,
    source: &ResolvedSkillSourceDefinition,
    input: &SourceSyncInput,
) -> Result<(PathBuf, Vec<DiscoveredSkill>)> {
    match (&input.source_root, &input.skills) {
        (Some(source_root), Some(skills)) => {
            let source_root = validate_sync_snapshot_root(store, source, source_root)?;
            let discovered = skills
                .iter()
                .map(|skill| sync_skill_option_to_discovered(&source_root, skill))
                .collect::<Result<Vec<_>>>()?;
            Ok((source_root, discovered))
        }
        _ => discover_source_skills_raw(store, source),
    }
}

pub(crate) fn preview_source_sync_conflicts_inner(
    store: &WorkspaceConfigStore,
    input: SourceSyncInput,
) -> Result<Vec<SourceSyncConflict>> {
    let config = store.parse()?;
    let source_definition = source_definition_from_config(&config, &input.source_id)?;
    let (source_root, discovered) = source_sync_snapshot(store, source_definition, &input)?;
    let selected: HashSet<&str> = input.skill_paths.iter().map(|s| s.as_str()).collect();
    let selected_skills = discovered
        .iter()
        .filter(|skill| selected.contains(skill.relative_path.as_str()))
        .collect::<Vec<_>>();

    source_sync_conflicts(&config, &source_root, &selected_skills, &input.target_ids)
}

fn source_sync_conflicts(
    config: &ResolvedManagerConfig,
    source_root: &Path,
    selected_skills: &[&DiscoveredSkill],
    target_ids: &[String],
) -> Result<Vec<SourceSyncConflict>> {
    let normalized_source_root = source_root
        .canonicalize()
        .unwrap_or_else(|_| source_root.to_path_buf());
    let mut conflicts = Vec::new();

    for target_id in target_ids {
        let Some(target) = resolve_target_from_id(config, target_id) else {
            continue;
        };
        if !target.enabled || (target.is_project && is_linked_skill_dir(&target.skill_dir)?) {
            continue;
        }

        for skill in selected_skills {
            let source_dir = source_skill_dir(source_root, &skill.relative_path)?;
            let destination = target.skill_dir.join(&skill.name);
            if let Some((existing_kind, detail)) =
                non_source_destination_detail(&destination, &normalized_source_root)?
            {
                conflicts.push(SourceSyncConflict {
                    skill_name: skill.name.clone(),
                    target_id: AgentTargetId(target_id.clone()),
                    source_path: display_path(&source_dir),
                    destination_path: display_path(&destination),
                    existing_kind,
                    detail,
                });
            }
        }
    }

    Ok(conflicts)
}

fn non_source_destination_detail(
    destination: &Path,
    source_root: &Path,
) -> Result<Option<(String, String)>> {
    let metadata = match destination.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to inspect {}", destination.display()));
        }
    };

    if let Some(resolved) = read_directory_link_target(destination)? {
        if resolved.starts_with(source_root) {
            return Ok(None);
        }
        return Ok(Some((
            "目录链接".to_string(),
            format!("当前指向 {}", display_path(&resolved)),
        )));
    }

    if metadata.file_type().is_symlink() {
        let link_target = fs::read_link(destination)
            .with_context(|| format!("读取 {} 的软链接失败", destination.display()))?;
        let resolved = resolve_symlink_target_path(destination, &link_target);
        if resolved.starts_with(source_root) {
            return Ok(None);
        }
        return Ok(Some((
            "软链接".to_string(),
            format!("当前指向 {}", display_path(&resolved)),
        )));
    }

    if metadata.is_dir() {
        return Ok(Some(("目录".to_string(), "当前是同名目录".to_string())));
    }

    if metadata.is_file() {
        return Ok(Some(("文件".to_string(), "当前是同名文件".to_string())));
    }

    Ok(Some(("路径".to_string(), "当前是同名路径".to_string())))
}

fn validate_sync_snapshot_root(
    store: &WorkspaceConfigStore,
    source: &ResolvedSkillSourceDefinition,
    source_root: &str,
) -> Result<PathBuf> {
    let source_root = PathBuf::from(source_root.trim());
    if source_root.as_os_str().is_empty() {
        bail!("同步来源路径不能为空。");
    }

    let expected_root = resolve_source_root(store, source)?;
    let normalized_expected = expected_root.canonicalize().unwrap_or(expected_root);
    let normalized_source = source_root
        .canonicalize()
        .with_context(|| format!("同步来源路径不存在：{}", source_root.display()))?;

    if !normalized_source.starts_with(&normalized_expected) {
        bail!(
            "同步来源路径不属于当前来源：{}",
            normalized_source.display()
        );
    }

    Ok(normalized_source)
}

// Local root of a source definition: the configured root path for local
// sources, the git cache dir for git sources.
fn resolve_source_root(
    store: &WorkspaceConfigStore,
    source: &ResolvedSkillSourceDefinition,
) -> Result<PathBuf> {
    match source {
        ResolvedSkillSourceDefinition::Local { root_path, .. } => Ok(root_path.clone()),
        ResolvedSkillSourceDefinition::Git { repo, .. } => store.git_cache_dir_for_repo(repo),
    }
}

fn sync_skill_option_to_discovered(
    source_root: &Path,
    skill: &SyncSkillOption,
) -> Result<DiscoveredSkill> {
    let source_dir = source_skill_dir(source_root, &skill.relative_path)?;
    Ok(DiscoveredSkill {
        name: skill.name.clone(),
        relative_path: skill.relative_path.clone(),
        skill_file_path: display_path(&source_dir.join("SKILL.md")),
        source_path: Some(display_path(&source_dir)),
    })
}

fn source_skill_dir(source_root: &Path, relative_path: &str) -> Result<PathBuf> {
    let relative = Path::new(relative_path);
    if relative.components().any(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        )
    }) {
        bail!("skill 路径不能越过来源根目录：{relative_path}");
    }

    let source_root = source_root
        .canonicalize()
        .unwrap_or_else(|_| source_root.to_path_buf());
    let source_dir = source_root.join(relative);
    let normalized_source_dir = source_dir
        .canonicalize()
        .with_context(|| format!("skill 来源目录不存在：{}", source_dir.display()))?;

    if !normalized_source_dir.starts_with(&source_root) {
        bail!("skill 路径不属于来源根目录：{relative_path}");
    }

    Ok(normalized_source_dir)
}

// Remove directory links installed by this source from the given targets. This is the
// "remove sync" action: clears the source's footprint on the picked targets
// only, leaving other targets untouched.
pub(crate) fn remove_source_sync_inner(
    store: &WorkspaceConfigStore,
    source_id: &str,
    target_ids: &[String],
) -> Result<SourceSyncResult> {
    let config = store.parse()?;
    let target_id_filter: HashSet<&str> = target_ids.iter().map(|s| s.as_str()).collect();
    let removed =
        remove_source_symlinks_from_targets(store, &config, source_id, Some(&target_id_filter))?;
    Ok(SourceSyncResult {
        removed,
        applied: Vec::new(),
        warnings: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Skill discovery (WalkDir + include patterns)
// ---------------------------------------------------------------------------

pub(super) fn discover_skills_in_directory(root: &Path) -> Result<Vec<DiscoveredSkill>> {
    let mut skills = Vec::new();

    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() || entry.file_name() != "SKILL.md" {
            continue;
        }

        let Some(parent) = entry.path().parent() else {
            continue;
        };
        let relative_path = parent
            .strip_prefix(root)
            .unwrap_or(parent)
            .to_string_lossy()
            .to_string();
        let name = parent
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("skill")
            .to_string();

        skills.push(DiscoveredSkill {
            name,
            relative_path: relative_path.clone(),
            skill_file_path: display_path(entry.path()),
            source_path: Some(display_path(parent)),
        });
    }

    skills.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(skills)
}

fn source_definition_from_config<'a>(
    config: &'a ResolvedManagerConfig,
    source_id: &str,
) -> Result<&'a ResolvedSkillSourceDefinition> {
    config
        .skill_sources
        .get(source_id)
        .ok_or_else(|| anyhow!("未找到来源：{source_id}"))
}

// ---------------------------------------------------------------------------
// Directory-link classify / inspect primitives
// ---------------------------------------------------------------------------

pub(super) fn classify_skill_destination(
    source_path: &Path,
    destination: &Path,
) -> Result<SkillAction> {
    if !destination.exists() && destination.symlink_metadata().is_err() {
        return Ok(SkillAction {
            kind: SkillActionKind::Create,
            detail: format!("将创建软链接：{}", display_path(destination)),
        });
    }

    let metadata = destination
        .symlink_metadata()
        .with_context(|| format!("Failed to inspect {}", destination.display()))?;

    if let Some(resolved) = read_directory_link_target(destination)? {
        let normalized_source = source_path
            .canonicalize()
            .unwrap_or_else(|_| source_path.to_path_buf());
        if resolved == normalized_source {
            return Ok(SkillAction {
                kind: SkillActionKind::Unchanged,
                detail: format!("目录链接已存在：{}", display_path(destination)),
            });
        }
    }

    if metadata.file_type().is_symlink() {
        if symlink_points_to_source(destination, source_path) {
            return Ok(SkillAction {
                kind: SkillActionKind::Unchanged,
                detail: format!("软链接已存在：{}", display_path(destination)),
            });
        }
    }

    Ok(SkillAction {
        kind: SkillActionKind::Replace,
        detail: format!("{} 已存在同名路径，将替换。", display_path(destination)),
    })
}

#[cfg(test)]
#[cfg_attr(windows, allow(dead_code))]
pub(super) fn inspect_skill_destination(
    source_path: &Path,
    destination: &Path,
) -> (String, String) {
    if !destination.exists() && destination.symlink_metadata().is_err() {
        return (
            "missing".to_string(),
            format!("尚未安装到 {}", display_path(destination)),
        );
    }

    let Ok(metadata) = destination.symlink_metadata() else {
        return (
            "error".to_string(),
            format!("无法读取 {}", display_path(destination)),
        );
    };

    if metadata.file_type().is_symlink() {
        let Ok(link_target) = fs::read_link(destination) else {
            return ("broken".to_string(), "软链接已损坏。".to_string());
        };

        let resolved_target = resolve_symlink_target_path(destination, &link_target);
        if symlink_points_to_source(destination, source_path) {
            return (
                "installed".to_string(),
                format!("已链接到 {}", display_path(source_path)),
            );
        }

        return (
            "conflict".to_string(),
            format!("当前软链接指向 {}", display_path(&resolved_target)),
        );
    }

    if metadata.is_dir() {
        return (
            "conflict".to_string(),
            "目标位置已经有真实目录。".to_string(),
        );
    }

    (
        "conflict".to_string(),
        "目标位置已经有真实文件。".to_string(),
    )
}

fn symlink_points_to_source(destination: &Path, source_path: &Path) -> bool {
    let Ok(link_target) = fs::read_link(destination) else {
        return false;
    };

    let normalized_source = source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf());
    resolve_symlink_target_path(destination, &link_target) == normalized_source
}

// ---------------------------------------------------------------------------
// Skill discovery commands (interactive preview before import)
// ---------------------------------------------------------------------------

pub(crate) fn discover_git_skills_inner(
    store: &WorkspaceConfigStore,
    state: &SkillDiscoveryState,
    repo: &str,
    reference: Option<&str>,
) -> Result<SkillDiscoveryResultView> {
    let cache = clone_git_repo_to_cache(store, repo, reference)?;
    let discovery_id = uuid::Uuid::new_v4().to_string();
    let skills = discover_skills_in_directory(&cache)?;

    state.store(
        discovery_id.clone(),
        SkillDiscoverySnapshot {
            source: DiscoverySourceDefinition::Git {
                repo: repo.to_string(),
                r#ref: reference.map(ToString::to_string),
            },
            skills: Arc::from(skills.clone()),
        },
    )?;

    Ok(SkillDiscoveryResultView {
        discovery_id,
        repo: repo.to_string(),
        r#ref: reference.map(ToString::to_string),
        include_name_patterns: Vec::new(),
        include_path_patterns: Vec::new(),
        skills,
        excluded_skills: Vec::new(),
    })
}

pub(crate) fn discover_local_skills_inner(
    state: &SkillDiscoveryState,
    source_path: &str,
) -> Result<SkillDiscoveryResultView> {
    let source_root = expand_path(source_path, None)?;

    if !source_root.exists() {
        bail!("目录不存在：{}", source_root.display());
    }

    let discovery_id = uuid::Uuid::new_v4().to_string();
    let skills = discover_skills_in_directory(&source_root)?;

    state.store(
        discovery_id.clone(),
        SkillDiscoverySnapshot {
            source: DiscoverySourceDefinition::Local {
                root_path: source_root.clone(),
            },
            skills: Arc::from(skills.clone()),
        },
    )?;

    Ok(SkillDiscoveryResultView {
        discovery_id,
        repo: display_path(&source_root),
        r#ref: None,
        include_name_patterns: Vec::new(),
        include_path_patterns: Vec::new(),
        skills,
        excluded_skills: Vec::new(),
    })
}

pub(crate) fn filter_discovered_skills_inner(
    state: &SkillDiscoveryState,
    discovery_id: &str,
    include_name_patterns: Option<&[String]>,
    include_path_patterns: Option<&[String]>,
) -> Result<SkillDiscoveryResultView> {
    let snapshot = state
        .load(discovery_id)?
        .ok_or_else(|| anyhow!("未找到 discovery：{discovery_id}"))?;
    let name_patterns = normalize_patterns(include_name_patterns.unwrap_or(&[]));
    let path_patterns = normalize_patterns(include_path_patterns.unwrap_or(&[]));
    let all_skills = snapshot.skills.as_ref().to_vec();
    let mut matched = all_skills.clone();
    apply_include_patterns(&mut matched, &name_patterns, &path_patterns);
    let excluded_skills = partition_excluded(&all_skills, &matched);

    Ok(SkillDiscoveryResultView {
        discovery_id: discovery_id.to_string(),
        repo: discovery_source_label(&snapshot.source),
        r#ref: discovery_source_ref(&snapshot.source),
        include_name_patterns: name_patterns,
        include_path_patterns: path_patterns,
        skills: matched,
        excluded_skills,
    })
}

// After apply_include_patterns has retained `kept` in place, collect the skills
// from `all` whose relative_path is not in `kept` as the excluded set.
pub(super) fn partition_excluded(
    all: &[DiscoveredSkill],
    kept: &[DiscoveredSkill],
) -> Vec<DiscoveredSkill> {
    let kept_paths: HashSet<&str> = kept.iter().map(|s| s.relative_path.as_str()).collect();
    all.iter()
        .filter(|s| !kept_paths.contains(s.relative_path.as_str()))
        .cloned()
        .collect()
}

// Import discovered skills: add the source definition to config (no skill registry).
// Softlinks are created later via sync_source_to_targets.
pub(crate) fn import_discovered_skills_inner(
    store: &WorkspaceConfigStore,
    state: &SkillDiscoveryState,
    discovery_id: &str,
    include_name_patterns: Option<&[String]>,
    include_path_patterns: Option<&[String]>,
) -> Result<SkillDiscoveryResultView> {
    let snapshot = state
        .load(discovery_id)?
        .ok_or_else(|| anyhow!("未找到 discovery：{discovery_id}"))?;
    let source_id = uuid::Uuid::new_v4().to_string();
    let name_patterns = normalize_patterns(include_name_patterns.unwrap_or(&[]));
    let path_patterns = normalize_patterns(include_path_patterns.unwrap_or(&[]));
    let mut discovered = snapshot.skills.as_ref().to_vec();
    apply_include_patterns(&mut discovered, &name_patterns, &path_patterns);

    store.locked(|config| {
        let mut raw_config = config.parse_raw()?;

        ensure_skill_source(
            &mut raw_config,
            RawSkillSourceConfig {
                id: source_id.clone(),
                source: discovery_source_to_raw_definition(
                    &snapshot.source,
                    name_patterns.clone(),
                    path_patterns.clone(),
                ),
            },
        )?;

        config.write_raw(&raw_config)
    })?;
    state.remove(discovery_id)?;

    Ok(SkillDiscoveryResultView {
        discovery_id: discovery_id.to_string(),
        repo: discovery_source_label(&snapshot.source),
        r#ref: discovery_source_ref(&snapshot.source),
        include_name_patterns: name_patterns,
        include_path_patterns: path_patterns,
        skills: discovered,
        excluded_skills: Vec::new(),
    })
}

pub(super) fn parse_batch_git_skill_import_sources(
    yaml_content: &str,
) -> Result<Vec<BatchGitSkillImportSourceInput>> {
    let trimmed = yaml_content.trim();

    if trimmed.is_empty() {
        bail!("批量导入内容不能为空。");
    }

    let mut sources = serde_yaml::from_str::<Vec<BatchGitSkillImportSourceInput>>(trimmed)
        .context("解析批量导入 YAML 失败")?;

    if sources.is_empty() {
        bail!("批量导入内容至少需要 1 个 git 来源。");
    }

    for (index, source) in sources.iter_mut().enumerate() {
        let source_number = index + 1;
        let repo = source.repo.trim();
        let reference = source.r#ref.trim();

        if repo.is_empty() {
            bail!("第 {source_number} 个来源缺少 repo。");
        }

        if reference.is_empty() {
            bail!("第 {source_number} 个来源缺少 ref。");
        }

        if source
            .include_name_patterns
            .iter()
            .chain(source.include_path_patterns.iter())
            .any(|pattern| pattern.trim().is_empty())
        {
            bail!("第 {source_number} 个来源的 include patterns 不能包含空字符串。");
        }

        source.repo = repo.to_string();
        source.r#ref = reference.to_string();
        source.include_name_patterns = normalize_patterns(&source.include_name_patterns);
        source.include_path_patterns = normalize_patterns(&source.include_path_patterns);
    }

    Ok(sources)
}

pub(crate) fn import_batch_git_skills_inner(
    store: &WorkspaceConfigStore,
    input: BatchGitSkillImportInput,
) -> Result<BatchGitSkillImportResult> {
    let sources = parse_batch_git_skill_import_sources(&input.yaml_content)?;

    store.locked(|config| {
        let mut raw_config = config.parse_raw()?;
        let mut items = Vec::with_capacity(sources.len());

        for source in sources {
            match import_git_source(store, &mut raw_config, &source) {
                Ok(result) => items.push(result),
                Err(error) => items.push(BatchGitSkillImportItemResult {
                    repo: source.repo,
                    r#ref: Some(source.r#ref),
                    include_name_patterns: source.include_name_patterns,
                    include_path_patterns: source.include_path_patterns,
                    imported_count: 0,
                    skill_names: Vec::new(),
                    error: Some(error.to_string()),
                }),
            }
        }

        config.write_raw(&raw_config)?;

        let imported_count = items.iter().map(|item| item.imported_count).sum();
        let failed_count = items.iter().filter(|item| item.error.is_some()).count();

        Ok(BatchGitSkillImportResult {
            imported_count,
            succeeded_count: items.len() - failed_count,
            failed_count,
            items,
        })
    })
}

fn import_git_source(
    store: &WorkspaceConfigStore,
    raw_config: &mut RawManagerConfig,
    source: &BatchGitSkillImportSourceInput,
) -> Result<BatchGitSkillImportItemResult> {
    let name_patterns = source.include_name_patterns.clone();
    let path_patterns = source.include_path_patterns.clone();
    let cache = clone_git_repo_to_cache(store, &source.repo, Some(&source.r#ref))?;
    let mut discovered = discover_skills_in_directory(&cache)?;
    apply_include_patterns(&mut discovered, &name_patterns, &path_patterns);
    let source_id = uuid::Uuid::new_v4().to_string();
    let skill_names = discovered
        .iter()
        .map(|skill| skill.name.clone())
        .collect::<Vec<_>>();

    ensure_skill_source(
        raw_config,
        RawSkillSourceConfig {
            id: source_id.clone(),
            source: RawSkillSourceDefinition::Git {
                repo: source.repo.clone(),
                r#ref: Some(source.r#ref.clone()),
                include_name_patterns: name_patterns.clone(),
                include_path_patterns: path_patterns.clone(),
            },
        },
    )?;

    Ok(BatchGitSkillImportItemResult {
        repo: source.repo.clone(),
        r#ref: Some(source.r#ref.clone()),
        include_name_patterns: name_patterns,
        include_path_patterns: path_patterns,
        imported_count: discovered.len(),
        skill_names,
        error: None,
    })
}

// ---------------------------------------------------------------------------
// Remove directory links pointing at skills of `source_id` from every target.
// Only links are touched — real dirs/files are user-managed.
// ---------------------------------------------------------------------------

// Remove directory links pointing at skills of `source_id` from targets, and
// collect each removed link so callers can report what was cleared.
// `target_ids = None` means every target; otherwise only the listed targets.
fn remove_source_symlinks_from_targets(
    store: &WorkspaceConfigStore,
    config: &ResolvedManagerConfig,
    source_id: &str,
    target_ids: Option<&HashSet<&str>>,
) -> Result<Vec<SkillSyncItem>> {
    let source_definition = source_definition_from_config(config, source_id)?;
    let source_root = resolve_source_root(store, source_definition)?;
    remove_source_symlinks_from_targets_with_root(config, &source_root, target_ids)
}

pub(super) fn remove_source_symlinks_from_targets_with_root(
    config: &ResolvedManagerConfig,
    source_root: &Path,
    target_ids: Option<&HashSet<&str>>,
) -> Result<Vec<SkillSyncItem>> {
    // Canonicalize so it matches the canonicalized link target from
    // resolve_symlink_target_path; otherwise starts_with can fail when the
    // configured root_path differs in case/links from its real path.
    let source_root = source_root
        .canonicalize()
        .unwrap_or_else(|_| source_root.to_path_buf());

    let global_targets = config
        .targets
        .values()
        .map(|target| (target.id.to_string(), target));
    let project_targets = config.projects.values().flat_map(|project| {
        project
            .agents
            .values()
            .map(move |target| (format!("{}:{}", project.id, target.id), target))
    });

    let mut removed = Vec::new();
    for (target_id, target) in global_targets.chain(project_targets) {
        if let Some(ids) = target_ids {
            if !ids.contains(target_id.as_str()) {
                continue;
            }
        }
        let Ok(entries) = fs::read_dir(&target.skill_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(resolved) = read_directory_link_target(&path)? else {
                continue;
            };
            if relative_to_source_root(&resolved, &source_root).is_some() {
                let skill_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                remove_existing_path(&path)?;
                removed.push(SkillSyncItem {
                    skill_name,
                    target_id: AgentTargetId(target_id.clone()),
                    source_path: display_path(&resolved),
                    destination_path: display_path(&path),
                    action: "remove".to_string(),
                    detail: String::new(),
                });
            }
        }
    }

    Ok(removed)
}

// ---------------------------------------------------------------------------
// Delete skill source
// ---------------------------------------------------------------------------

// 只更新 source 配置；已有 target 软链接在用户下次同步时才按新规则调整。
pub(crate) fn update_skill_source_inner(
    store: &WorkspaceConfigStore,
    input: SkillSourceMutationInput,
) -> Result<WorkspaceState> {
    store.locked(|config| {
        let mut raw_config = config.parse_raw()?;

        let source = raw_config
            .skill_sources
            .iter_mut()
            .find(|s| s.id == input.source_id)
            .ok_or_else(|| anyhow!("未找到来源：{}", input.source_id))?;

        let name_patterns = normalize_patterns(&input.include_name_patterns);
        let path_patterns = normalize_patterns(&input.include_path_patterns);

        match &mut source.source {
            RawSkillSourceDefinition::Git {
                r#ref,
                include_name_patterns,
                include_path_patterns,
                ..
            } => {
                if let Some(new_ref) = input.r#ref {
                    *r#ref = Some(new_ref);
                }
                *include_name_patterns = name_patterns;
                *include_path_patterns = path_patterns;
            }
            RawSkillSourceDefinition::Local {
                include_name_patterns,
                include_path_patterns,
                ..
            } => {
                *include_name_patterns = name_patterns;
                *include_path_patterns = path_patterns;
            }
        }

        config.write_raw(&raw_config)
    })?;

    workspace_state_inner(store)
}

pub(crate) fn delete_skill_source_inner(
    store: &WorkspaceConfigStore,
    source_id: &str,
) -> Result<WorkspaceState> {
    store.locked(|config| {
        let raw_content = config.read_raw()?;
        let mut raw_config: RawManagerConfig = serde_yaml::from_str(&raw_content)?;
        delete_skill_source_entries(
            store,
            config.config_path(),
            &raw_content,
            &mut raw_config,
            source_id,
        )?;
        config.write_raw(&raw_config)
    })?;

    workspace_state_inner(store)
}

// Remove one skill source from an in-memory raw config: existence check,
// symlink cleanup, git cache cleanup and the config entry itself. Shared by
// single and batch delete; must only be called while the store lock is held.
fn delete_skill_source_entries(
    store: &WorkspaceConfigStore,
    config_path: &Path,
    raw_content: &str,
    raw_config: &mut RawManagerConfig,
    source_id: &str,
) -> Result<()> {
    if !raw_config
        .skill_sources
        .iter()
        .any(|source| source.id == source_id)
    {
        bail!("未找到来源：{source_id}");
    }

    // Remove softlinks for this source's skills from every target.
    let resolved_config = parse_manager_config(raw_content, config_path)?;
    remove_source_symlinks_from_targets(store, &resolved_config, source_id, None)?;

    // Remove git cache if this is a git source AND no other source references
    // the same repo (cache is now keyed by owner/repo and shared across refs).
    let source = raw_config.skill_sources.iter().find(|s| s.id == source_id);
    if let Some(source) = source {
        if let RawSkillSourceDefinition::Git { repo, .. } = &source.source {
            let still_referenced = raw_config.skill_sources.iter().any(|s| {
                s.id != source_id
                    && matches!(
                        &s.source,
                        RawSkillSourceDefinition::Git { repo: other_repo, .. } if other_repo == repo
                    )
            });
            if !still_referenced {
                let cache = store.git_cache_dir_for_repo(repo)?;
                remove_existing_path(&cache)?;
                remove_empty_parent_dirs(&store.git_cache_root(), cache.parent());
            }
        }
    }

    raw_config
        .skill_sources
        .retain(|source| source.id != source_id);

    Ok(())
}

pub(crate) fn delete_skill_sources_inner(
    store: &WorkspaceConfigStore,
    source_ids: Vec<String>,
) -> Result<()> {
    if source_ids.is_empty() {
        bail!("source_ids 不能为空");
    }

    // The frontend batch-deletes by firing one request per item, so per-item
    // read/write here would both race against concurrent commands and rewrite
    // the file N times. One lock, one read-modify-write for the whole batch.
    store.locked(|config| {
        let raw_content = config.read_raw()?;
        let mut raw_config: RawManagerConfig = serde_yaml::from_str(&raw_content)?;

        for source_id in &source_ids {
            delete_skill_source_entries(
                store,
                config.config_path(),
                &raw_content,
                &mut raw_config,
                source_id,
            )?;
        }

        config.write_raw(&raw_config)
    })
}

fn ensure_skill_source(config: &mut RawManagerConfig, source: RawSkillSourceConfig) -> Result<()> {
    if let Some(existing) = config
        .skill_sources
        .iter()
        .find(|item| item.id == source.id)
    {
        let existing_raw = serde_yaml::to_string(existing)?;
        let next_raw = serde_yaml::to_string(&source)?;

        if existing_raw != next_raw {
            bail!("skill source 配置冲突：{}", source.id);
        }

        return Ok(());
    }

    config.skill_sources.push(source);
    Ok(())
}

// ---------------------------------------------------------------------------
// Git cache — clone into persistent <config>/reins/git/<owner>/<repo>/.
// ---------------------------------------------------------------------------

const GIT_AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const GIT_CACHE_REFRESH_MARKER: &str = "reins-last-fetch";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitCacheRefreshPolicy {
    IfStale,
    Force,
}

fn discovery_source_label(source: &DiscoverySourceDefinition) -> String {
    match source {
        DiscoverySourceDefinition::Local { root_path } => display_path(root_path),
        DiscoverySourceDefinition::Git { repo, .. } => repo.clone(),
    }
}

fn discovery_source_ref(source: &DiscoverySourceDefinition) -> Option<String> {
    match source {
        DiscoverySourceDefinition::Local { .. } => None,
        DiscoverySourceDefinition::Git { r#ref, .. } => r#ref.clone(),
    }
}

fn discovery_source_to_raw_definition(
    source: &DiscoverySourceDefinition,
    include_name_patterns: Vec<String>,
    include_path_patterns: Vec<String>,
) -> RawSkillSourceDefinition {
    match source {
        DiscoverySourceDefinition::Local { root_path } => RawSkillSourceDefinition::Local {
            root_path: display_path(root_path),
            include_name_patterns,
            include_path_patterns,
        },
        DiscoverySourceDefinition::Git { repo, r#ref, .. } => RawSkillSourceDefinition::Git {
            repo: repo.clone(),
            r#ref: r#ref.clone(),
            include_name_patterns,
            include_path_patterns,
        },
    }
}

// Resolve the persistent cache for normal discovery. Existing caches are only
// refreshed after the interval expires, so opening the workspace stays local.
fn clone_git_repo_to_cache(
    store: &WorkspaceConfigStore,
    repo: &str,
    reference: Option<&str>,
) -> Result<PathBuf> {
    refresh_git_repo_cache(store, repo, reference, GitCacheRefreshPolicy::IfStale)
}

pub(crate) fn refresh_git_skill_source_inner(
    store: &WorkspaceConfigStore,
    source_id: &str,
) -> Result<()> {
    let config = store.parse()?;
    let source = source_definition_from_config(&config, source_id)?;
    let ResolvedSkillSourceDefinition::Git { repo, r#ref, .. } = source else {
        bail!("来源不是 git：{source_id}");
    };

    refresh_git_repo_cache(store, repo, r#ref.as_deref(), GitCacheRefreshPolicy::Force)?;
    Ok(())
}

fn refresh_git_repo_cache(
    store: &WorkspaceConfigStore,
    repo: &str,
    reference: Option<&str>,
    policy: GitCacheRefreshPolicy,
) -> Result<PathBuf> {
    let cache_dir = store.git_cache_dir_for_repo(repo)?;
    fs::create_dir_all(store.git_cache_root())?;

    // A path that already exists but isn't a git worktree would break checkout,
    // so wipe it and fall through to a clean clone.
    let git_dir = cache_dir.join(".git");
    if cache_dir.exists() && !git_dir.exists() {
        remove_existing_path(&cache_dir)?;
    }

    if cache_dir.exists() {
        let marker_path = git_cache_refresh_marker_path(&cache_dir);
        let recorded_reference = fs::read_to_string(&marker_path).ok();
        let last_refreshed_at = marker_path.metadata().and_then(|meta| meta.modified()).ok();
        let should_refresh = policy == GitCacheRefreshPolicy::Force
            || git_cache_refresh_is_due(
                recorded_reference.as_deref().map(str::trim),
                reference.map(str::trim),
                last_refreshed_at,
                SystemTime::now(),
            );

        if should_refresh {
            update_git_cache(&cache_dir, reference)?;
            write_git_cache_refresh_marker(&cache_dir, reference)?;
        }
        return Ok(cache_dir);
    }

    // Fresh clone.
    let mut clone_command = Command::new("git");
    clone_command.arg("clone");

    if let Some(reference) = reference
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
    {
        clone_command.arg("--branch").arg(reference);
    }

    clone_command.arg(repo).arg(&cache_dir);
    run_command(&mut clone_command, "拉取 git 仓库失败")?;
    write_git_cache_refresh_marker(&cache_dir, reference)?;

    Ok(cache_dir)
}

fn git_cache_refresh_marker_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(".git").join(GIT_CACHE_REFRESH_MARKER)
}

// 列表展示复用刷新 marker 的修改时间，避免为展示状态增加另一份持久化数据。
pub(super) fn git_cache_last_fetched_at_ms(
    store: &WorkspaceConfigStore,
    repo: &str,
) -> Option<i64> {
    let marker_path = git_cache_refresh_marker_path(&store.git_cache_dir_for_repo(repo).ok()?);
    fs::metadata(marker_path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

fn write_git_cache_refresh_marker(cache_dir: &Path, reference: Option<&str>) -> Result<()> {
    let marker_path = git_cache_refresh_marker_path(cache_dir);
    fs::write(&marker_path, reference.unwrap_or_default().trim())
        .with_context(|| format!("记录 git 仓库更新时间失败：{}", display_path(&marker_path)))
}

pub(super) fn git_cache_refresh_is_due(
    recorded_reference: Option<&str>,
    requested_reference: Option<&str>,
    last_refreshed_at: Option<SystemTime>,
    now: SystemTime,
) -> bool {
    if recorded_reference != Some(requested_reference.unwrap_or_default().trim()) {
        return true;
    }

    let Some(last_refreshed_at) = last_refreshed_at else {
        return true;
    };

    match now.duration_since(last_refreshed_at) {
        Ok(elapsed) => elapsed >= GIT_AUTO_REFRESH_INTERVAL,
        Err(_) => true,
    }
}

fn update_git_cache(cache_dir: &Path, reference: Option<&str>) -> Result<()> {
    // Bare fetch then checkout handles both branches and tags; if the ref is
    // missing remotely the fetch/pull surfaces the git error to the user.
    run_command(
        Command::new("git")
            .current_dir(cache_dir)
            .arg("fetch")
            .arg("origin"),
        "更新 git 仓库失败",
    )?;

    if let Some(reference) = reference
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
    {
        run_command(
            Command::new("git")
                .current_dir(cache_dir)
                .arg("checkout")
                .arg(reference),
            "切换 git 分支失败",
        )?;
    }

    run_command(
        Command::new("git")
            .current_dir(cache_dir)
            .arg("pull")
            .arg("--ff-only"),
        "更新 git 仓库失败",
    )
}

fn matches_any_pattern(value: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| wildcard_match(value, pattern))
}

// A skill matches the include filter when it passes BOTH name and path filters.
// Empty patterns = match everything (no constraint on that dimension).
fn matches_include(
    name: &str,
    path: &str,
    name_patterns: &[String],
    path_patterns: &[String],
) -> bool {
    let name_ok = name_patterns.is_empty() || matches_any_pattern(name, name_patterns);
    let path_ok = path_patterns.is_empty() || matches_any_pattern(path, path_patterns);
    name_ok && path_ok
}

// Keep only skills matching the include filter. Used by import/discovery paths
// that only persist matched skills. The sync dialog does NOT call this — it
// keeps all skills and tags each with `matched` for UI grouping.
pub(super) fn apply_include_patterns(
    skills: &mut Vec<DiscoveredSkill>,
    name_patterns: &[String],
    path_patterns: &[String],
) {
    if name_patterns.is_empty() && path_patterns.is_empty() {
        return;
    }
    skills.retain(|skill| {
        matches_include(
            &skill.name,
            &skill.relative_path,
            name_patterns,
            path_patterns,
        )
    });
}

fn wildcard_match(value: &str, pattern: &str) -> bool {
    let value = value.as_bytes();
    let pattern = pattern.as_bytes();
    let mut value_index = 0usize;
    let mut pattern_index = 0usize;
    let mut star_index = None;
    let mut match_index = 0usize;

    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == value[value_index])
        {
            value_index += 1;
            pattern_index += 1;
            continue;
        }

        if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            match_index = value_index;
            continue;
        }

        if let Some(index) = star_index {
            pattern_index = index + 1;
            match_index += 1;
            value_index = match_index;
            continue;
        }

        return false;
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SkillActionKind {
    Create,
    Unchanged,
    Replace,
}

impl SkillActionKind {
    // Wire form for the SkillSyncItem contract field.
    pub(super) fn as_str(self) -> &'static str {
        match self {
            SkillActionKind::Create => "create",
            SkillActionKind::Unchanged => "unchanged",
            SkillActionKind::Replace => "replace",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SkillAction {
    pub(super) kind: SkillActionKind,
    pub(super) detail: String,
}
