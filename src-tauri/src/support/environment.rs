// 工具全局目录的环境视图：进程环境的唯一读取点，也是「工具 → 重定向变量」
// 映射的唯一来源。agents 清单按 Redirect 匿名解析路径（不知道变量名），
// providers 的适配器经同一视图解析各自配置文件的根。
//
// 生产用 Environment::from_env()；测试用具名字段构造，从而不依赖进程环境。

use std::path::PathBuf;

// 工具目录重定向：每个变体对应一个官方环境变量与未重定向时的默认目录名。
// 只有具名重定向工具需要声明；opencode / zcode 没有官方变量，走 Home 兜底。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Redirect {
    GrokHome,
    PiAgentDir,
    CodexHome,
    ClaudeConfigDir,
    DshHome,
}

impl Redirect {
    // 该重定向对应的官方环境变量。
    fn env_var(self) -> &'static str {
        match self {
            Redirect::GrokHome => "GROK_HOME",
            Redirect::PiAgentDir => "PI_CODING_AGENT_DIR",
            Redirect::CodexHome => "CODEX_HOME",
            Redirect::ClaudeConfigDir => "CLAUDE_CONFIG_DIR",
            Redirect::DshHome => "DSH_HOME",
        }
    }

    // 未重定向时的目录名（相对 HOME）。
    fn default_dir(self) -> &'static str {
        match self {
            Redirect::GrokHome => ".grok",
            Redirect::PiAgentDir => ".pi/agent",
            Redirect::CodexHome => ".codex",
            Redirect::ClaudeConfigDir => ".claude",
            Redirect::DshHome => ".dsh",
        }
    }

    // 错误文案里的工具名（解析失败时用户看到的主语）。
    fn label(self) -> &'static str {
        match self {
            Redirect::GrokHome => "grok",
            Redirect::PiAgentDir => "pi",
            Redirect::CodexHome => "codex",
            Redirect::ClaudeConfigDir => "claude",
            Redirect::DshHome => "dsh",
        }
    }
}

// 一个进程环境里与工具路径相关的取值。重定向字段为 None = 未设置（空串在
// from_env 里已归一为 None）。
#[derive(Clone, Debug, Default)]
pub(crate) struct Environment {
    pub(crate) home: Option<PathBuf>,
    pub(crate) grok_home: Option<PathBuf>,
    pub(crate) pi_agent_dir: Option<PathBuf>,
    pub(crate) codex_home: Option<PathBuf>,
    pub(crate) claude_config_dir: Option<PathBuf>,
    pub(crate) dsh_home: Option<PathBuf>,
}

impl Environment {
    // home 走 user_home_dir()（支持测试 override）；重定向变量空串视为未设置。
    pub(crate) fn from_env() -> Self {
        Self {
            home: crate::support::fs::user_home_dir(),
            grok_home: env_path(Redirect::GrokHome.env_var()),
            pi_agent_dir: env_path(Redirect::PiAgentDir.env_var()),
            codex_home: env_path(Redirect::CodexHome.env_var()),
            claude_config_dir: env_path(Redirect::ClaudeConfigDir.env_var()),
            dsh_home: env_path(Redirect::DshHome.env_var()),
        }
    }

    pub(crate) fn value(&self, redirect: Redirect) -> Option<&PathBuf> {
        match redirect {
            Redirect::GrokHome => self.grok_home.as_ref(),
            Redirect::PiAgentDir => self.pi_agent_dir.as_ref(),
            Redirect::CodexHome => self.codex_home.as_ref(),
            Redirect::ClaudeConfigDir => self.claude_config_dir.as_ref(),
            Redirect::DshHome => self.dsh_home.as_ref(),
        }
    }

    // 重定向优先，HOME 兜底；home 缺失时返回 None。
    pub(crate) fn resolve(&self, redirect: Redirect) -> Option<PathBuf> {
        self.value(redirect).cloned().or_else(|| {
            self.home
                .as_ref()
                .map(|home| home.join(redirect.default_dir()))
        })
    }

    // 同 resolve，但 home 缺失时报错（调用方要一个确定路径时用）。
    pub(crate) fn resolve_required(&self, redirect: Redirect) -> anyhow::Result<PathBuf> {
        self.resolve(redirect)
            .ok_or_else(|| anyhow::anyhow!("无法解析 {} 目录。", redirect.label()))
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// 工具的配置文件路径（相对布局从 agents 清单取，这里只做「根 + 清单相对路径」）
// ---------------------------------------------------------------------------

// 清单路径的唯一解析：`~/` 前缀落在 HOME 下，其余相对 GlobalRoot 的解析结果。
// workspace 的 defaults 表与本文件的 providers 路径都调它，`~/` 语义只有一份。
pub(crate) fn manifest_path(
    env: &Environment,
    root: Option<PathBuf>,
    path: &str,
) -> Option<PathBuf> {
    if let Some(home_relative) = path.strip_prefix("~/") {
        return env.home.as_ref().map(|home| home.join(home_relative));
    }
    root.map(|base| base.join(path))
}

// 某工具在全局清单里声明的 MCP 配置路径（无该入口时 None）。
pub(crate) fn mcp_config_path(
    env: &Environment,
    app: crate::providers::types::ProviderAppId,
) -> Option<PathBuf> {
    let spec = crate::agents::spec_by_provider_app(app)?;
    manifest_path(
        env,
        spec.global.root.resolve(env),
        spec.global.mcp_config_path?,
    )
}
