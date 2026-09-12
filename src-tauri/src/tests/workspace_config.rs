use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use super::config::write_atomic;
use super::*;

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

// Seed an isolated store with a config file holding the given local skill
// sources. Local sources keep the test off the network and off the real git
// cache directory.
fn store_with_local_sources(root: &TestDir, source_ids: &[&str]) -> Result<WorkspaceConfigStore> {
    let store = WorkspaceConfigStore::at(root.path());
    let raw = RawManagerConfig {
        skill_sources: source_ids
            .iter()
            .map(|id| RawSkillSourceConfig {
                id: id.to_string(),
                source: RawSkillSourceDefinition::Local {
                    root_path: root.path().join("sources").join(id).display().to_string(),
                    include_name_patterns: Vec::new(),
                    include_path_patterns: Vec::new(),
                },
            })
            .collect(),
        ..RawManagerConfig::default()
    };
    fs::create_dir_all(root.path().join("sources"))?;
    fs::write(store.config_path(), serde_yaml::to_string(&raw)?)?;
    Ok(store)
}

fn read_config_sources(store: &WorkspaceConfigStore) -> Result<Vec<String>> {
    let raw: RawManagerConfig = serde_yaml::from_str(&fs::read_to_string(store.config_path())?)?;
    Ok(raw
        .skill_sources
        .into_iter()
        .map(|source| source.id)
        .collect())
}

#[test]
fn delete_skill_source_inner_removes_entry_from_config_file() -> Result<()> {
    let root = TestDir::new("config-delete-single")?;
    let store = store_with_local_sources(&root, &["source-a", "source-b"])?;

    let state = delete_skill_source_inner(&store, "source-a")?;

    assert_eq!(read_config_sources(&store)?, vec!["source-b".to_string()]);
    let document = state.document.expect("state carries document");
    let config = document.config.expect("config parses after delete");
    assert_eq!(config.skill_sources.len(), 1);
    assert!(
        !fs::read_to_string(store.config_path())?.contains("source-a"),
        "deleted id must be gone from the file on disk"
    );
    Ok(())
}

#[test]
fn delete_skill_source_inner_rejects_unknown_id() -> Result<()> {
    let root = TestDir::new("config-delete-unknown")?;
    let store = store_with_local_sources(&root, &["source-a"])?;

    let error =
        delete_skill_source_inner(&store, "ghost").expect_err("unknown source id must fail");

    assert!(
        error.to_string().contains("未找到来源：ghost"),
        "error message must stay stable, got: {error}"
    );
    assert_eq!(
        read_config_sources(&store)?,
        vec!["source-a".to_string()],
        "failed delete must not touch the file"
    );
    Ok(())
}

#[test]
fn delete_skill_sources_inner_batch_removes_all_in_one_write() -> Result<()> {
    let root = TestDir::new("config-delete-batch")?;
    let store = store_with_local_sources(&root, &["a", "b", "c"])?;

    delete_skill_sources_inner(&store, vec!["a".to_string(), "c".to_string()])?;

    assert_eq!(read_config_sources(&store)?, vec!["b".to_string()]);
    Ok(())
}

#[test]
fn delete_skill_sources_inner_requires_known_ids() -> Result<()> {
    let root = TestDir::new("config-delete-batch-unknown")?;
    let store = store_with_local_sources(&root, &["a"])?;

    delete_skill_sources_inner(&store, vec!["a".to_string(), "ghost".to_string()])
        .expect_err("batch with unknown id must fail");

    assert_eq!(
        read_config_sources(&store)?,
        vec!["a".to_string()],
        "single write-at-end means a failing batch is rolled back"
    );
    Ok(())
}

#[test]
fn delete_skill_sources_inner_rejects_empty_input() -> Result<()> {
    let root = TestDir::new("config-delete-empty")?;
    let store = store_with_local_sources(&root, &["a"])?;

    let error = delete_skill_sources_inner(&store, Vec::new()).expect_err("empty batch must fail");
    assert!(error.to_string().contains("source_ids 不能为空"));
    Ok(())
}

#[test]
fn create_workspace_target_inner_writes_target_to_config() -> Result<()> {
    let root = TestDir::new("config-create-target")?;
    let store = WorkspaceConfigStore::at(root.path());
    fs::write(
        store.config_path(),
        serde_yaml::to_string(&RawManagerConfig::default())?,
    )?;

    let result = create_workspace_target_inner(
        &store,
        RawTargetInput {
            target_id: "cursor".to_string(),
            enabled: true,
            skill_dir: root.path().join("cursor-skills").display().to_string(),
            config_path: Some("~/.cursor/config.json".to_string()),
            mcp_config_prefix: "mcpServers".to_string(),
            mcp_config_type: McpConfigType::Common,
        },
    )?;

    assert_eq!(result.target_id.as_str(), "cursor");
    assert_eq!(
        result.updated_paths,
        vec![store.config_path().display().to_string()]
    );

    let raw: RawManagerConfig = serde_yaml::from_str(&fs::read_to_string(store.config_path())?)?;
    assert!(raw.targets.contains_key("cursor"));
    assert!(raw.targets["cursor"].skill_dir.contains("cursor-skills"));
    Ok(())
}

