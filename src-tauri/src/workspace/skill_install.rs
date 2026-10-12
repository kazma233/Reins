// skill 分发的「target 安装面」：把「一个 skill 装进一个 target」的物理语义
// 收在一处——目标是否参与分发、链接落在哪、落点上现在是什么、装/卸的结果。
//
// 安装、冲突预演、链接枚举、按来源卸载四条路径都经这里，skip 规则、落点计算、
// 链接分类与路径规范化只有一份实现；各调用方只决定「拿这些结果做什么文案」。
// 盘上真实链接是安装状态的权威来源，config.yaml 只声明意图。

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use super::display_path;
use super::targets::read_directory_link_target;
use super::types::{
    AgentTargetId, ResolvedManagerConfig, ResolvedTargetConfig, SkillSyncItem, TargetRef,
    TargetSkip,
};
use super::{create_directory_symlink, remove_existing_path};

// 一个参与分发决策的 target。
pub(super) struct ManagedTarget<'a> {
    pub(super) id: String,
    // 目标不存在（Missing）时没有配置可指，调用方必须先看 skip。
    pub(super) target: Option<&'a ResolvedTargetConfig>,
    // Some = 不参与安装与清理；枚举（UI 只读展示）仍需要它。
    pub(super) skip: Option<TargetSkip>,
}

impl ManagedTarget<'_> {
    // 只在 skip == None（目标存在且参与分发）时调用。
    pub(super) fn skill_dir(&self) -> &Path {
        &self
            .target
            .expect("跳过的目标不参与分发，调用方必须先看 skip")
            .skill_dir
    }
}

// 解析一批 target id（含项目复合 id），保持输入顺序：不存在的 id 也占一位
// （TargetSkip::Missing），由调用方决定是警告还是忽略。同步与冲突预演共用，
// 才能保证「预演会报的问题」与「同步会跳过的目标」是同一批。
pub(super) fn resolve_ids<'a>(
    config: &'a ResolvedManagerConfig,
    target_ids: &[String],
) -> Result<Vec<ManagedTarget<'a>>> {
    target_ids
        .iter()
        .map(|target_id| {
            let reference = TargetRef::parse(target_id);
            Ok(ManagedTarget {
                id: target_id.clone(),
                target: config.target(&reference),
                skip: config.skip_reason(&reference)?,
            })
        })
        .collect()
}

// 配置里全部全局 target + 项目级 target；skip 已判定，调用方只决定呈现。
pub(super) fn managed_targets(config: &ResolvedManagerConfig) -> Result<Vec<ManagedTarget<'_>>> {
    config
        .managed_refs()?
        .into_iter()
        .map(|(reference, target, skip)| {
            Ok(ManagedTarget {
                id: reference.composite_id(),
                target: Some(target),
                skip,
            })
        })
        .collect()
}

// 一个分发来源：来源根 + 该来源下的 skill。
pub(super) struct SkillInstall<'a> {
    root: &'a Path,
    // 规范化后的来源根：判定「落点是否指进本来源」的同一口径，两侧都规范化
    // 才能让 /var → /private/var 这类系统软链接下的比对成立。
    normalized_root: PathBuf,
}

impl<'a> SkillInstall<'a> {
    pub(super) fn new(root: &'a Path) -> Self {
        Self {
            root,
            normalized_root: normalize_link_path(root),
        }
    }

