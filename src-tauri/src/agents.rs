// agents：reins 管理的全部 AI 编码工具的静态清单（AgentSpec）。一级模块
// 而非业务域：session / workspace / providers 的 defaults 表、模板与文案
// 都从这里派生；本模块只收静态知识，不吸收任何行为，除枚举类型（跨域
// id 映射与布局字段）外不依赖任何业务域。

use std::path::PathBuf;

use crate::providers::types::ProviderAppId;
use crate::session::model::SourceApp;
use crate::workspace::types::McpConfigType;

// 全局布局的路径根：多数工具固定在 HOME 下；grok / pi 支持 env 重定向，
// 解析规则与 support::fs 的 grok_home_path / pi_agent_dir_path 同一口径。
#[derive(Clone, Copy)]
pub(crate) enum GlobalRoot {
    Home,
    GrokHome,
    PiAgentDir,
}

impl GlobalRoot {
    // 解析为实际根目录：env 重定向优先，HOME 兜底。
    pub(crate) fn resolve(self) -> Option<PathBuf> {
        let home = dirs::home_dir();
        match self {
            GlobalRoot::Home => home,
            GlobalRoot::GrokHome => {
                crate::support::fs::grok_home_path(std::env::var_os("GROK_HOME"), home)
            }
            GlobalRoot::PiAgentDir => {
                crate::support::fs::pi_agent_dir_path(std::env::var_os("PI_CODING_AGENT_DIR"), home)
            }
        }
    }

    // 未重定向时的 ~ 缩写前缀（默认模板路径与注释文案用）。
    pub(crate) fn home_prefix(self) -> &'static str {
        match self {
            GlobalRoot::Home => "~",
            GlobalRoot::GrokHome => "~/.grok",
            GlobalRoot::PiAgentDir => "~/.pi/agent",
        }
    }
}

// MCP 配置格式属性，层级无关：同一工具全局与项目的写入格式一致。
pub(crate) struct McpLayout {
    pub(crate) prefix: &'static str,
    pub(crate) config_type: McpConfigType,
}

// 全局布局：skill 目录与 MCP 配置入口都相对 GlobalRoot 根。
pub(crate) struct GlobalLayout {
    pub(crate) root: GlobalRoot,
    pub(crate) skill_dir: &'static str,
    pub(crate) mcp_config_path: Option<&'static str>,
}

// 项目级布局：路径相对项目根。全局与项目非同构，如实表达（claude 的
// MCP 配置在项目根 .mcp.json、opencode 在项目根 opencode.json、dsh 项目
// 级无 MCP 配置入口等）。
pub(crate) struct ProjectLayout {
    pub(crate) skill_dir: &'static str,
    pub(crate) mcp_config_path: Option<&'static str>,
}

// 默认 config.yaml 模板里该工具的条目形态。
pub(crate) enum TemplateEntry {
    // 写全 skill_dir 与 MCP 路径（~ 缩写；pi 在 env 重定向时固化为绝对路径）。
    // mcp.enabled 键现状只有部分条目携带（pi 段没有），如实区分。
    Standard { mcp_enabled: bool },
    // 留空 skill_dir 与 MCP 路径：解析时回落 builtin defaults，跟随
    // GROK_HOME 重定向，避免模板固化错误路径（grokbuild）。
    FollowEnv,
    // 不出现在默认模板（dsh）。
    Absent,
}

// 一个被管理工具的静态描述。
pub(crate) struct AgentSpec {
    // 规范键：各域 id 的公共形式（与 AgentTargetId 一致的连字符小写）；
    // 读者在前端生成物导出（export_bindings_agent_labels），生产路径不按键查询
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) key: &'static str,
    // 产品名：跨层唯一一份展示文案
    pub(crate) label: &'static str,
    // session 域 wire id 映射（wire 写法各自保持现状）；同样只被前端
    // 生成物导出读取
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) source_app: Option<SourceApp>,
    // workspace 域 AgentTargetId
    pub(crate) target_id: Option<&'static str>,
    // providers 域键
    pub(crate) provider_app: Option<ProviderAppId>,
    pub(crate) mcp: McpLayout,
    pub(crate) global: GlobalLayout,
    pub(crate) project: Option<ProjectLayout>,
    pub(crate) template: TemplateEntry,
}