#[test]
fn create_workspace_target_inner_allows_target_without_mcp_config() -> Result<()> {
    let root = TestDir::new("config-create-target-no-mcp")?;
    let store = WorkspaceConfigStore::at(root.path());
    fs::write(
        store.config_path(),
        serde_yaml::to_string(&RawManagerConfig::default())?,
    )?;

    create_workspace_target_inner(
        &store,
        RawTargetInput {
            target_id: "pi".to_string(),
            enabled: true,
            skill_dir: root.path().join("pi-skills").display().to_string(),
            config_path: None,
            mcp_config_prefix: String::new(),
            mcp_config_type: McpConfigType::Common,
        },
    )?;

    let raw: RawManagerConfig = serde_yaml::from_str(&fs::read_to_string(store.config_path())?)?;
    let pi = &raw.targets["pi"];
    assert!(pi.mcp.config_path.is_none());
    assert!(pi.mcp.config_prefix.is_none());
    Ok(())
}

#[test]
fn create_workspace_target_inner_requires_mcp_path_and_prefix_in_pairs() -> Result<()> {
    let root = TestDir::new("config-create-target-mcp-pair")?;
    let store = WorkspaceConfigStore::at(root.path());
    fs::write(
        store.config_path(),
        serde_yaml::to_string(&RawManagerConfig::default())?,
    )?;

    let missing_prefix = create_workspace_target_inner(
        &store,
        RawTargetInput {
            target_id: "cursor".to_string(),
            enabled: true,
            skill_dir: root.path().join("skills").display().to_string(),
            config_path: Some("~/.cursor/config.json".to_string()),
            mcp_config_prefix: String::new(),
            mcp_config_type: McpConfigType::Common,
        },
    )
    .expect_err("config path without prefix must fail");
    assert!(missing_prefix.to_string().contains("configPrefix"));

    let missing_path = create_workspace_target_inner(
        &store,
        RawTargetInput {
            target_id: "cursor".to_string(),
            enabled: true,
            skill_dir: root.path().join("skills").display().to_string(),
            config_path: None,
            mcp_config_prefix: "mcpServers".to_string(),
            mcp_config_type: McpConfigType::Common,
        },
    )
    .expect_err("prefix without config path must fail");
    assert!(missing_path.to_string().contains("MCP 配置文件路径"));

    Ok(())
}

#[test]
fn concurrent_mutations_do_not_lose_updates() -> Result<()> {
    let root = TestDir::new("config-concurrent")?;
    let store = Arc::new(store_with_local_sources(&root, &[])?);

    // Mimic two overlapping run_blocking commands: each does its own full
    // read-modify-write, so without the store lock one write would clobber
    // the other.
    let left = {
        let store = Arc::clone(&store);
        std::thread::spawn(move || {
            store.locked(|config| {
                let mut raw = config.parse_raw()?;
                raw.skill_sources.push(RawSkillSourceConfig {
                    id: "left".to_string(),
                    source: RawSkillSourceDefinition::Local {
                        root_path: "/tmp/left".to_string(),
                        include_name_patterns: Vec::new(),
                        include_path_patterns: Vec::new(),
                    },
                });
                config.write_raw(&raw)
            })
        })
    };
    let right = {
        let store = Arc::clone(&store);
        std::thread::spawn(move || {
            store.locked(|config| {
                let mut raw = config.parse_raw()?;
                raw.skill_sources.push(RawSkillSourceConfig {
                    id: "right".to_string(),
                    source: RawSkillSourceDefinition::Local {
                        root_path: "/tmp/right".to_string(),
                        include_name_patterns: Vec::new(),
                        include_path_patterns: Vec::new(),
                    },
                });
                config.write_raw(&raw)
            })
        })
    };

    left.join().expect("left thread panicked")?;
    right.join().expect("right thread panicked")?;

    let sources = read_config_sources(&store)?;
    assert!(
        sources.contains(&"left".to_string()) && sources.contains(&"right".to_string()),
        "both mutations must survive, got: {sources:?}"
    );
    Ok(())
}

#[test]
fn write_atomic_leaves_no_temp_files_and_full_content() -> Result<()> {
    let root = TestDir::new("config-atomic")?;
    let target = root.path().join("agent-config.json");

    write_atomic(&target, "{\n  \"first\": 1\n}\n")?;
    write_atomic(&target, "{\n  \"first\": 1,\n  \"second\": 2\n}\n")?;

    assert_eq!(
        fs::read_to_string(&target)?,
        "{\n  \"first\": 1,\n  \"second\": 2\n}\n",
        "second write must fully replace the first"
    );
    let leftovers: Vec<_> = fs::read_dir(root.path())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files must be gone: {leftovers:?}"
    );
    Ok(())
}

#[test]
fn first_load_bootstraps_template_that_parses() -> Result<()> {
    let root = TestDir::new("config-bootstrap")?;
    let store = WorkspaceConfigStore::at(root.path());

    assert!(!store.config_path().exists());
    let document = store.load_document()?;

    assert!(
        store.config_path().exists(),
        "template must be written on first load"
    );
    // First load bootstraps the template and reports exists=true so callers
    // can parse it immediately — no need for a second load to surface config.
    assert!(document.exists);
    assert!(
        document.validation.valid,
        "bootstrapped template must parse"
    );
    assert_eq!(
        document
            .config
            .as_ref()
            .expect("template parses")
            .targets
            .len(),
        5
    );

    let again = store.load_document()?;
    assert_eq!(again.raw_content, document.raw_content);
    assert!(again.exists);
    assert!(again.validation.valid, "bootstrapped template must parse");
    assert_eq!(again.config.expect("template parses").targets.len(), 5);
    Ok(())
}