    // 一个 skill 的源目录（规范化）：来源根 + 清单相对路径。
    pub(super) fn source_dir(&self, relative_path: &str) -> Result<PathBuf> {
        let relative = Path::new(relative_path);
        if relative.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        }) {
            bail!("skill 路径不能越过来源根目录：{relative_path}");
        }

        let normalized_root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.to_path_buf());
        let source_dir = normalized_root.join(relative);
        let normalized_source_dir = source_dir
            .canonicalize()
            .with_context(|| format!("skill 来源目录不存在：{}", source_dir.display()))?;

        if !normalized_source_dir.starts_with(&normalized_root) {
            bail!("skill 路径不属于来源根目录：{relative_path}");
        }

        Ok(normalized_source_dir)
    }

    // 预演：哪些落点会被非本来源路径覆盖。只有会真正阻断同步的条目才产出。
    pub(super) fn conflicts(
        &self,
        targets: &[&ManagedTarget<'_>],
        selected: &[SelectedSkill<'_>],
    ) -> Result<Vec<InstallConflict>> {
        let mut conflicts = Vec::new();
        for target in targets {
            for skill in selected {
                let source_dir = self.source_dir(skill.relative_path)?;
                let destination = destination_of(target, skill.name);
                let Some(conflict) = conflict_of(&self.classify(&source_dir, &destination)?) else {
                    continue;
                };
                conflicts.push(InstallConflict {
                    target_id: target.id.clone(),
                    skill_name: skill.name.to_string(),
                    source_path: source_dir,
                    destination_path: destination,
                    existing_kind: conflict.0,
                    detail: conflict.1,
                });
            }
        }
        Ok(conflicts)
    }

    // 安装：把选中的 skill 装进给定 target。已指向本来源的落点记为 unchanged，
    // 由调用方决定提示；覆盖非本来源路径是调用方的明确选择。
    pub(super) fn apply(
        &self,
        targets: &[&ManagedTarget<'_>],
        selected: &[SelectedSkill<'_>],
    ) -> Result<Vec<InstallItem>> {
        let mut items = Vec::new();
        for target in targets {
            for skill in selected {
                let source_dir = self.source_dir(skill.relative_path)?;
                let destination = destination_of(target, skill.name);
                let state = self.classify(&source_dir, &destination)?;

                if matches!(state, DestinationState::LinkedHere { .. }) {
                    items.push(InstallItem::unchanged(
                        skill,
                        &target.id,
                        &source_dir,
                        &destination,
                        &state,
                    ));
                    continue;
                }

                if destination.exists() || destination.symlink_metadata().is_ok() {
                    remove_existing_path(&destination)?;
                }
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("创建目标目录失败：{}", parent.display()))?;
                }
                create_directory_symlink(&source_dir, &destination)?;
                items.push(InstallItem::installed(
                    skill,
                    &target.id,
                    &source_dir,
                    &destination,
                    &state,
                ));
            }
        }
        Ok(items)
    }

    // 卸载：删掉所有指进本来源的链接（targets 只含参与分发的目标）。
    pub(super) fn remove_installed(
        &self,
        targets: &[&ManagedTarget<'_>],
    ) -> Result<Vec<InstallItem>> {
        let mut removed = Vec::new();
        for target in targets {
            for (destination, resolved) in enumerate_links(target.skill_dir())? {
                if !self.links_into_source(&resolved) {
                    continue;
                }
                remove_existing_path(&destination)?;
                removed.push(InstallItem::removed(&target.id, &destination, &resolved));
            }
        }
        Ok(removed)
    }

    // 单个落点的卸载（同步弹窗逐条清理）：先确认它属于该 target 的目录，
    // 再确认确实是链接；真实目录/文件一律拒绝。
    pub(super) fn remove_at(
        &self,
        target_id: &str,
        skill_dir: &Path,
        destination: &Path,
    ) -> Result<InstallItem> {
        // 只规范化父目录：对链接自身 canonicalize 会把链接解析到目标路径，
        // 无法再判断它位于哪个 skill_dir 下。
        let parent = destination
            .parent()
            .ok_or_else(|| anyhow!("路径无效：{}", display_path(destination)))?;
        if normalize_link_path(parent) != normalize_link_path(skill_dir) {
            bail!(
                "路径不属于该 target 的 skills 目录：{}",
                display_path(destination)
            );
        }

        let resolved = read_directory_link_target(destination)?
            .ok_or_else(|| anyhow!("目标不是软链接，已跳过：{}", display_path(destination)))?;
        remove_existing_path(destination)?;

        Ok(InstallItem::removed(target_id, destination, &resolved))
    }

    fn links_into_source(&self, resolved: &Path) -> bool {
        relative_to_source_root(&normalize_link_path(resolved), &self.normalized_root).is_some()
    }

    // 落点上现在是什么：create / unchanged / replace / conflict 四种呈现都从
    // 这一个状态派生，调用方不再各自比较路径。
    fn classify(&self, source_dir: &Path, destination: &Path) -> Result<DestinationState> {
        let metadata = match destination.symlink_metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(DestinationState::Uninstalled);
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to inspect {}", destination.display()));
            }
        };

        if let Some(resolved) = read_directory_link_target(destination)? {
            return Ok(self.link_state(&resolved, source_dir, "目录链接"));
        }

        if metadata.file_type().is_symlink() {
            let link_target = fs::read_link(destination)
                .with_context(|| format!("读取 {} 的软链接失败", destination.display()))?;
            let resolved = resolve_symlink_target_path(destination, &link_target);
            return Ok(self.link_state(&resolved, source_dir, "软链接"));
        }

        if metadata.is_dir() {
            return Ok(DestinationState::RealDirectory);
        }
        if metadata.is_file() {
            return Ok(DestinationState::RealFile);
        }
        Ok(DestinationState::RealPath)
    }

    // 恰好指到要装的那个 skill = 已安装（幂等 noop）；指进本来源但指到别的
    // skill = 位置不对，算冲突；指向本来源之外 = 非受管路径，同样冲突。
    fn link_state(&self, resolved: &Path, source_dir: &Path, kind: &str) -> DestinationState {
        let normalized = normalize_link_path(resolved);
        if self.same_skill(&normalized, source_dir) {
            return DestinationState::LinkedHere {
                kind: kind.to_string(),
            };
        }
        if relative_to_source_root(&normalized, &self.normalized_root).is_some() {
            return DestinationState::LinkedOtherSkill {
                resolved: resolved.to_path_buf(),
                kind: kind.to_string(),
            };
        }
        DestinationState::LinkedElsewhere {
            resolved: resolved.to_path_buf(),
            kind: kind.to_string(),
        }
    }

    fn same_skill(&self, normalized_resolved: &Path, source_dir: &Path) -> bool {
        let normalized_source = source_dir
            .canonicalize()
            .unwrap_or_else(|_| source_dir.to_path_buf());
        normalized_resolved == normalized_source
    }
}