// 顺序即默认 config.yaml 模板的条目顺序；不在模板的 dsh 殿后。
pub(crate) static AGENTS: &[AgentSpec] = &[
    AgentSpec {
        key: "codex",
        label: "Codex",
        source_app: Some(SourceApp::Codex),
        target_id: Some("codex"),
        provider_app: Some(ProviderAppId::Codex),
        mcp: McpLayout {
            prefix: "mcp_servers",
            config_type: McpConfigType::Common,
        },
        global: GlobalLayout {
            root: GlobalRoot::Home,
            skill_dir: ".agents/skills",
            mcp_config_path: Some(".codex/config.toml"),
        },
        project: Some(ProjectLayout {
            skill_dir: ".agents/skills",
            mcp_config_path: Some(".codex/config.toml"),
        }),
        template: TemplateEntry::Standard { mcp_enabled: true },
    },
    AgentSpec {
        key: "claude",
        label: "Claude Code",
        source_app: Some(SourceApp::ClaudeCode),
        target_id: Some("claude"),
        provider_app: Some(ProviderAppId::Claude),
        mcp: McpLayout {
            prefix: "mcpServers",
            config_type: McpConfigType::Common,
        },
        global: GlobalLayout {
            root: GlobalRoot::Home,
            skill_dir: ".claude/skills",
            mcp_config_path: Some(".claude.json"),
        },
        // Claude 的项目级 MCP 配置按官方约定在项目根 .mcp.json。
        project: Some(ProjectLayout {
            skill_dir: ".claude/skills",
            mcp_config_path: Some(".mcp.json"),
        }),
        template: TemplateEntry::Standard { mcp_enabled: true },
    },
    AgentSpec {
        key: "opencode",
        label: "OpenCode",
        source_app: Some(SourceApp::OpenCode),
        target_id: Some("opencode"),
        provider_app: Some(ProviderAppId::Opencode),
        // opencode v2 的 mcp 配置在 mcp.servers 下，顶层 mcp 不被识别。
        mcp: McpLayout {
            prefix: "mcp.servers",
            config_type: McpConfigType::OpenCode,
        },
        global: GlobalLayout {
            root: GlobalRoot::Home,
            skill_dir: ".config/opencode/skills",
            mcp_config_path: Some(".config/opencode/opencode.json"),
        },
        project: Some(ProjectLayout {
            skill_dir: ".opencode/skills",
            mcp_config_path: Some("opencode.json"),
        }),
        template: TemplateEntry::Standard { mcp_enabled: true },
    },
    AgentSpec {
        key: "zcode",
        label: "ZCode",
        source_app: Some(SourceApp::Zcode),
        target_id: Some("zcode"),
        // zcode 只有会话来源与 workspace 分发，不在 providers 域纳管。
        provider_app: None,
        mcp: McpLayout {
            prefix: "mcp.servers",
            config_type: McpConfigType::Common,
        },
        global: GlobalLayout {
            root: GlobalRoot::Home,
            skill_dir: ".zcode/skills",
            mcp_config_path: Some(".zcode/cli/config.json"),
        },
        project: Some(ProjectLayout {
            skill_dir: ".zcode/skills",
            mcp_config_path: Some(".zcode/config.json"),
        }),
        template: TemplateEntry::Standard { mcp_enabled: true },
    },
    AgentSpec {
        key: "grokbuild",
        label: "Grok Build",
        source_app: Some(SourceApp::GrokBuild),
        target_id: Some("grokbuild"),
        provider_app: Some(ProviderAppId::Grokbuild),
        mcp: McpLayout {
            prefix: "mcp_servers",
            config_type: McpConfigType::GrokBuild,
        },
        global: GlobalLayout {
            root: GlobalRoot::GrokHome,
            skill_dir: "skills",
            mcp_config_path: Some("config.toml"),
        },
        project: Some(ProjectLayout {
            skill_dir: ".grok/skills",
            mcp_config_path: Some(".grok/config.toml"),
        }),
        template: TemplateEntry::FollowEnv,
    },
    AgentSpec {
        key: "pi",
        label: "Pi",
        source_app: Some(SourceApp::Pi),
        target_id: Some("pi"),
        provider_app: Some(ProviderAppId::Pi),
        // pi ≥0.99 支持 MCP：配置在 <agentDir>/mcp.json 顶层 mcpServers，
        // 形状与其他 MCP client 一致；legacy SSE transport 不被接受。
        mcp: McpLayout {
            prefix: "mcpServers",
            config_type: McpConfigType::Common,
        },
        global: GlobalLayout {
            root: GlobalRoot::PiAgentDir,
            skill_dir: "skills",
            mcp_config_path: Some("mcp.json"),
        },
        // 项目级 mcp.json 仅在项目被 pi trust 后生效；写入配置本身无害。
        project: Some(ProjectLayout {
            skill_dir: ".pi/skills",
            mcp_config_path: Some(".pi/mcp.json"),
        }),
        template: TemplateEntry::Standard { mcp_enabled: false },
    },
    AgentSpec {
        key: "dsh",
        label: "DeepSeek Harness",
        source_app: Some(SourceApp::Dsh),
        target_id: Some("dsh"),
        provider_app: Some(ProviderAppId::Dsh),
        // dsh 的 MCP 配置是 Cordis patch 操作列表，按 name+serverName
        // 定位条目，没有 configPrefix 概念。
        mcp: McpLayout {
            prefix: "",
            config_type: McpConfigType::Dsh,
        },
        global: GlobalLayout {
            root: GlobalRoot::Home,
            skill_dir: ".dsh/skills",
            mcp_config_path: Some(".dsh/cordis.patch.yml"),
        },
        // dsh 项目层只读 skills（项目 rank 优先于用户级）；MCP 走全局
        // Cordis patch，项目级没有配置文件，是“仅 skill 分发”形态。
        project: Some(ProjectLayout {
            skill_dir: ".dsh/skills",
            mcp_config_path: None,
        }),
        template: TemplateEntry::Absent,
    },
];

