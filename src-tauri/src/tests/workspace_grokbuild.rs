use super::*;
use crate::workspace::targets::builtin_target_preset_inner;
use serde_json::json;

#[test]
fn grokbuild_defaults_template_and_preset_agree() -> Result<()> {
    let path = PathBuf::from("/tmp/reins-grok-defaults.yaml");
    let config = parse_manager_config(&default_config_template(), &path)?;
    let target = resolve_target_from_id(&config, "grokbuild").unwrap();
    let root = std::env::var_os("GROK_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().unwrap().join(".grok"));
    assert_eq!(target.skill_dir, root.join("skills"));
    assert_eq!(target.config_path, Some(root.join("config.toml")));
    assert_eq!(target.mcp_config_prefix, "mcp_servers");
    assert_eq!(target.mcp_config_type, McpConfigType::GrokBuild);
    let preset = builtin_target_preset_inner("grokbuild")?;
    assert_eq!(
        serde_json::to_value(preset)?,
        serde_json::to_value(target_to_view(target))?
    );
    assert!(builtin_target_preset_inner("unknown").is_err());
    assert_eq!(
        serde_json::to_string(&McpConfigType::GrokBuild)?,
        "\"grokbuild\""
    );
    assert_eq!(
        serde_json::from_str::<McpConfigType>("\"grokbuild\"")?,
        McpConfigType::GrokBuild
    );
    assert!(serde_json::from_str::<McpConfigType>("\"grok_build\"").is_err());
    Ok(())
}

#[test]
fn grokbuild_explicit_paths_win_and_old_config_is_unchanged() -> Result<()> {
    let root = TestDir::new("grok-explicit")?;
    let store = WorkspaceConfigStore::at(root.path());
    let raw = "targets:\n  grokbuild:\n    skill_dir: explicit/skills\n    mcp:\n      config_path: explicit/config.toml\n";
    fs::write(store.config_path(), raw)?;
    let config = store.parse()?;
    let target = resolve_target_from_id(&config, "grokbuild").unwrap();
    assert_eq!(target.skill_dir, root.path().join("explicit/skills"));
    assert_eq!(
        target.config_path,
        Some(root.path().join("explicit/config.toml"))
    );
    store.load_document()?;
    assert_eq!(store.read_raw()?, raw);

    let old = "targets:\n  pi:\n    enabled: true\n    skill_dir: isolated/skills\n";
    fs::write(store.config_path(), old)?;
    let document = store.load_document()?;
    assert_eq!(document.config.unwrap().targets.len(), 1);
    assert_eq!(store.read_raw()?, old);
    assert!(resolve_target_from_id(&store.parse()?, "grokbuild").is_none());
    Ok(())
}

fn fixture(root: &TestDir, transport: &str, timeout: Option<u64>) -> Result<WorkspaceConfigStore> {
    let store = WorkspaceConfigStore::at(root.path());
    let config = json!({
        "targets": {
            "grokbuild": {"skill_dir": "global/.grok/skills", "mcp": {"config_path": "global/.grok/config.toml"}},
            "custom": {"skill_dir": "custom/skills", "mcp": {"config_path": "custom/config.toml", "config_prefix": "mcp_servers", "config_type": "grokbuild"}},
            "codex": {"skill_dir": "codex/skills", "mcp": {"config_path": "codex/config.toml"}}
        },
        "projects": {"demo": {"path": "project", "agents": {"grokbuild": {"enabled": true}}}},
        "skill_sources": [
            {"id": "first", "type": "local", "root_path": "source-a"},
            {"id": "second", "type": "local", "root_path": "source-b"}
        ],
        "mcps": [{"name": "probe", "enabled": false, "transport": transport,
            "command": "node", "args": ["server.js"], "env": {"VALUE": "${VAR:-default}"},
            "url": "https://example.invalid/mcp", "headers": {"Authorization": "Bearer ${TOKEN}"}, "timeout": timeout}]
    });
    fs::write(store.config_path(), serde_yaml::to_string(&config)?)?;
    Ok(store)
}

#[test]
fn grokbuild_project_layout_and_skill_link_lifecycle() -> Result<()> {
    let root = TestDir::new("grok-skills")?;
    let store = fixture(&root, "stdio", None)?;
    write_skill(
        &root.path().join("source-a"),
        "probe",
        "---\nname: probe\ndescription: synthetic\n---\nA",
    )?;
    write_skill(
        &root.path().join("source-b"),
        "probe",
        "---\nname: probe\ndescription: synthetic\n---\nB",
    )?;
    let config = store.parse()?;
    let project = resolve_target_from_id(&config, "demo:grokbuild").unwrap();
    assert_eq!(project.skill_dir, root.path().join("project/.grok/skills"));
    assert_eq!(
        project.config_path,
        Some(root.path().join("project/.grok/config.toml"))
    );
    assert_eq!(project.mcp_config_type, McpConfigType::GrokBuild);
    let targets = vec!["grokbuild".to_string(), "demo:grokbuild".to_string()];
    let mut input = SourceSyncInput {
        source_id: "first".into(),
        skill_paths: vec!["probe".into()],
        target_ids: targets.clone(),
        overwrite_existing: false,
        source_root: None,
        skills: None,
    };
    assert_eq!(
        sync_source_to_targets_inner(&store, input.clone())?
            .applied
            .len(),
        2
    );
    for id in &targets {
        let dest = resolve_target_from_id(&config, id)
            .unwrap()
            .skill_dir
            .join("probe");
        assert_eq!(
            read_skill_dir_link(&dest)?,
            Some(root.path().join("source-a/probe").canonicalize()?)
        );
    }
    input.source_id = "second".into();
    assert_eq!(
        preview_source_sync_conflicts_inner(&store, input.clone())?.len(),
        2
    );
    assert!(sync_source_to_targets_inner(&store, input.clone()).is_err());
    input.overwrite_existing = true;
    assert_eq!(
        sync_source_to_targets_inner(&store, input)?.applied.len(),
        2
    );
    assert_eq!(
        remove_source_sync_inner(&store, "second", &targets)?
            .removed
            .len(),
        2
    );
    for id in &targets {
        assert!(
            resolve_target_from_id(&config, id)
                .unwrap()
                .skill_dir
                .join("probe")
                .symlink_metadata()
                .is_err()
        );
    }
    assert!(root.path().join("source-a/probe/SKILL.md").exists());
    assert!(root.path().join("source-b/probe/SKILL.md").exists());
    Ok(())
}

#[test]
fn grokbuild_mcp_preview_apply_read_remove_preserves_unrelated_config() -> Result<()> {
    for transport in ["stdio", "http", "sse"] {
        let root = TestDir::new("grok-mcp")?;
        let store = fixture(&root, transport, Some(2000))?;
        let config = store.parse()?;
        for id in ["grokbuild", "custom", "demo:grokbuild"] {
            let target = resolve_target_from_id(&config, id).unwrap();
            let path = target.config_path.as_ref().unwrap();
            fs::create_dir_all(path.parent().unwrap())?;
            let original: toml::Value =
                toml::from_str("model = 'unchanged'\n[mcp_servers.other]\ncommand = 'keep'\n")?;
            fs::write(path, toml::to_string(&original)?)?;
            let preview = preview_mcp_target_inner(&store, "probe", id)?;
            assert_eq!(preview.format, "toml");
            let entry: toml::Value = toml::from_str(&preview.content)?;
            assert_eq!(entry["enabled"].as_bool(), Some(false));
            assert_eq!(entry["tool_timeout_sec"].as_integer(), Some(2));
            assert!(entry.get("http_headers").is_none());
            if transport == "stdio" {
                assert_eq!(entry["command"].as_str(), Some("node"));
                assert_eq!(entry["args"][0].as_str(), Some("server.js"));
                assert_eq!(entry["env"]["VALUE"].as_str(), Some("${VAR:-default}"));
            } else {
                assert_eq!(entry["url"].as_str(), Some("https://example.invalid/mcp"));
                assert_eq!(
                    entry["headers"]["Authorization"].as_str(),
                    Some("Bearer ${TOKEN}")
                );
            }
            apply_mcp_to_target_inner(&store, "probe", id)?;
            let first = fs::read_to_string(path)?;
            apply_mcp_to_target_inner(&store, "probe", id)?;
            assert_eq!(fs::read_to_string(path)?, first);
            assert_eq!(
                read_existing_mcp_entries(target, path)?["probe"],
                serde_json::to_value(entry)?
            );
            remove_mcp_from_target_inner(&store, "probe", id)?;
            assert_eq!(
                toml::from_str::<toml::Value>(&fs::read_to_string(path)?)?,
                original
            );
            assert_eq!(
                remove_mcp_from_target_inner(&store, "probe", id)?.action,
                "noop"
            );
        }
    }
    Ok(())
}

#[test]
fn grokbuild_timeout_rejects_fractional_seconds_without_changing_codex() -> Result<()> {
    let root = TestDir::new("grok-timeout")?;
    let store = fixture(&root, "http", Some(1500))?;
    assert!(
        preview_mcp_target_inner(&store, "probe", "grokbuild")
            .unwrap_err()
            .to_string()
            .contains("整秒")
    );
    assert!(apply_mcp_to_target_inner(&store, "probe", "grokbuild").is_err());
    assert!(!root.path().join("global/.grok/config.toml").exists());
    let preview = preview_mcp_target_inner(&store, "probe", "codex")?;
    let entry: toml::Value = toml::from_str(&preview.content)?;
    assert_eq!(entry["tool_timeout_sec"].as_float(), Some(1.5));
    assert_eq!(
        entry["http_headers"]["Authorization"].as_str(),
        Some("Bearer ${TOKEN}")
    );
    assert!(entry.get("headers").is_none());
    for timeout in [None, Some(0), Some(u64::MAX - u64::MAX % 1000)] {
        let store = fixture(&root, "stdio", timeout)?;
        let entry: toml::Value =
            toml::from_str(&preview_mcp_target_inner(&store, "probe", "grokbuild")?.content)?;
        assert_eq!(
            entry
                .get("tool_timeout_sec")
                .and_then(toml::Value::as_integer),
            timeout.map(|n| (n / 1000) as i64)
        );
    }
    Ok(())
}

#[test]
fn grokbuild_rejects_json_and_malformed_config_without_writing() -> Result<()> {
    let root = TestDir::new("grok-invalid-format")?;
    let store = fixture(&root, "stdio", None)?;
    let config = store.parse()?;
    let target = resolve_target_from_id(&config, "grokbuild").unwrap();
    let path = target.config_path.as_ref().unwrap();
    fs::create_dir_all(path.parent().unwrap())?;
    for content in ["{\"mcp_servers\":{}}", "[invalid"] {
        fs::write(path, content)?;
        assert!(preview_mcp_target_inner(&store, "probe", "grokbuild").is_err());
        assert!(apply_mcp_to_target_inner(&store, "probe", "grokbuild").is_err());
        assert!(read_existing_mcp_entries(target, path).is_err());
        assert!(remove_mcp_from_target_inner(&store, "probe", "grokbuild").is_err());
        assert_eq!(fs::read_to_string(path)?, content);
    }
    Ok(())
}