// 本次分发选中的 skill：展示名 + 来源内的相对路径。
#[derive(Clone, Copy)]
pub(super) struct SelectedSkill<'a> {
    pub(super) name: &'a str,
    pub(super) relative_path: &'a str,
}

fn destination_of(target: &ManagedTarget<'_>, skill_name: &str) -> PathBuf {
    target.skill_dir().join(skill_name)
}

// 落点左右四种可区分的事实；「指向本来源」与「指向别处」必须分开，
// 前者是幂等 noop，后者是冲突。
enum DestinationState {
    Uninstalled,
    LinkedHere { kind: String },
    // 指进本来源，但不是要装的那个 skill：位置不对，会阻断同步。
    LinkedOtherSkill { resolved: PathBuf, kind: String },
    LinkedElsewhere { resolved: PathBuf, kind: String },
    RealDirectory,
    RealFile,
    RealPath,
}

// 落点的对外呈现：同步弹窗（只读 state/detail）与安装判定（读 action）
// 共用同一状态机，不再各自维护一份分类规则。当前唯一消费方是测试辅助
// inspect_skill_destination，故随测试编译。
#[cfg(test)]
pub(super) struct DestinationSnapshot {
    // installed / conflict / missing / broken / error
    pub(super) state: &'static str,
    pub(super) detail: String,
    // create / replace / unchanged
    pub(super) action: &'static str,
}

impl DestinationState {
    #[cfg(test)]
    fn snapshot(&self, destination: &Path) -> DestinationSnapshot {
        match self {
            DestinationState::Uninstalled => DestinationSnapshot {
                state: "missing",
                detail: format!("尚未安装到 {}", display_path(destination)),
                action: "create",
            },
            DestinationState::LinkedHere { kind } => DestinationSnapshot {
                state: "installed",
                detail: format!("{kind}已存在：{}", display_path(destination)),
                action: "unchanged",
            },
            DestinationState::LinkedOtherSkill { resolved, kind } => DestinationSnapshot {
                state: "conflict",
                detail: format!("{kind}指向本来源的其它位置：{}", display_path(resolved)),
                action: "replace",
            },
            DestinationState::LinkedElsewhere { resolved, .. } => DestinationSnapshot {
                state: "conflict",
                detail: format!("当前链接指向 {}", display_path(resolved)),
                action: "replace",
            },
            DestinationState::RealDirectory => DestinationSnapshot {
                state: "conflict",
                detail: "目标位置已经有真实目录。".to_string(),
                action: "replace",
            },
            DestinationState::RealFile => DestinationSnapshot {
                state: "conflict",
                detail: "目标位置已经有真实文件。".to_string(),
                action: "replace",
            },
            DestinationState::RealPath => DestinationSnapshot {
                state: "conflict",
                detail: format!("目标路径已存在：{}", display_path(destination)),
                action: "replace",
            },
        }
    }
}

