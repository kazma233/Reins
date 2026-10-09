// 派生等价快照（候选 6+7）：锁住工具清单现状拷贝的完整输出——workspace
// 两张 defaults 表、默认 config.yaml 模板文本、PROVIDER_APPS 产品名与
// providers 兜底 config_paths。AgentSpec 派生改造前后这些断言必须保持
// 绿，即行为等价证明。两张 defaults 表的 Vec 顺序无消费方依赖，按 id
// 排序后断言；模板是落盘文本，逐字节断言。

use std::collections::BTreeMap;
use std::path::Path;

use crate::providers::apps::ToolEnv;
use crate::providers::commands::app_config_paths;
use crate::providers::types::{PROVIDER_APPS, ProviderAppId};
use crate::test_support::{TestDir, TestEnvGuard};
use crate::workspace::default_config_template;
use crate::workspace::targets::{TargetDefaults, builtin_target_defaults, project_agent_defaults};
use crate::workspace::types::{AgentTargetId, McpConfigType};

fn render_default(item: &TargetDefaults) -> String {
    format!(
        "{}|{}|{}|{}|{:?}",
        item.id.0,
        item.skill_dir.display(),
        item.config_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string()),
        item.config_prefix,
        item.config_type
    )
}

fn render_sorted(items: Vec<TargetDefaults>) -> Vec<String> {
    let sorted: BTreeMap<String, String> = items
        .into_iter()
        .map(|item| (item.id.0.clone(), render_default(&item)))
        .collect();
    sorted.into_values().collect()
}

#[test]
fn snapshot_builtin_target_defaults() {
    let dir = TestDir::new("agents-snap-builtin").unwrap();
    let home = dir.path().to_path_buf();
    let _guard = TestEnvGuard::set_home(&home);

    let expected = render_sorted(vec![
        TargetDefaults {
            id: AgentTargetId("grokbuild".to_string()),
            skill_dir: home.join(".grok/skills"),
            config_path: Some(home.join(".grok/config.toml")),
            config_prefix: "mcp_servers",
            config_type: McpConfigType::GrokBuild,
        },
        TargetDefaults {
            id: AgentTargetId("codex".to_string()),
            skill_dir: home.join(".agents/skills"),
            config_path: Some(home.join(".codex/config.toml")),
            config_prefix: "mcp_servers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("claude".to_string()),
            skill_dir: home.join(".claude/skills"),
            config_path: Some(home.join(".claude.json")),
            config_prefix: "mcpServers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("opencode".to_string()),
            skill_dir: home.join(".config/opencode/skills"),
            config_path: Some(home.join(".config/opencode/opencode.json")),
            config_prefix: "mcp.servers",
            config_type: McpConfigType::OpenCode,
        },
        TargetDefaults {
            id: AgentTargetId("zcode".to_string()),
            skill_dir: home.join(".zcode/skills"),
            config_path: Some(home.join(".zcode/cli/config.json")),
            config_prefix: "mcp.servers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("pi".to_string()),
            skill_dir: home.join(".pi/agent/skills"),
            config_path: Some(home.join(".pi/agent/mcp.json")),
            config_prefix: "mcpServers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("dsh".to_string()),
            skill_dir: home.join(".dsh/skills"),
            config_path: Some(home.join(".dsh/cordis.patch.yml")),
            config_prefix: "",
            config_type: McpConfigType::Dsh,
        },
    ]);

    assert_eq!(render_sorted(builtin_target_defaults()), expected);
}