pub(crate) fn spec_by_provider_app(app: ProviderAppId) -> Option<&'static AgentSpec> {
    AGENTS.iter().find(|spec| spec.provider_app == Some(app))
}

// ---------------------------------------------------------------------------
// 前端生成物：agent 产品名常量（src/shared/lib/agent-labels.ts）
// ---------------------------------------------------------------------------

// 序列化 AGENTS 为前端常量文件；文件路径保持不变是为了 AGENTS.md 与前端
// 消费点的既有引用继续准确。只被导出测试调用，随测试一起编译。
#[cfg(test)]
fn render_agent_labels_ts() -> String {
    let mut labels = String::new();
    let mut aliases = String::new();
    for spec in AGENTS {
        labels.push_str(&format!("  {}: \"{}\",\n", spec.key, spec.label));
        if let Some(source) = spec.source_app {
            let wire = source.as_str();
            if wire != spec.key {
                aliases.push_str(&format!("  {}: \"{}\",\n", wire, spec.key));
            }
        }
    }
    format!(
        r#"// 自动生成：cargo test export_bindings_agent_labels 触发（pnpm codegen）。
// 来源：src-tauri/src/agents.rs 的 AGENTS 表（工具静态清单单一来源）；请勿手改。

// agent 的产品名集中维护：会话来源、workspace target、providers 的界面文案
// 都从这里取，避免同一个工具在不同界面出现 codex / Codex / claude 等不同写法。
export const AGENT_LABELS: Record<string, string> = {{
{labels}}};

// 跨域 id 映射（wire id → 规范键）：session 来源等 wire 写法与规范键不同时
// 显式列出；其余域 id 与规范键一致，不需要映射。
export const AGENT_ID_ALIASES: Record<string, string> = {{
{aliases}}};
"#
    )
}

#[test]
fn export_bindings_agent_labels() {
    let content = render_agent_labels_ts();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/shared/lib/agent-labels.ts");
    // 与 ts-rs 的 export 测试同一模式：内容有变化时覆写前端生成物。
    let unchanged = matches!(&std::fs::read_to_string(&path), Ok(current) if current == &content);
    if !unchanged {
        std::fs::write(&path, &content).expect("写入 agent-labels.ts 失败");
    }
}