#[cfg(test)]
impl DestinationSnapshot {
    // 读不到落点元数据时的兜底呈现（只有 UI 只读路径会遇到）。
    pub(super) fn unreadable(destination: &Path) -> Self {
        Self {
            state: "error",
            detail: format!("无法读取 {}", display_path(destination)),
            action: "replace",
        }
    }
}

// 同步弹窗的只读呈现：(state, detail)。state 取值 missing / installed /
// conflict / error，与安装路径共用同一状态机，不会各判一套。
#[cfg(test)]
#[cfg_attr(windows, allow(dead_code))]
pub(super) fn inspect_skill_destination(
    source_path: &Path,
    destination: &Path,
) -> (String, String) {
    let install = SkillInstall::new(source_path);
    let source_dir = source_path.to_path_buf();
    let snapshot = destination_snapshot(&install, &source_dir, destination);
    (snapshot.state.to_string(), snapshot.detail)
}

// UI 只读呈现：落点当前是什么。与安装路径共用同一状态机。
#[cfg(test)]
pub(super) fn destination_snapshot(
    install: &SkillInstall<'_>,
    source_dir: &Path,
    destination: &Path,
) -> DestinationSnapshot {
    match install.classify(source_dir, destination) {
        Ok(state) => state.snapshot(destination),
        Err(_) => DestinationSnapshot::unreadable(destination),
    }
}

// 只有会阻断同步的落点产出冲突；「没东西」与「已指向本来源」都不阻断。
fn conflict_of(state: &DestinationState) -> Option<(String, String)> {
    match state {
        DestinationState::Uninstalled | DestinationState::LinkedHere { .. } => None,
        DestinationState::LinkedOtherSkill { resolved, kind }
        | DestinationState::LinkedElsewhere { resolved, kind } => {
            Some((kind.clone(), format!("当前指向 {}", display_path(resolved))))
        }
        DestinationState::RealDirectory => Some(("目录".to_string(), "当前是同名目录".to_string())),
        DestinationState::RealFile => Some(("文件".to_string(), "当前是同名文件".to_string())),
        DestinationState::RealPath => Some(("路径".to_string(), "当前是同名路径".to_string())),
    }
}

// 安装/卸载的事实条目；wire 类型（SkillSyncItem / SourceSyncConflict）在
// 调用方转换，这里不承担展示格式。
pub(super) struct InstallItem {
    pub(super) skill_name: String,
    pub(super) target_id: AgentTargetId,
    pub(super) source_path: String,
    pub(super) destination_path: String,
    pub(super) action: &'static str,
    pub(super) detail: Option<String>,
}

impl InstallItem {
    fn installed(
        skill: &SelectedSkill<'_>,
        target_id: &str,
        source_dir: &Path,
        destination: &Path,
        state: &DestinationState,
    ) -> Self {
        let (action, detail) = match state {
            DestinationState::RealDirectory => {
                ("replace", Some("目标位置已经有真实目录，已替换。"))
            }
            DestinationState::RealFile => ("replace", Some("目标位置已经有真实文件，已替换。")),
            DestinationState::RealPath => ("replace", Some("目标路径已存在，已替换。")),
            _ => ("create", None),
        };
        Self {
            skill_name: skill.name.to_string(),
            target_id: AgentTargetId(target_id.to_string()),
            source_path: display_path(source_dir),
            destination_path: display_path(destination),
            action,
            detail: detail.map(str::to_string),
        }
    }

    fn unchanged(
        skill: &SelectedSkill<'_>,
        target_id: &str,
        source_dir: &Path,
        destination: &Path,
        state: &DestinationState,
    ) -> Self {
        let DestinationState::LinkedHere { kind } = state else {
            unreachable!("unchanged 只由 LinkedHere 产出");
        };
        Self {
            skill_name: skill.name.to_string(),
            target_id: AgentTargetId(target_id.to_string()),
            source_path: display_path(source_dir),
            destination_path: display_path(destination),
            action: "unchanged",
            detail: Some(format!("{kind}已存在：{}", display_path(destination))),
        }
    }