#[test]
fn snapshot_builtin_target_defaults_with_env_redirect() {
    let dir = TestDir::new("agents-snap-builtin-env").unwrap();
    let home = dir.path().to_path_buf();
    let _guard = TestEnvGuard::set_home(&home);
    let grok_home = dir.path().join("custom-grok");
    let pi_dir = dir.path().join("custom-pi");
    unsafe { std::env::set_var("GROK_HOME", &grok_home) };
    unsafe { std::env::set_var("PI_CODING_AGENT_DIR", &pi_dir) };

    let expected = render_sorted(vec![
        TargetDefaults {
            id: AgentTargetId("grokbuild".to_string()),
            skill_dir: grok_home.join("skills"),
            config_path: Some(grok_home.join("config.toml")),
            config_prefix: "mcp_servers",
            config_type: McpConfigType::GrokBuild,
        },
        TargetDefaults {
            id: AgentTargetId("pi".to_string()),
            skill_dir: pi_dir.join("skills"),
            config_path: Some(pi_dir.join("mcp.json")),
            config_prefix: "mcpServers",
            config_type: McpConfigType::Common,
        },
    ]);

    let redirected: Vec<TargetDefaults> = builtin_target_defaults()
        .into_iter()
        .filter(|item| item.id.0 == "grokbuild" || item.id.0 == "pi")
        .collect();
    assert_eq!(render_sorted(redirected), expected);
}

#[test]
fn snapshot_project_agent_defaults() {
    let project = Path::new("/tmp/reins-snap-project");
    let expected = render_sorted(vec![
        TargetDefaults {
            id: AgentTargetId("grokbuild".to_string()),
            skill_dir: project.join(".grok/skills"),
            config_path: Some(project.join(".grok/config.toml")),
            config_prefix: "mcp_servers",
            config_type: McpConfigType::GrokBuild,
        },
        TargetDefaults {
            id: AgentTargetId("claude".to_string()),
            skill_dir: project.join(".claude/skills"),
            config_path: Some(project.join(".mcp.json")),
            config_prefix: "mcpServers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("codex".to_string()),
            skill_dir: project.join(".agents/skills"),
            config_path: Some(project.join(".codex/config.toml")),
            config_prefix: "mcp_servers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("opencode".to_string()),
            skill_dir: project.join(".opencode/skills"),
            config_path: Some(project.join("opencode.json")),
            config_prefix: "mcp.servers",
            config_type: McpConfigType::OpenCode,
        },
        TargetDefaults {
            id: AgentTargetId("zcode".to_string()),
            skill_dir: project.join(".zcode/skills"),
            config_path: Some(project.join(".zcode/config.json")),
            config_prefix: "mcp.servers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("pi".to_string()),
            skill_dir: project.join(".pi/skills"),
            config_path: Some(project.join(".pi/mcp.json")),
            config_prefix: "mcpServers",
            config_type: McpConfigType::Common,
        },
        TargetDefaults {
            id: AgentTargetId("dsh".to_string()),
            skill_dir: project.join(".dsh/skills"),
            config_path: None,
            config_prefix: "",
            config_type: McpConfigType::Dsh,
        },
    ]);

    assert_eq!(render_sorted(project_agent_defaults(project)), expected);
}

#[test]
fn snapshot_default_config_template() {
    let dir = TestDir::new("agents-snap-template").unwrap();
    let _guard = TestEnvGuard::set_home(dir.path());
    unsafe { std::env::remove_var("PI_CODING_AGENT_DIR") };

    let expected = r#"targets:
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
      config_prefix: mcp.servers
      config_type: opencode

  zcode:
    enabled: true
    skill_dir: ~/.zcode/skills
    mcp:
      enabled: true
      config_path: ~/.zcode/cli/config.json
      config_prefix: mcp.servers
      config_type: common

  # 留空路径以跟随 GROK_HOME；默认 ~/.grok/skills 和 ~/.grok/config.toml。
  grokbuild:
    enabled: true
    skill_dir: ""
    mcp:
      config_prefix: mcp_servers
      config_type: grokbuild

  pi:
    enabled: true
    skill_dir: ~/.pi/agent/skills
    mcp:
      config_path: ~/.pi/agent/mcp.json
      config_prefix: mcpServers
      config_type: common

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
#       grokbuild:
#         enabled: true
"#;

    assert_eq!(default_config_template(), expected);
}

