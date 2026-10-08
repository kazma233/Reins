use super::*;
use crate::test_support::TestDir;
use crate::workspace::targets::builtin_target_preset_inner;
use serde_json::json;

const DSH_PLUGIN: &str = "@deepseek-ai/dsh-mcp-client";

// 与 grokbuild fixture 同构：dsh target 的 patch 路径落在 TestDir 下，
// MCP 名称/transport/timeout 可注入。
fn dsh_fixture(
    root: &TestDir,
    transport: &str,
    timeout: Option<u64>,
    name: &str,
) -> Result<WorkspaceConfigStore> {
    let store = WorkspaceConfigStore::at(root.path());
    let config = json!({
        "targets": {
            "dsh": {"skill_dir": "dsh/skills", "mcp": {"config_path": "dsh/cordis.patch.yml", "config_type": "dsh"}}
        },
        "mcps": [{"name": name, "enabled": true, "transport": transport,
            "command": "node", "args": ["server.js"], "env": {"VALUE": "1"},
            "url": "https://example.invalid/mcp", "headers": {}, "timeout": timeout}]
    });
    fs::write(store.config_path(), serde_yaml::to_string(&config)?)?;
    Ok(store)
}

fn patch_path(store: &WorkspaceConfigStore) -> PathBuf {
    store
        .config_path()
        .parent()
        .unwrap()
        .join("dsh/cordis.patch.yml")
}

fn parse_ops(path: &Path) -> Result<serde_yaml::Value> {
    Ok(serde_yaml::from_str(&fs::read_to_string(path)?)?)
}

// 找到 Reins 认领条目（name+serverName），返回它的 config mapping；
// 未命中返回 None。
fn claimed_config<'a>(
    ops: &'a serde_yaml::Value,
    server_name: &str,
) -> Option<&'a serde_yaml::Value> {
    let list = ops.as_sequence()?;
    for op in list {
        let Some(entries) = op.get("insert").and_then(|value| value.as_sequence()) else {
            continue;
        };
        for entry in entries {
            let matched = entry.get("name").and_then(|value| value.as_str()) == Some(DSH_PLUGIN)
                && entry
                    .get("config")
                    .and_then(|config| config.get("serverName"))
                    .and_then(|value| value.as_str())
                    == Some(server_name);
            if matched {
                return entry.get("config");
            }
        }
    }
    None
}

#[test]
fn dsh_defaults_preset_and_prefix_relaxation() -> Result<()> {
    let home = home_dir().ok_or_else(|| anyhow!("测试环境缺少 HOME"))?;
    let preset = builtin_target_preset_inner("dsh")?;
    assert_eq!(
        preset.skill_dir,
        home.join(".dsh/skills").display().to_string()
    );
    assert_eq!(
        preset.config_path.as_deref(),
        Some(
            home.join(".dsh/cordis.patch.yml")
                .display()
                .to_string()
                .as_str()
        )
    );
    assert_eq!(preset.mcp_config_prefix, "");
    assert_eq!(preset.mcp_config_type, McpConfigType::Dsh);
    assert_eq!(serde_json::to_string(&McpConfigType::Dsh)?, "\"dsh\"");
    assert_eq!(
        serde_json::from_str::<McpConfigType>("\"dsh\"")?,
        McpConfigType::Dsh
    );

    // config.yaml 里 dsh 允许“有 config_path + 空 prefix”；common 不允许。
    let path = PathBuf::from("/tmp/reins-dsh-defaults.yaml");
    let config = parse_manager_config(
        "targets:\n  dsh:\n    skill_dir: dsh/skills\n    mcp:\n      config_path: dsh/cordis.patch.yml\n",
        &path,
    )?;
    let target = resolve_target_from_id(&config, "dsh").unwrap();
    assert_eq!(target.mcp_config_type, McpConfigType::Dsh);
    assert_eq!(target.mcp_config_prefix, "");
    assert!(parse_manager_config(
        "targets:\n  other:\n    skill_dir: x/skills\n    mcp:\n      config_path: x/config.json\n",
        &path,
    )
    .is_err());
    Ok(())
}

#[test]
fn dsh_project_defaults_are_skills_only() -> Result<()> {
    // dsh 项目层只派生 .dsh/skills;MCP 走全局 Cordis patch,项目级无配置文件。
    let project = Path::new("/tmp/reins-demo");
    let dsh = super::targets::project_agent_defaults(project)
        .into_iter()
        .find(|item| item.id.as_str() == "dsh")
        .expect("dsh 项目默认值应存在");
    assert_eq!(dsh.skill_dir, project.join(".dsh/skills"));
    assert!(dsh.config_path.is_none(), "项目层不应派生 MCP 配置路径");
    assert_eq!(dsh.config_prefix, "");
    assert_eq!(dsh.config_type, McpConfigType::Dsh);
    Ok(())
}