    fn removed(target_id: &str, destination: &Path, resolved: &Path) -> Self {
        Self {
            skill_name: destination
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("skill")
                .to_string(),
            target_id: AgentTargetId(target_id.to_string()),
            source_path: display_path(resolved),
            destination_path: display_path(destination),
            action: "remove",
            detail: None,
        }
    }

    pub(super) fn to_sync_item(&self) -> SkillSyncItem {
        SkillSyncItem {
            skill_name: self.skill_name.clone(),
            target_id: self.target_id.clone(),
            source_path: self.source_path.clone(),
            destination_path: self.destination_path.clone(),
            action: self.action.to_string(),
            detail: self.detail.clone().unwrap_or_default(),
        }
    }
}

// 冲突条目的事实形态。
pub(super) struct InstallConflict {
    pub(super) skill_name: String,
    pub(super) target_id: String,
    pub(super) source_path: PathBuf,
    pub(super) destination_path: PathBuf,
    pub(super) existing_kind: String,
    pub(super) detail: String,
}

// ---------------------------------------------------------------------------
// 目录链接枚举与路径规范化（安装面的物理细节）
// ---------------------------------------------------------------------------

// 列出一个 skills 目录里的目录链接：(落点, 链接目标)。目录不存在 = 没有链接；
// 其他读取错误必须上报，否则调用方会把部分成功当成成功。
pub(super) fn enumerate_links(skill_dir: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
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
        let Some(resolved) = read_directory_link_target(&destination)? else {
            continue;
        };
        links.push((destination, resolved));
    }
    Ok(links)
}

