use super::*;
use std::time::{Duration, UNIX_EPOCH};

#[path = "workspace_grokbuild.rs"]
mod grokbuild;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Result<Self> {
        let path = std::env::temp_dir().join(format!("reins-{prefix}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).ok();
    }
}

fn write_skill(root: &Path, relative_path: &str, body: &str) -> Result<()> {
    let skill_dir = root.join(relative_path);
    fs::create_dir_all(&skill_dir)?;
    fs::write(skill_dir.join("SKILL.md"), body)?;
    Ok(())
}

fn resolved_target(id: &str, skill_dir: PathBuf) -> ResolvedTargetConfig {
    ResolvedTargetConfig {
        id: AgentTargetId(id.to_string()),
        enabled: true,
        is_project: false,
        skill_dir,
        config_path: None,
        mcp_config_prefix: String::new(),
        mcp_config_type: McpConfigType::Common,
    }
}

fn resolved_config(targets: Vec<ResolvedTargetConfig>) -> ResolvedManagerConfig {
    let targets = targets
        .into_iter()
        .map(|target| (target.id.clone(), target))
        .collect();

    ResolvedManagerConfig {
        targets,
        projects: BTreeMap::new(),
        skill_sources: BTreeMap::new(),
        mcps: Vec::new(),
    }
}

fn link_source(id: &str, root_path: PathBuf, include_name_patterns: &[&str]) -> SkillLinkSource {
    SkillLinkSource {
        id: id.to_string(),
        root_path: root_path.canonicalize().unwrap_or(root_path),
        include_name_patterns: include_name_patterns
            .iter()
            .map(|pattern| (*pattern).to_string())
            .collect(),
        include_path_patterns: Vec::new(),
    }
}

#[test]
fn sync_target_options_report_all_link_states() -> Result<()> {
    let root = TestDir::new("sync-target-link-states")?;
    let source_root = root.path().join("source");
    let target_skill_dir = root.path().join("target").join("skills");
    let unmanaged_root = root.path().join("unmanaged");

    write_skill(&source_root, "linked", "linked")?;
    write_skill(&source_root, "excluded", "excluded")?;
    write_skill(&source_root, "removed", "removed")?;
    write_skill(&unmanaged_root, "external", "external")?;
    fs::create_dir_all(&target_skill_dir)?;

    create_directory_symlink(
        &source_root.join("linked"),
        &target_skill_dir.join("linked"),
    )?;
    create_directory_symlink(
        &source_root.join("excluded"),
        &target_skill_dir.join("excluded"),
    )?;
    create_directory_symlink(
        &source_root.join("removed"),
        &target_skill_dir.join("removed"),
    )?;
    create_directory_symlink(
        &unmanaged_root.join("external"),
        &target_skill_dir.join("external"),
    )?;
    fs::remove_dir_all(source_root.join("removed"))?;

    let config = resolved_config(vec![resolved_target("codex", target_skill_dir)]);
    let sources = vec![link_source("local", source_root, &["linked"])];
    let options = build_sync_target_options_with_sources(&config, &sources)?;
    let links = &options[0].links;

    assert_eq!(links.len(), 4, "所有目录链接都应展示");
    assert_eq!(links[0].skill_name, "excluded");
    assert_eq!(links[0].state, SkillLinkState::Excluded);
    assert_eq!(links[0].matched_source_ids, vec!["local"]);
    assert_eq!(links[1].skill_name, "external");
    assert_eq!(links[1].state, SkillLinkState::Unmanaged);
    assert!(links[1].matched_source_ids.is_empty());
    assert_eq!(links[2].skill_name, "linked");
    assert_eq!(links[2].state, SkillLinkState::Linked);
    assert_eq!(links[3].skill_name, "removed");
    assert_eq!(links[3].state, SkillLinkState::SourceMissing);

    Ok(())
}

#[test]
fn sync_target_options_keep_missing_skill_when_source_root_is_linked() -> Result<()> {
    let root = TestDir::new("missing-skill-through-linked-source-root")?;
    let source_root = root.path().join("source");
    let linked_source_root = root.path().join("linked-source");
    let target_skill_dir = root.path().join("target").join("skills");

    write_skill(&source_root, "removed", "removed")?;
    fs::create_dir_all(&target_skill_dir)?;
    create_directory_symlink(&source_root, &linked_source_root)?;
    create_directory_symlink(
        &linked_source_root.join("removed"),
        &target_skill_dir.join("removed"),
    )?;
    fs::remove_dir_all(source_root.join("removed"))?;

    let config = resolved_config(vec![resolved_target("codex", target_skill_dir)]);
    let sources = vec![link_source("local", linked_source_root, &[])];
    let options = build_sync_target_options_with_sources(&config, &sources)?;
    let link = options[0]
        .links
        .first()
        .expect("missing skill link should be found");

    assert_eq!(link.state, SkillLinkState::SourceMissing);
    assert_eq!(link.matched_source_ids, vec!["local"]);

    Ok(())
}

#[test]
fn sync_target_options_keep_all_overlapping_source_matches() -> Result<()> {
    let root = TestDir::new("sync-target-overlapping-sources")?;
    let outer_root = root.path().join("sources");
    let inner_root = outer_root.join("nested");
    let target_skill_dir = root.path().join("target").join("skills");

    write_skill(&inner_root, "alpha", "alpha")?;
    fs::create_dir_all(&target_skill_dir)?;
    create_directory_symlink(&inner_root.join("alpha"), &target_skill_dir.join("alpha"))?;

    let config = resolved_config(vec![resolved_target("codex", target_skill_dir)]);
    let sources = vec![
        link_source("outer", outer_root, &[]),
        link_source("inner", inner_root, &[]),
    ];
    let options = build_sync_target_options_with_sources(&config, &sources)?;
    let link = options[0]
        .links
        .first()
        .expect("skill link should be found");

    assert_eq!(link.state, SkillLinkState::Linked);
    assert_eq!(link.matched_source_ids, vec!["outer", "inner"]);

    Ok(())
}

#[test]
fn sync_target_options_do_not_duplicate_managed_directory_inheritance() -> Result<()> {
    let root = TestDir::new("sync-target-directory-inheritance")?;
    let source_root = root.path().join("source");
    let owner_skill_dir = root.path().join("owner").join("skills");
    let replica_skill_dir = root.path().join("replica").join("skills");

    write_skill(&source_root, "alpha", "alpha")?;
    fs::create_dir_all(&owner_skill_dir)?;
    fs::create_dir_all(replica_skill_dir.parent().expect("replica parent"))?;
    create_directory_symlink(&source_root.join("alpha"), &owner_skill_dir.join("alpha"))?;
    create_directory_symlink(&owner_skill_dir, &replica_skill_dir)?;

    let config = resolved_config(vec![
        resolved_target("owner", owner_skill_dir),
        resolved_target("replica", replica_skill_dir),
    ]);
    let sources = vec![link_source("local", source_root, &[])];
    let options = build_sync_target_options_with_sources(&config, &sources)?;
    let owner = options
        .iter()
        .find(|target| target.id == "owner")
        .expect("owner target should be found");
    let replica = options
        .iter()
        .find(|target| target.id == "replica")
        .expect("replica target should be found");

    assert_eq!(owner.links.len(), 1);
    assert_eq!(replica.linked_target_id.as_deref(), Some("owner"));
    assert!(replica.links.is_empty());

    Ok(())
}

#[cfg(windows)]
#[test]
fn read_directory_link_target_keeps_missing_junction_target() -> Result<()> {
    let root = TestDir::new("missing-junction-target")?;
    let source = root.path().join("source");
    let destination = root.path().join("target").join("alpha");

    fs::create_dir_all(&source)?;
    fs::create_dir_all(destination.parent().expect("destination parent"))?;
    create_directory_symlink(&source, &destination)?;
    fs::remove_dir_all(&source)?;

    let target = read_directory_link_target(&destination)?.expect("junction target should persist");
    assert_eq!(target, source);

    Ok(())
}

#[cfg(unix)]
#[test]
fn inspect_skill_destination_treats_symlink_to_another_skill_dir_as_conflict() -> Result<()> {
    let root = TestDir::new("inspect-symlink")?;
    let source = root.path().join("source");
    let claude_dir = root.path().join("claude").join("skills").join("alpha");
    let codex_destination = root.path().join("codex").join("skills").join("alpha");

    fs::create_dir_all(&source)?;
    fs::write(source.join("SKILL.md"), "alpha")?;
    fs::create_dir_all(&claude_dir)?;
    fs::write(claude_dir.join("SKILL.md"), "alpha")?;
    fs::create_dir_all(codex_destination.parent().unwrap())?;
    std::os::unix::fs::symlink(&claude_dir, &codex_destination)?;

    let (state, _detail) = inspect_skill_destination(&source, &codex_destination);
    assert_eq!(
        state, "conflict",
        "软链接指向另一份同名 skill（来自不同 source）应判 conflict",
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn inspect_skill_destination_treats_symlink_to_empty_dir_as_conflict() -> Result<()> {
    let root = TestDir::new("inspect-symlink-empty")?;
    let source = root.path().join("source");
    let empty_dir = root.path().join("empty");
    let destination = root.path().join("destination");

    fs::create_dir_all(&source)?;
    fs::write(source.join("SKILL.md"), "alpha")?;
    fs::create_dir_all(&empty_dir)?;
    std::os::unix::fs::symlink(&empty_dir, &destination)?;

    let (state, _detail) = inspect_skill_destination(&source, &destination);
    assert_eq!(state, "conflict", "软链接指向空目录应判 conflict");

    Ok(())
}

#[test]
fn create_directory_link_is_detected_as_installed() -> Result<()> {
    let root = TestDir::new("directory-link-detect")?;
    let source = root.path().join("source").join("alpha");
    let destination = root.path().join("target").join("alpha");

    fs::create_dir_all(&source)?;
    fs::write(source.join("SKILL.md"), "alpha")?;
    fs::create_dir_all(destination.parent().unwrap())?;

    create_directory_symlink(&source, &destination)?;

    let link_target = read_skill_dir_link(&destination)?.expect("directory link should exist");
    assert_eq!(link_target, source.canonicalize()?);

    let action = classify_skill_destination(&source, &destination)?;
    assert_eq!(action.kind, SkillActionKind::Unchanged);

    remove_existing_path(&destination)?;
    assert!(destination.symlink_metadata().is_err());
    assert!(source.join("SKILL.md").exists());

    Ok(())
}

#[test]
fn remove_source_links_keeps_source_directory() -> Result<()> {
    let root = TestDir::new("directory-link-cleanup")?;
    let source_root = root.path().join("source");
    let source_skill = source_root.join("alpha");
    let target_skill_dir = root.path().join("target").join("skills");
    let destination = target_skill_dir.join("alpha");

    fs::create_dir_all(&source_skill)?;
    fs::write(source_skill.join("SKILL.md"), "alpha")?;
    fs::create_dir_all(&target_skill_dir)?;
    create_directory_symlink(&source_skill, &destination)?;

    let mut targets = BTreeMap::new();
    targets.insert(
        AgentTargetId("codex".to_string()),
        ResolvedTargetConfig {
            id: AgentTargetId("codex".to_string()),
            enabled: true,
            is_project: false,
            skill_dir: target_skill_dir,
            config_path: None,
            mcp_config_prefix: String::new(),
            mcp_config_type: McpConfigType::Common,
        },
    );
    let config = ResolvedManagerConfig {
        targets,
        projects: BTreeMap::new(),
        skill_sources: BTreeMap::new(),
        mcps: Vec::new(),
    };

    let removed = remove_source_symlinks_from_targets_with_root(&config, &source_root, None)?;

    assert_eq!(removed.len(), 1);
    assert!(destination.symlink_metadata().is_err());
    assert!(source_skill.join("SKILL.md").exists());

    Ok(())
}

// 两个 target + 一个 local 来源的最小 workspace 配置，供链接移除用例复用。
fn write_link_store(root: &TestDir) -> Result<WorkspaceConfigStore> {
    let store = WorkspaceConfigStore::at(root.path());
    let config = serde_json::json!({
        "targets": {
            "codex": {"skill_dir": root.path().join("target/skills").display().to_string()},
            "other": {"skill_dir": root.path().join("other/skills").display().to_string()},
        },
        "skill_sources": [{
            "id": "src",
            "type": "local",
            "root_path": root.path().join("source").display().to_string(),
        }],
    });
    fs::write(store.config_path(), serde_yaml::to_string(&config)?)?;
    Ok(store)
}

// 来源根目录还在、单独 skill 目录被删时，链接目标 canonicalize 会失败并退回
// 原始路径，与规范化后的来源根目录在 /var → /private/var 这类系统软链接下失配，
// 已被识别为待清理的链接会删不掉。
#[test]
fn remove_source_sync_clears_link_whose_skill_directory_is_gone() -> Result<()> {
    let root = TestDir::new("remove-sync-missing-skill")?;
    let source_root = root.path().join("source");
    let target_skill_dir = root.path().join("target").join("skills");
    write_skill(&source_root, "alpha", "alpha")?;
    write_skill(&source_root, "beta", "beta")?;
    fs::create_dir_all(&target_skill_dir)?;
    create_directory_symlink(&source_root.join("alpha"), &target_skill_dir.join("alpha"))?;
    create_directory_symlink(&source_root.join("beta"), &target_skill_dir.join("beta"))?;

    let store = write_link_store(&root)?;
    fs::remove_dir_all(source_root.join("alpha"))?;

    let removed = remove_source_sync_inner(&store, "src", &["codex".to_string()])?.removed;

    assert_eq!(removed.len(), 2);
    assert!(target_skill_dir.join("alpha").symlink_metadata().is_err());
    assert!(target_skill_dir.join("beta").symlink_metadata().is_err());
    assert!(source_root.join("beta/SKILL.md").exists());

    Ok(())
}

// 整个来源目录被删后，来源根目录也 canonicalize 不了；两侧都必须退回同一套
// 规范化规则，否则删除同步会漏掉这些链接。
#[test]
fn remove_source_sync_clears_link_after_source_root_is_deleted() -> Result<()> {
    let root = TestDir::new("remove-sync-deleted-root")?;
    let source_root = root.path().join("source");
    let target_skill_dir = root.path().join("target").join("skills");
    write_skill(&source_root, "alpha", "alpha")?;
    fs::create_dir_all(&target_skill_dir)?;
    create_directory_symlink(&source_root.join("alpha"), &target_skill_dir.join("alpha"))?;

    let store = write_link_store(&root)?;
    fs::remove_dir_all(&source_root)?;

    let removed = remove_source_sync_inner(&store, "src", &["codex".to_string()])?.removed;

    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].skill_name, "alpha");
    assert!(target_skill_dir.join("alpha").symlink_metadata().is_err());

    Ok(())
}

#[test]
fn remove_target_skill_link_only_deletes_links_inside_target_skill_dir() -> Result<()> {
    let root = TestDir::new("remove-single-skill-link")?;
    let source_root = root.path().join("source");
    let target_skill_dir = root.path().join("target").join("skills");
    let other_skill_dir = root.path().join("other").join("skills");
    write_skill(&source_root, "alpha", "alpha")?;
    write_skill(&source_root, "beta", "beta")?;
    fs::create_dir_all(&target_skill_dir)?;
    fs::create_dir_all(&other_skill_dir)?;
    create_directory_symlink(&source_root.join("alpha"), &target_skill_dir.join("alpha"))?;
    create_directory_symlink(&source_root.join("beta"), &other_skill_dir.join("beta"))?;
    fs::create_dir_all(target_skill_dir.join("real-dir"))?;

    let store = write_link_store(&root)?;

    let removed =
        remove_target_skill_link_inner(&store, "codex", &display_path(&target_skill_dir.join("alpha")))?;
    assert_eq!(removed.skill_name, "alpha");
    assert!(target_skill_dir.join("alpha").symlink_metadata().is_err());
    assert!(source_root.join("alpha/SKILL.md").exists());

    // 真实目录不是链接，拒绝且不删除内容。
    assert!(
        remove_target_skill_link_inner(&store, "codex", &display_path(&target_skill_dir.join("real-dir")))
            .is_err()
    );
    assert!(target_skill_dir.join("real-dir").is_dir());

    // 别的 target 目录下的链接不能借用 codex 的 id 删除。
    assert!(
        remove_target_skill_link_inner(&store, "codex", &display_path(&other_skill_dir.join("beta")))
            .is_err()
    );
    assert!(other_skill_dir.join("beta").symlink_metadata().is_ok());

    assert!(
        remove_target_skill_link_inner(&store, "missing", &display_path(&target_skill_dir.join("alpha")))
            .is_err()
    );

    Ok(())
}

// 弹窗把扫描出来的 destination_path 原样回传给单条移除命令，所以展示用的
// 路径字符串必须能通过后端的目录归属校验。
#[test]
fn remove_target_skill_link_accepts_destination_path_from_sync_options() -> Result<()> {
    let root = TestDir::new("remove-link-round-trip")?;
    let unmanaged_root = root.path().join("external");
    let target_skill_dir = root.path().join("target").join("skills");
    write_skill(&unmanaged_root, "external", "external")?;
    fs::create_dir_all(&target_skill_dir)?;
    create_directory_symlink(
        &unmanaged_root.join("external"),
        &target_skill_dir.join("external"),
    )?;

    let store = write_link_store(&root)?;
    let options = build_sync_target_options(&store)?;
    let option = options
        .iter()
        .find(|option| option.id == "codex")
        .expect("codex option exists");
    let link = option.links.first().expect("unmanaged link is scanned");

    assert_eq!(link.state, SkillLinkState::Unmanaged);
    let removed = remove_target_skill_link_inner(&store, &option.id, &link.destination_path)?;

    assert_eq!(removed.skill_name, "external");
    assert!(target_skill_dir.join("external").symlink_metadata().is_err());
    assert!(unmanaged_root.join("external/SKILL.md").exists());

    Ok(())
}

// 目标目录读不出来时必须上报：静默跳过会让调用方把「链接没删掉」当成功。
#[test]
fn remove_source_sync_reports_unreadable_target_directory() -> Result<()> {
    let root = TestDir::new("remove-sync-unreadable-target")?;
    write_skill(&root.path().join("source"), "alpha", "alpha")?;
    // skill_dir 指向一个真实文件：read_dir 会失败，且不是「目录不存在」。
    let broken_skill_dir = root.path().join("target").join("skills");
    fs::create_dir_all(broken_skill_dir.parent().expect("parent exists"))?;
    fs::write(&broken_skill_dir, "not a directory")?;

    let store = write_link_store(&root)?;

    assert!(remove_source_sync_inner(&store, "src", &["codex".to_string()]).is_err());

    Ok(())
}

#[test]
fn parse_batch_git_skill_import_sources_requires_yaml_array() {
    let result = parse_batch_git_skill_import_sources("repo: https://example.com/repo.git");
    assert!(result.is_err());
}

#[test]
fn parse_batch_git_skill_import_sources_rejects_invalid_item() {
    let result = parse_batch_git_skill_import_sources(
        "- repo: https://example.com/repo.git\n  include_name_patterns:\n    - ''\n",
    );
    assert!(result.is_err());
}

#[test]
fn parse_batch_git_skill_import_sources_accepts_git_sources() -> Result<()> {
    let sources = parse_batch_git_skill_import_sources(
        "- repo: https://example.com/repo.git\n  ref: main\n  include_name_patterns:\n    - archive/*\n",
    )?;

    assert_eq!(sources.len(), 1);
    let source = &sources[0];
    assert_eq!(source.repo, "https://example.com/repo.git");
    assert_eq!(source.r#ref, "main");
    assert_eq!(source.include_name_patterns, vec!["archive/*".to_string()]);
    Ok(())
}

#[test]
fn include_patterns_match_name_and_path() -> Result<()> {
    let root = TestDir::new("manager-include-skill-name")?;
    write_skill(root.path(), "skills/beeper", "beeper")?;
    write_skill(root.path(), "skills/agent-helper", "agent-helper")?;

    let mut skills = discover_skills_in_directory(root.path())?;
    apply_include_patterns(&mut skills, &["agent-*".to_string()], &[]);

    let remaining_names = skills
        .into_iter()
        .map(|skill| skill.name)
        .collect::<Vec<_>>();
    assert_eq!(remaining_names, vec!["agent-helper".to_string()]);
    Ok(())
}

#[test]
fn partition_excluded_returns_skills_not_kept() {
    let all = vec![
        DiscoveredSkill {
            name: "beeper".to_string(),
            relative_path: "skills/beeper".to_string(),
            skill_file_path: "/tmp/source/skills/beeper/SKILL.md".to_string(),
            source_path: Some("/tmp/source/skills/beeper".to_string()),
        },
        DiscoveredSkill {
            name: "agent-helper".to_string(),
            relative_path: "skills/agent-helper".to_string(),
            skill_file_path: "/tmp/source/skills/agent-helper/SKILL.md".to_string(),
            source_path: Some("/tmp/source/skills/agent-helper".to_string()),
        },
    ];
    let kept = vec![all[1].clone()];

    let excluded = partition_excluded(&all, &kept);

    assert_eq!(excluded.len(), 1);
    assert_eq!(excluded[0].name, "beeper");
}

#[test]
fn default_config_template_parses_with_builtin_targets() -> Result<()> {
    let config_path = PathBuf::from("/tmp/reins-default-template-test.yaml");
    let config = parse_manager_config(&default_config_template(), &config_path)?;

    assert_eq!(config.targets.len(), 6);
    assert!(
        config
            .targets
            .contains_key(&AgentTargetId("codex".to_string()))
    );
    assert!(
        config
            .targets
            .contains_key(&AgentTargetId("claude".to_string()))
    );
    assert!(
        config
            .targets
            .contains_key(&AgentTargetId("opencode".to_string()))
    );
    assert!(
        config
            .targets
            .contains_key(&AgentTargetId("zcode".to_string()))
    );
    assert!(
        config
            .targets
            .contains_key(&AgentTargetId("pi".to_string()))
    );

    for target in config.targets.values() {
        assert!(target.enabled, "target {} should be enabled", target.id);
        assert!(!target.skill_dir.as_os_str().is_empty());
    }

    let opencode = config
        .targets
        .get(&AgentTargetId("opencode".to_string()))
        .expect("opencode target exists");
    assert!(opencode.skill_dir.ends_with(".config/opencode/skills"));
    assert_eq!(
        opencode.config_path.as_deref(),
        home_dir()
            .map(|home| home.join(".config/opencode/opencode.json"))
            .as_deref()
    );
    assert_eq!(opencode.mcp_config_prefix, "mcp");
    assert_eq!(opencode.mcp_config_type, McpConfigType::OpenCode);

    let zcode = config
        .targets
        .get(&AgentTargetId("zcode".to_string()))
        .expect("zcode target exists");
    assert!(zcode.skill_dir.ends_with(".zcode/skills"));
    assert_eq!(
        zcode.config_path.as_deref(),
        home_dir()
            .map(|home| home.join(".zcode/cli/config.json"))
            .as_deref()
    );
    assert_eq!(zcode.mcp_config_prefix, "mcp.servers");
    assert_eq!(zcode.mcp_config_type, McpConfigType::Common);

    let pi = config
        .targets
        .get(&AgentTargetId("pi".to_string()))
        .expect("pi target exists");
    assert!(pi.skill_dir.ends_with(".pi/agent/skills"));
    // pi 不主动支持 MCP：默认没有配置文件和 configPrefix。
    assert!(pi.config_path.is_none());
    assert_eq!(pi.mcp_config_prefix, "");
    assert_eq!(pi.mcp_config_type, McpConfigType::Common);

    Ok(())
}

#[test]
fn global_pi_target_parses_without_mcp_config() -> Result<()> {
    let config_path = PathBuf::from("/tmp/reins-pi-target-parse-test.yaml");
    let raw = r#"targets:
  pi:
    enabled: true
    skill_dir: ~/.pi/agent/skills
"#;

    let config = parse_manager_config(raw, &config_path)?;
    let pi = config
        .targets
        .get(&AgentTargetId("pi".to_string()))
        .expect("pi target exists");

    assert!(pi.config_path.is_none());
    assert_eq!(pi.mcp_config_prefix, "");

    Ok(())
}

#[test]
fn project_opencode_agent_uses_opencode_project_layout() -> Result<()> {
    let project_path = PathBuf::from("/tmp/reins-opencode-project-layout-test");
    let config_path = PathBuf::from("/tmp/reins-opencode-project-layout-test.yaml");
    let raw = format!(
        r#"projects:
  my-app:
    path: {}
    agents:
      opencode:
        enabled: true
"#,
        project_path.display()
    );

    let config = parse_manager_config(&raw, &config_path)?;
    let project = config.projects.get("my-app").expect("project exists");
    let target = project
        .agents
        .get(&AgentTargetId("opencode".to_string()))
        .expect("opencode project agent exists");
    let mcp_config_path = project_path.join("opencode.json");

    assert_eq!(target.skill_dir, project_path.join(".opencode/skills"));
    assert_eq!(target.config_path.as_ref(), Some(&mcp_config_path));
    assert_eq!(target.mcp_config_prefix, "mcp");
    assert_eq!(target.mcp_config_type, McpConfigType::OpenCode);

    Ok(())
}

#[test]
fn project_zcode_agent_uses_zcode_project_layout() -> Result<()> {
    let project_path = PathBuf::from("/tmp/reins-zcode-project-layout-test");
    let config_path = PathBuf::from("/tmp/reins-zcode-project-layout-test.yaml");
    let raw = format!(
        r#"projects:
  my-app:
    path: {}
    agents:
      zcode:
        enabled: true
"#,
        project_path.display()
    );

    let config = parse_manager_config(&raw, &config_path)?;
    let project = config.projects.get("my-app").expect("project exists");
    let target = project
        .agents
        .get(&AgentTargetId("zcode".to_string()))
        .expect("zcode project agent exists");
    let mcp_config_path = project_path.join(".zcode/config.json");

    assert_eq!(target.skill_dir, project_path.join(".zcode/skills"));
    assert_eq!(target.config_path.as_ref(), Some(&mcp_config_path));
    assert_eq!(target.mcp_config_prefix, "mcp.servers");
    assert_eq!(target.mcp_config_type, McpConfigType::Common);

    Ok(())
}

#[test]
fn delete_workspace_mcp_skips_targets_without_mcp_config() -> Result<()> {
    let root = TestDir::new("mcp-delete-skips-unconfigured")?;
    let store = WorkspaceConfigStore::at(root.path());
    let claude_config = root.path().join("claude.json");
    fs::write(
        store.config_path(),
        format!(
            r#"targets:
  pi:
    enabled: true
    skill_dir: {}
  claude:
    enabled: true
    skill_dir: {}
    mcp:
      enabled: true
      config_path: {}
      config_prefix: mcpServers
      config_type: common
mcps:
- name: test-server
  transport: stdio
  command: node
"#,
            root.path().join("pi-skills").display(),
            root.path().join("claude-skills").display(),
            claude_config.display(),
        ),
    )?;
    fs::write(
        &claude_config,
        r#"{"mcpServers": {"test-server": {"type": "stdio", "command": "node"}}}"#,
    )?;

    let result = delete_workspace_mcp_inner(&store, "test-server")?;

    assert_eq!(result.server_name, "test-server");
    // pi 没有 MCP 配置文件，删除时必须被跳过而不是中断整个删除流程。
    let cleaned = fs::read_to_string(&claude_config)?;
    assert!(!cleaned.contains("test-server"));

    Ok(())
}

#[test]
fn opencode_mcp_entry_preserves_disabled_state() -> Result<()> {
    let server = ResolvedMcpConfig {
        name: "disabled-server".to_string(),
        enabled: false,
        transport: McpTransport::Stdio,
        created_at: None,
        homepage: None,
        command: Some("node".to_string()),
        args: vec!["server.js".to_string()],
        env: BTreeMap::new(),
        url: None,
        headers: BTreeMap::new(),
        timeout: Some(3000),
    };

    let entry = desired_opencode_mcp(&server)?;

    assert_eq!(entry["enabled"], false);
    assert_eq!(entry["type"], "local");
    assert_eq!(entry["command"], serde_json::json!(["node", "server.js"]));
    assert_eq!(entry["timeout"], 3000);

    Ok(())
}

#[test]
fn parse_git_owner_repo_handles_common_url_forms() {
    let cases: &[(&str, &str, &str)] = &[
        ("https://github.com/owner/repo.git", "owner", "repo"),
        ("https://github.com/owner/repo", "owner", "repo"),
        ("git@github.com:owner/repo.git", "owner", "repo"),
        ("ssh://git@github.com/owner/repo.git", "owner", "repo"),
        ("ssh://git@github.com:22/owner/repo.git", "owner", "repo"),
        ("https://gitlab.com/group/sub/repo.git", "sub", "repo"),
        ("owner/repo", "owner", "repo"),
    ];
    for (input, want_owner, want_repo) in cases {
        let (owner, repo) =
            parse_git_owner_repo(input).unwrap_or_else(|| panic!("should parse: {input}"));
        assert_eq!(owner, *want_owner, "owner for {input}");
        assert_eq!(repo, *want_repo, "repo for {input}");
    }
}

#[test]
fn parse_git_owner_repo_rejects_malformed() {
    assert!(parse_git_owner_repo("").is_none());
    assert!(parse_git_owner_repo("just-a-name").is_none());
}

#[test]
fn git_cache_dir_for_repo_uses_owner_name_layout() -> Result<()> {
    let store = WorkspaceConfigStore::at(PathBuf::from("/tmp/reins-cache-layout"));
    let dir = store.git_cache_dir_for_repo("https://github.com/acme/skills.git")?;
    assert!(dir.ends_with("git/acme/skills"));
    assert!(store.git_cache_dir_for_repo("just-a-name").is_err());
    Ok(())
}

#[test]
fn git_cache_last_fetched_at_reads_refresh_marker() -> Result<()> {
    let root = TestDir::new("git-cache-last-fetched-at")?;
    let store = WorkspaceConfigStore::at(root.path());
    let repo = "https://github.com/acme/skills.git";

    assert_eq!(git_cache_last_fetched_at_ms(&store, repo), None);

    let marker_path = store
        .git_cache_dir_for_repo(repo)?
        .join(".git")
        .join("reins-last-fetch");
    fs::create_dir_all(marker_path.parent().expect("marker parent"))?;
    fs::write(&marker_path, "main")?;

    let expected = i64::try_from(
        fs::metadata(&marker_path)?
            .modified()?
            .duration_since(UNIX_EPOCH)?
            .as_millis(),
    )?;
    assert_eq!(git_cache_last_fetched_at_ms(&store, repo), Some(expected));

    Ok(())
}

#[test]
fn git_cache_refresh_respects_24_hour_interval() {
    let now = UNIX_EPOCH + Duration::from_secs(2_000_000);
    let recent = now - Duration::from_secs(24 * 60 * 60 - 1);
    let expired = now - Duration::from_secs(24 * 60 * 60);

    assert!(!git_cache_refresh_is_due(
        Some("main"),
        Some("main"),
        Some(recent),
        now
    ));
    assert!(git_cache_refresh_is_due(
        Some("main"),
        Some("main"),
        Some(expired),
        now
    ));
    assert!(git_cache_refresh_is_due(
        Some("main"),
        Some("release"),
        Some(recent),
        now
    ));
    assert!(git_cache_refresh_is_due(
        None,
        Some("main"),
        Some(recent),
        now
    ));
    assert!(git_cache_refresh_is_due(
        Some("main"),
        Some("main"),
        None,
        now
    ));
    assert!(git_cache_refresh_is_due(
        Some("main"),
        Some("main"),
        Some(now + Duration::from_secs(1)),
        now,
    ));
}

#[cfg(windows)]
#[test]
fn display_path_strips_windows_extended_length_prefix() {
    assert_eq!(
        display_path(Path::new(r"\\?\C:\Users\ly\skills\agent-model-config")),
        r"C:\Users\ly\skills\agent-model-config",
    );
    assert_eq!(
        display_path(Path::new(r"\\?\UNC\server\share\skills")),
        r"\\server\share\skills",
    );
}