#[test]
fn snapshot_default_config_template_pins_pi_dir() {
    let dir = TestDir::new("agents-snap-template-pi").unwrap();
    let _guard = TestEnvGuard::set_home(dir.path());
    let pi_dir = dir.path().join("custom-pi");
    unsafe { std::env::set_var("PI_CODING_AGENT_DIR", &pi_dir) };

    let template = default_config_template();
    let skill_line = format!("    skill_dir: {}", pi_dir.join("skills").display());
    let mcp_line = format!("      config_path: {}", pi_dir.join("mcp.json").display());
    assert!(template.contains(&skill_line), "模板应固化 pi skills 路径");
    assert!(template.contains(&mcp_line), "模板应固化 pi mcp 路径");
    assert!(
        !template.contains("~/.pi/agent"),
        "PI_CODING_AGENT_DIR 设置时不应再出现 ~ 缩写路径"
    );
}

#[test]
fn snapshot_provider_app_labels() {
    let labels: Vec<(ProviderAppId, &'static str)> = PROVIDER_APPS
        .iter()
        .map(|app| (*app, app.label()))
        .collect();
    assert_eq!(
        labels,
        vec![
            (ProviderAppId::Codex, "Codex"),
            (ProviderAppId::Claude, "Claude Code"),
            (ProviderAppId::Opencode, "OpenCode"),
            (ProviderAppId::Pi, "Pi"),
            (ProviderAppId::Grokbuild, "Grok Build"),
            (ProviderAppId::Dsh, "DeepSeek Harness"),
        ]
    );
}

#[test]
fn snapshot_app_config_paths() {
    let dir = TestDir::new("agents-snap-paths").unwrap();
    let home = dir.path().to_path_buf();
    let _guard = TestEnvGuard::set_home(&home);
    let env = ToolEnv::default();

    let cases: Vec<(ProviderAppId, Vec<String>)> = vec![
        (
            ProviderAppId::Codex,
            vec![home.join(".codex/config.toml").display().to_string()],
        ),
        (
            ProviderAppId::Claude,
            vec![home.join(".claude/settings.json").display().to_string()],
        ),
        (
            ProviderAppId::Opencode,
            vec![
                home.join(".config/opencode/opencode.json")
                    .display()
                    .to_string(),
                home.join(".config/opencode/opencode.jsonc")
                    .display()
                    .to_string(),
            ],
        ),
        (
            ProviderAppId::Pi,
            vec![
                home.join(".pi/agent/models.json").display().to_string(),
                home.join(".pi/agent/settings.json").display().to_string(),
            ],
        ),
        (
            ProviderAppId::Grokbuild,
            vec![home.join(".grok/config.toml").display().to_string()],
        ),
        (
            ProviderAppId::Dsh,
            vec![
                home.join(".dsh/profiles/desktop/cordis.patch.yml")
                    .display()
                    .to_string(),
                home.join(".dsh/.credentials.yaml").display().to_string(),
            ],
        ),
    ];

    for (app, expected) in cases {
        let actual: Vec<String> = app_config_paths(&env, app)
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        assert_eq!(actual, expected, "{app:?}");
    }
}

// AgentSpec 清单自身的一致性：键与各域 id 唯一、PROVIDER_APPS 全覆盖
//（label() 派生的 expect 依赖最后一条成立，新工具只加 PROVIDER_APPS
// 不加 AGENTS 时在这里失败，而不是运行时 panic）。
#[test]
fn agents_registry_is_consistent() {
    use crate::agents::{AGENTS, spec_by_provider_app};
    use std::collections::HashSet;

    let mut keys = HashSet::new();
    let mut targets = HashSet::new();
    let mut providers = HashSet::new();
    let mut wire_ids = HashSet::new();
    for spec in AGENTS {
        assert!(keys.insert(spec.key), "key 重复：{}", spec.key);
        if let Some(target_id) = spec.target_id {
            assert!(targets.insert(target_id), "target_id 重复：{target_id}");
        }
        if let Some(app) = spec.provider_app {
            assert!(providers.insert(app), "provider_app 重复：{app:?}");
        }
        if let Some(source) = spec.source_app {
            assert!(
                wire_ids.insert(source.as_str()),
                "wire id 重复：{}",
                source.as_str()
            );
        }
    }

    for app in PROVIDER_APPS.iter() {
        assert!(
            spec_by_provider_app(*app).is_some(),
            "PROVIDER_APPS 的 {app:?} 未登记 agents 清单"
        );
    }
}