#[test]
fn dsh_create_target_accepts_config_path_without_prefix() -> Result<()> {
    let root = TestDir::new("dsh-create-target")?;
    let store = WorkspaceConfigStore::at(root.path());
    fs::write(store.config_path(), "{}\n")?;

    create_workspace_target_inner(
        &store,
        RawTargetInput {
            target_id: "dsh".to_string(),
            enabled: true,
            skill_dir: "dsh/skills".to_string(),
            config_path: Some("dsh/cordis.patch.yml".to_string()),
            mcp_config_prefix: String::new(),
            mcp_config_type: McpConfigType::Dsh,
        },
    )?;

    let config = store.parse()?;
    let target = resolve_target_from_id(&config, "dsh").unwrap();
    assert_eq!(
        target.config_path,
        Some(root.path().join("dsh/cordis.patch.yml"))
    );
    assert_eq!(target.mcp_config_prefix, "");

    assert!(
        create_workspace_target_inner(
            &store,
            RawTargetInput {
                target_id: "plain".to_string(),
                enabled: true,
                skill_dir: "plain/skills".to_string(),
                config_path: Some("plain/config.json".to_string()),
                mcp_config_prefix: String::new(),
                mcp_config_type: McpConfigType::Common,
            },
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn dsh_apply_creates_patch_file_and_preview_matches() -> Result<()> {
    let root = TestDir::new("dsh-apply-create")?;
    let store = dsh_fixture(&root, "stdio", Some(30_000), "probe")?;
    let path = patch_path(&store);

    let preview = preview_mcp_target_inner(&store, "probe", "dsh")?;
    assert_eq!(preview.format, "yaml");
    let preview_ops: serde_yaml::Value = serde_yaml::from_str(&preview.content)?;
    assert_eq!(preview_ops.as_sequence().map(Vec::len), Some(1));

    apply_mcp_to_target_inner(&store, "probe", "dsh")?;

    let ops = parse_ops(&path)?;
    let config = claimed_config(&ops, "probe").ok_or_else(|| anyhow!("写入后应包含 probe 条目"))?;
    assert_eq!(
        ops.as_sequence().map(Vec::len),
        Some(1),
        "新建文件只包含一个 insert 操作"
    );
    assert_eq!(
        config.get("serverName").and_then(|v| v.as_str()),
        Some("probe")
    );
    assert_eq!(
        config.get("transport").and_then(|v| v.as_str()),
        Some("stdio")
    );
    assert_eq!(config.get("command").and_then(|v| v.as_str()), Some("node"));
    assert_eq!(
        config
            .get("args")
            .and_then(|v| v.as_sequence())
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        config
            .get("env")
            .and_then(|v| v.get("VALUE"))
            .and_then(|v| v.as_str()),
        Some("1")
    );
    assert_eq!(
        config.get("cwd").and_then(|v| v.as_str()),
        home_dir().map(|home| home.display().to_string()).as_deref()
    );
    assert_eq!(
        config.get("toolCallTimeoutMs").and_then(|v| v.as_i64()),
        Some(30_000)
    );
    assert_eq!(
        config.get("failOnStartupError").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert!(
        !fs::read_to_string(&path)?.contains("!!js"),
        "写入的条目不得携带 js 扩展标签"
    );

    let config = store.parse()?;
    let target = resolve_target_from_id(&config, "dsh").unwrap();
    assert!(read_existing_mcp_entries(target, &path)?.contains_key("probe"));

    // preview 展示的操作与落盘内容一致：用户看到的就是将要写入的。
    assert_eq!(preview_ops, ops);
    Ok(())
}

#[test]
fn dsh_apply_merges_into_existing_file_and_preserves_user_entries() -> Result<()> {
    let root = TestDir::new("dsh-merge")?;
    let store = dsh_fixture(&root, "stdio", None, "probe")?;
    let path = patch_path(&store);
    fs::create_dir_all(path.parent().unwrap())?;
    // 用户已有内容：无关插件条目（带 !!js 扩展标签）、未知操作，以及和
    // probe 混在同一个 insert 里的手写条目（不同 id，按 serverName 命中）。
    let existing = r#"- insert:
    - id: memory-memorix
      name: memorix
      config:
        cwd: !!js process.cwd()
        keep: true
- remove: [user-owned-node]
- insert:
    - id: handwritten
      name: '@deepseek-ai/dsh-mcp-client'
      config:
        serverName: probe
        transport: stdio
        command: old-command
        args: []
        env: {}
        cwd: /tmp
        toolCallTimeoutMs: 1000
        failOnStartupError: true
    - id: sibling
      name: other-plugin
      config: {keep: me}
"#;
    fs::write(&path, existing)?;

    apply_mcp_to_target_inner(&store, "probe", "dsh")?;
    let first = fs::read_to_string(&path)?;
    let ops = parse_ops(&path)?;
    assert_eq!(ops.as_sequence().map(Vec::len), Some(3), "不新增操作条目");

    let memorix = ops.as_sequence().unwrap()[0]
        .get("insert")
        .unwrap()
        .as_sequence()
        .unwrap()[0]
        .clone();
    assert_eq!(
        memorix.get("name").and_then(|v| v.as_str()),
        Some("memorix")
    );
    // !!js 标签在字节层面原样保留(serde_yaml 解析会丢标签,所以断言落盘文本
    // 而不是解析结构)。
    assert!(
        first.contains("!!js process.cwd()"),
        "用户条目的 !!js 标签必须原样保留: {first}"
    );
    let keep = memorix
        .get("config")
        .and_then(|config| config.get("keep"))
        .and_then(|v| v.as_bool());
    assert_eq!(keep, Some(true));

    assert!(
        ops.as_sequence().unwrap()[1].get("remove").is_some(),
        "未知操作原样保留"
    );

    let config = claimed_config(&ops, "probe").unwrap();
    assert_eq!(config.get("command").and_then(|v| v.as_str()), Some("node"));
    assert_eq!(
        config.get("toolCallTimeoutMs").and_then(|v| v.as_i64()),
        Some(60_000),
        "未设置超时时写默认 60s"
    );
    let mixed = ops.as_sequence().unwrap()[2]
        .get("insert")
        .unwrap()
        .as_sequence()
        .unwrap();
    assert_eq!(mixed.len(), 2, "同 insert 内的其它条目保留");
    assert_eq!(mixed[1].get("id").and_then(|v| v.as_str()), Some("sibling"));

    // 重复 apply 幂等：文件字节不变。
    apply_mcp_to_target_inner(&store, "probe", "dsh")?;
    assert_eq!(fs::read_to_string(&path)?, first);

    // 更新 MCP 后再 apply：原地改写，不产生第二条目。
    update_workspace_mcp_inner(
        &store,
        "probe",
        RawMcpConfig {
            name: "probe".to_string(),
            enabled: true,
            transport: McpTransport::Stdio,
            created_at: None,
            homepage: None,
            command: Some("deno".to_string()),
            args: vec!["run".to_string()],
            env: BTreeMap::new(),
            url: None,
            headers: BTreeMap::new(),
            timeout: Some(2_000),
        },
    )?;
    apply_mcp_to_target_inner(&store, "probe", "dsh")?;
    let ops = parse_ops(&path)?;
    assert_eq!(ops.as_sequence().map(Vec::len), Some(3));
    let config = claimed_config(&ops, "probe").unwrap();
    assert_eq!(config.get("command").and_then(|v| v.as_str()), Some("deno"));
    assert_eq!(
        config.get("toolCallTimeoutMs").and_then(|v| v.as_i64()),
        Some(2_000)
    );
    Ok(())
}

#[test]
fn dsh_remove_deletes_only_claimed_entries() -> Result<()> {
    let root = TestDir::new("dsh-remove")?;
    let store = dsh_fixture(&root, "stdio", None, "probe")?;
    let path = patch_path(&store);

    // 文件不存在时移除是 noop，且不创建文件。
    let mutation = remove_mcp_from_target_inner(&store, "probe", "dsh")?;
    assert_eq!(mutation.action, "noop");
    assert!(!path.exists());

    apply_mcp_to_target_inner(&store, "probe", "dsh")?;
    let reins_content = fs::read_to_string(&path)?;
    fs::write(
        &path,
        format!(
            "- insert:\n    - id: memory\n      name: memorix\n      config: {{keep: true}}\n{reins_content}"
        ),
    )?;
    assert_eq!(parse_ops(&path)?.as_sequence().map(Vec::len), Some(2));

    remove_mcp_from_target_inner(&store, "probe", "dsh")?;
    let ops = parse_ops(&path)?;
    assert_eq!(ops.as_sequence().map(Vec::len), Some(1));
    assert!(claimed_config(&ops, "probe").is_none());
    assert_eq!(
        ops.as_sequence().unwrap()[0]
            .get("insert")
            .unwrap()
            .as_sequence()
            .unwrap()[0]
            .get("name")
            .and_then(|v| v.as_str()),
        Some("memorix"),
        "用户条目不受移除影响"
    );
    assert_eq!(
        remove_mcp_from_target_inner(&store, "probe", "dsh")?.action,
        "noop"
    );

    // 文件里只剩 Reins 条目时，删除后保留空列表文件。
    let root = TestDir::new("dsh-remove-empty")?;
    let store = dsh_fixture(&root, "stdio", None, "probe")?;
    let path = patch_path(&store);
    apply_mcp_to_target_inner(&store, "probe", "dsh")?;
    remove_mcp_from_target_inner(&store, "probe", "dsh")?;
    assert_eq!(fs::read_to_string(&path)?, "[]\n");
    Ok(())
}

#[test]
fn dsh_server_name_sanitization() -> Result<()> {
    let root = TestDir::new("dsh-sanitize")?;
    let store = dsh_fixture(&root, "stdio", None, "my probe.v2/中文")?;
    let path = patch_path(&store);

    apply_mcp_to_target_inner(&store, "my probe.v2/中文", "dsh")?;
    let ops = parse_ops(&path)?;
    assert!(
        claimed_config(&ops, "my-probe-v2---").is_some(),
        "非法字符折叠为 -"
    );

    let config = store.parse()?;
    let target = resolve_target_from_id(&config, "dsh").unwrap();
    assert_eq!(
        read_existing_mcp_entries(target, &path)?
            .keys()
            .collect::<Vec<_>>(),
        vec!["my-probe-v2---"],
        "读取侧同样按清洗后的 serverName 建键"
    );

    // 超长名称截断到 32 个字符。
    let root = TestDir::new("dsh-sanitize-long")?;
    let long_name = "a".repeat(40);
    let store = dsh_fixture(&root, "stdio", None, &long_name)?;
    let path = patch_path(&store);
    apply_mcp_to_target_inner(&store, &long_name, "dsh")?;
    let ops = parse_ops(&path)?;
    let server_name = ops
        .as_sequence()
        .and_then(|list| list.first())
        .and_then(|op| op.get("insert"))
        .and_then(|v| v.as_sequence())
        .and_then(|entries| entries.first())
        .and_then(|entry| entry.get("config"))
        .and_then(|config| config.get("serverName"))
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(server_name.len(), 32);
    Ok(())
}

#[test]
fn dsh_rejects_http_transport_and_malformed_files_without_writing() -> Result<()> {
    let root = TestDir::new("dsh-invalid")?;
    let store = dsh_fixture(&root, "http", None, "probe")?;
    let path = patch_path(&store);

    for result in [
        preview_mcp_target_inner(&store, "probe", "dsh").map(|_| ()),
        apply_mcp_to_target_inner(&store, "probe", "dsh").map(|_| ()),
    ] {
        assert!(result.unwrap_err().to_string().contains("仅支持 stdio"));
    }
    assert!(!path.exists(), "失败路径不得创建文件");

    let root = TestDir::new("dsh-malformed")?;
    let store = dsh_fixture(&root, "stdio", None, "probe")?;
    let path = patch_path(&store);
    fs::create_dir_all(path.parent().unwrap())?;
    for content in ["model: unchanged\n", "[unclosed\n"] {
        fs::write(&path, content)?;
        assert!(preview_mcp_target_inner(&store, "probe", "dsh").is_err());
        assert!(apply_mcp_to_target_inner(&store, "probe", "dsh").is_err());
        let config = store.parse()?;
        let target = resolve_target_from_id(&config, "dsh").unwrap();
        assert!(read_existing_mcp_entries(target, &path).is_err());
        assert!(remove_mcp_from_target_inner(&store, "probe", "dsh").is_err());
        assert_eq!(fs::read_to_string(&path)?, content);
    }

    // 空文件与空列表都是合法的 patch（按“无操作”处理）。
    for content in ["", "[]\n"] {
        fs::write(&path, content)?;
        apply_mcp_to_target_inner(&store, "probe", "dsh")?;
        assert!(claimed_config(&parse_ops(&path)?, "probe").is_some());
    }
    Ok(())
}