// 一侧是系统软链接（/var → /private/var）、另一侧不是时，canonicalize 会失配，
// 已安装的链接会既认不出来也删不掉。逐级向上找第一个能规范化的祖先，再补回
// 缺失层级；所有比较路径的调用方都走这里，两侧口径才不会分叉。
pub(super) fn normalize_link_path(path: &Path) -> PathBuf {
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

// 同上，但用于「这个目录是不是我的」这类归属比较：只规范化**父目录**，
// 最后一段保留原样。对一个目录链接调用 canonicalize 会解析到链接的另一端，
// 于是「别名目录」与「被指向的目录」会都等于目标目录 —— 别名会认成自己的
// 目标（此前 `linked_target_id` 就是这么算出「继承自自己」的）。
pub(super) fn normalize_own_dir(path: &Path) -> PathBuf {
    let Some(parent) = path.parent() else {
        return normalize_link_path(path);
    };
    let Some(name) = path.file_name() else {
        return normalize_link_path(path);
    };
    normalize_link_path(parent).join(name)
}

pub(super) fn relative_to_source_root(path: &Path, source_root: &Path) -> Option<PathBuf> {
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

// Windows 路径组件比较：去掉扩展长度前缀后忽略大小写（前缀不影响语义，
// 拼写大小写可能不同），与原先 skills.rs 的实现逐字一致。
#[cfg(windows)]
fn windows_path_component_eq(left: Component<'_>, right: Component<'_>) -> bool {
    let left = left.as_os_str().to_string_lossy();
    let right = right.as_os_str().to_string_lossy();
    let left = left.strip_prefix(r"\\?\").unwrap_or(&left);
    let right = right.strip_prefix(r"\\?\").unwrap_or(&right);

    left.eq_ignore_ascii_case(right)
}

// 软链接目标解析：相对链接基于链接所在目录。
pub(super) fn resolve_symlink_target_path(link_path: &Path, link_target: &Path) -> PathBuf {
    if link_target.is_absolute() {
        return link_target.to_path_buf();
    }
    link_path
        .parent()
        .map(|parent| parent.join(link_target))
        .unwrap_or_else(|| link_target.to_path_buf())
}

// ---------------------------------------------------------------------------
// 测试：单 target 最小夹具（三行 ResolvedTargetConfig + 一个真实源目录），
// 不再手拼整份 config.yaml。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use anyhow::Result;

    use super::*;
    use crate::test_support::TestDir;
    use crate::workspace::types::McpConfigType;

    // 一个全局 target；其余字段与解析结果同形。
    fn target(id: &str, skill_dir: PathBuf) -> ResolvedTargetConfig {
        ResolvedTargetConfig {
            id: AgentTargetId(id.to_string()),
            is_project: false,
            skill_dir,
            config_path: None,
            mcp_config_prefix: String::new(),
            mcp_config_type: McpConfigType::Common,
        }
    }

    fn config_with(targets: Vec<ResolvedTargetConfig>) -> ResolvedManagerConfig {
        ResolvedManagerConfig {
            targets: targets
                .into_iter()
                .map(|target| (target.id.clone(), target))
                .collect::<BTreeMap<_, _>>(),
            projects: BTreeMap::new(),
            skill_sources: BTreeMap::new(),
            mcps: Vec::new(),
        }
    }

    fn write_skill(root: &Path, name: &str) -> Result<PathBuf> {
        let dir = root.join(name);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), name)?;
        Ok(dir)
    }

    // 一个目标：装一次创建链接，再装一次是幂等 noop（已指向本来源）。
    #[test]
    fn install_creates_link_then_reports_unchanged() -> Result<()> {
        let root = TestDir::new("install-apply")?;
        let source_root = root.path().join("source");
        let skill_dir = root.path().join("target/skills");
        write_skill(&source_root, "alpha")?;
        let config = config_with(vec![target("codex", skill_dir.clone())]);
        let selected = [SelectedSkill {
            name: "alpha",
            relative_path: "alpha",
        }];

        let install = SkillInstall::new(&source_root);
        let targets = resolve_ids(&config, &["codex".to_string()])?;
        let participating: Vec<&ManagedTarget<'_>> = targets.iter().collect();

        let applied = install.apply(&participating, &selected)?;
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].action, "create");
        assert_eq!(applied[0].detail, None);
        assert_eq!(
            skill_dir.join("alpha").canonicalize()?,
            source_root.join("alpha").canonicalize()?
        );

        let again = install.apply(&participating, &selected)?;
        assert_eq!(again[0].action, "unchanged");
        assert!(again[0].detail.is_some());

        // 冲突预演对已装好的落点必须静默，否则用户每次同步都会被弹窗拦住。
        assert!(install.conflicts(&participating, &selected)?.is_empty());

        Ok(())
    }

    // 不存在的目标与禁用的目标都产出 skip 原因；调用方据此过滤后目标集合为空。
    // 不存在的目标产出 skip 原因；调用方据此提示并跳过（agent 没有启停状态，
    // 唯一的不参与原因就是「继承自另一个受管目录」）。
    #[test]
    fn missing_targets_are_skipped() -> Result<()> {
        let root = TestDir::new("install-skip")?;
        let source_root = root.path().join("source");
        write_skill(&source_root, "alpha")?;
        let skill_dir = root.path().join("target/skills");
        let config = config_with(vec![target("codex", skill_dir.clone())]);

        let targets = resolve_ids(&config, &["codex".to_string(), "ghost".to_string()])?;
        assert_eq!(targets[0].skip, None);
        assert_eq!(targets[1].skip, Some(TargetSkip::Missing));

        // 与 sync 路径同一段收敛逻辑：只有 skip 为空的目标参与安装。
        let participating: Vec<ManagedTarget<'_>> = targets
            .into_iter()
            .filter(|item| item.skip.is_none())
            .collect();
        assert_eq!(participating.len(), 1);

        let selected = [SelectedSkill {
            name: "alpha",
            relative_path: "alpha",
        }];
        let refs: Vec<&ManagedTarget<'_>> = participating.iter().collect();
        assert_eq!(
            SkillInstall::new(&source_root)
                .apply(&refs, &selected)?
                .len(),
            1
        );
        assert!(skill_dir.join("alpha").symlink_metadata().is_ok());

        Ok(())
    }

    // 落点被真实目录占住时：预演报冲突并说明占位类型，安装走替换。
    #[test]
    fn real_directory_is_reported_as_conflict_then_replaced() -> Result<()> {
        let root = TestDir::new("install-conflict")?;
        let source_root = root.path().join("source");
        let skill_dir = root.path().join("target/skills");
        write_skill(&source_root, "alpha")?;
        let occupied = skill_dir.join("alpha");
        fs::create_dir_all(&occupied)?;
        fs::write(occupied.join("stray.txt"), "not a link")?;
        let config = config_with(vec![target("codex", skill_dir.clone())]);
        let targets = resolve_ids(&config, &["codex".to_string()])?;
        let participating: Vec<&ManagedTarget<'_>> = targets.iter().collect();
        let selected = [SelectedSkill {
            name: "alpha",
            relative_path: "alpha",
        }];
        let install = SkillInstall::new(&source_root);

        let conflicts = install.conflicts(&participating, &selected)?;
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].existing_kind, "目录");
        assert_eq!(conflicts[0].target_id, "codex");

        let applied = install.apply(&participating, &selected)?;
        assert_eq!(applied[0].action, "replace");
        assert_eq!(
            applied[0].detail.as_deref(),
            Some("目标位置已经有真实目录，已替换。")
        );

        Ok(())
    }

    // 指向同来源另一个 skill 的链接：安装判定为冲突（位置不对），卸载按来源清理时删掉。
    #[test]
    fn link_to_other_skill_of_same_source_is_conflict_and_removed() -> Result<()> {
        let root = TestDir::new("install-other-skill")?;
        let source_root = root.path().join("source");
        let beta_dir = write_skill(&source_root, "beta")?;
        write_skill(&source_root, "alpha")?;
        let skill_dir = root.path().join("target/skills");
        fs::create_dir_all(&skill_dir)?;
        let destination = skill_dir.join("alpha");
        create_directory_symlink(&beta_dir, &destination)?;

        let config = config_with(vec![target("codex", skill_dir.clone())]);
        let targets = resolve_ids(&config, &["codex".to_string()])?;
        let participating: Vec<&ManagedTarget<'_>> = targets.iter().collect();
        let selected = [SelectedSkill {
            name: "alpha",
            relative_path: "alpha",
        }];
        let install = SkillInstall::new(&source_root);

        assert_eq!(install.conflicts(&participating, &selected)?.len(), 1);

        // 卸载按「指进本来源」清理，位置不对的链接同样会被删掉（它是本来源的残留）。
        let removed = install.remove_installed(&participating)?;
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].action, "remove");
        assert!(destination.symlink_metadata().is_err());

        Ok(())
    }

    // 项目级 target 的 skill_dir 整层是目录链接时不参与分发：目录内容由链接
    // 的另一端负责，这里不能往它里面装。全局 target 不受此判定（既有行为）。
    #[test]
    fn linked_project_skill_dir_is_skipped() -> Result<()> {
        let root = TestDir::new("install-skip-linked-dir")?;
        let owner_skill_dir = root.path().join("owner/skills");
        fs::create_dir_all(&owner_skill_dir)?;
        let linked_skill_dir = root.path().join("linked/skills");
        fs::create_dir_all(linked_skill_dir.parent().expect("parent"))?;
        create_directory_symlink(&owner_skill_dir, &linked_skill_dir)?;

        let mut project_target = target("claude", linked_skill_dir.clone());
        project_target.is_project = true;
        let config = config_with(vec![
            target("codex", linked_skill_dir.clone()),
            project_target,
        ]);

        let targets = resolve_ids(&config, &["codex".to_string(), "claude".to_string()])?;
        // 全局 target 的链接不触发跳过；项目级触发。
        assert_eq!(targets[0].skip, None);
        assert_eq!(targets[1].skip, Some(TargetSkip::LinkedDir));

        let managed = managed_targets(&config)?;
        assert_eq!(
            managed
                .iter()
                .find(|target| target.id == "claude")
                .expect("claude 应在枚举里")
                .skip,
            Some(TargetSkip::LinkedDir)
        );

        Ok(())
    }

    // 别名落点的归属比较：一个目录链接不能与它指向的目录认成同一个落点、
    // 更不能认成自己。normalize_link_path 会解析链接（用于判断「内容去哪了」），
    // 归属比较必须走 normalize_own_dir（最后一段保留原样）。
    #[test]
    fn own_dir_normalization_keeps_alias_distinct_from_its_target() -> Result<()> {
        let root = TestDir::new("install-alias-ownership")?;
        let owner_skill_dir = root.path().join("owner/skills");
        fs::create_dir_all(&owner_skill_dir)?;
        let alias_skill_dir = root.path().join("project/skills");
        fs::create_dir_all(alias_skill_dir.parent().expect("parent"))?;
        create_directory_symlink(&owner_skill_dir, &alias_skill_dir)?;

        assert_ne!(
            normalize_own_dir(&alias_skill_dir),
            normalize_own_dir(&owner_skill_dir),
            "别名与它的目标必须是两个落点"
        );
        // 父目录被规范化（macOS 上 /var → /private/var），最后一段保留原样。
        assert_eq!(
            normalize_own_dir(&alias_skill_dir),
            alias_skill_dir
                .parent()
                .expect("parent")
                .canonicalize()?
                .join("skills")
        );

        // 对照：穿链接的规范化仍然把两者解析到同一处（内容确实在同一处）。
        assert_eq!(
            normalize_link_path(&alias_skill_dir.canonicalize()?),
            normalize_link_path(&owner_skill_dir)
        );

        Ok(())
    }

    // 项目级落点指向全局 target 的目录时，属主必须报完整地址：只报 agent 名
    // 的话，跨项目就看不出它属于哪个 target（`claude` 到底指哪个）。
    #[test]
    fn alias_owner_is_reported_as_full_address() -> Result<()> {
        let root = TestDir::new("install-alias-owner-address")?;
        let global_skill_dir = root.path().join("global/skills");
        let project_skill_dir = root.path().join("project/skills");
        fs::create_dir_all(&global_skill_dir)?;
        fs::create_dir_all(project_skill_dir.parent().expect("parent"))?;
        create_directory_symlink(&global_skill_dir, &project_skill_dir)?;

        let mut config = config_with(vec![target("claude", global_skill_dir.clone())]);
        config.projects.insert(
            "my-app".to_string(),
            crate::workspace::types::ResolvedProjectConfig {
                id: "my-app".to_string(),
                project_path: root.path().join("project"),
                agents: [(
                    AgentTargetId("claude".to_string()),
                    target("claude", project_skill_dir.clone()),
                )]
                .into_iter()
                .collect(),
            },
        );

        let links = config.skill_dir_links()?;
        let project = links
            .iter()
            .find(|link| link.id == "my-app:claude")
            .expect("项目级落点应在表里");

        assert!(project.is_alias());
        assert_eq!(
            project.linked_target_id.as_deref(),
            Some("claude"),
            "全局 target 的地址就是它的 id"
        );

        // 反向：全局落点是链接、指向项目级目录时，属主报的是复合地址。
        let mut reversed = config_with(vec![target("claude", project_skill_dir.clone())]);
        reversed.projects.insert(
            "my-app".to_string(),
            crate::workspace::types::ResolvedProjectConfig {
                id: "my-app".to_string(),
                project_path: root.path().join("project"),
                agents: [(
                    AgentTargetId("codex".to_string()),
                    target("codex", global_skill_dir),
                )]
                .into_iter()
                .collect(),
            },
        );
        let links = reversed.skill_dir_links()?;
        let global = links
            .iter()
            .find(|link| link.id == "claude")
            .expect("全局落点应在表里");
        assert_eq!(
            global.linked_target_id.as_deref(),
            Some("my-app:codex"),
            "属主是项目级 target 时要报复合地址"
        );

        Ok(())
    }

    // 指向别的来源的链接不属于本来源：卸载不动它。
    #[test]
    fn remove_installed_keeps_other_source_links() -> Result<()> {
        let root = TestDir::new("install-remove-foreign")?;
        let source_root = root.path().join("source");
        write_skill(&source_root, "alpha")?;
        let foreign_root = root.path().join("foreign");
        let foreign_skill = write_skill(&foreign_root, "beta")?;
        let skill_dir = root.path().join("target/skills");
        fs::create_dir_all(&skill_dir)?;
        let destination = skill_dir.join("beta");
        create_directory_symlink(&foreign_skill, &destination)?;

        let config = config_with(vec![target("codex", skill_dir.clone())]);
        let targets = resolve_ids(&config, &["codex".to_string()])?;
        let participating: Vec<&ManagedTarget<'_>> = targets.iter().collect();

        let removed = SkillInstall::new(&source_root).remove_installed(&participating)?;
        assert!(removed.is_empty());
        assert!(destination.symlink_metadata().is_ok());

        Ok(())
    }
}
