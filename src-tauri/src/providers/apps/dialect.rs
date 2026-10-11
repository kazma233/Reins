// 方言（dialect）：归一化枚举（协议、思考档位）在某工具配置文件里的实际
// 拼写知识。每个适配器经 AppAdapter 声明一张，apply 写入、inspect 反读、
// empty_state 下发的映射文案全部从同一张表派生；正反互逆由 roundtrip
// 测试守卫。收编前这些知识散在六份适配器的正反函数对里，dsh 曾因缺一份
// 覆写把 off 档的映射文案下发成 "none"（实际写 "off"）。

use crate::providers::types::{ProviderProtocol, ReasoningLevel};

pub(crate) struct ReasoningDialect {
    // 档位 → 工具拼写。apply 只会拼写 supported_reasoning_levels 内的档位
    // （validate_plan 前置校验），表按需覆盖；查表缺失属静态数据 bug，
    // expect 大声失败。
    pub(crate) spells: &'static [(ReasoningLevel, &'static str)],
    // 写入折叠声明：档位在该工具里折叠成另一档的拼写写入（grok 的
    // max→xhigh），反向不闭合；roundtrip 测试按此豁免（因此仅测试读取）。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) lossy: &'static [(ReasoningLevel, ReasoningLevel)],
}

impl ReasoningDialect {
    pub(crate) fn spell(&self, level: ReasoningLevel) -> &'static str {
        self.spells
            .iter()
            .find(|(known, _)| *known == level)
            .map(|(_, spell)| *spell)
            .expect("方言表必须覆盖该档位")
    }

    // 反读沿 ReasoningLevel::parse 的归一口径（trim + 小写）做 best-effort
    // 识别，认不出的取值留空展示、不报错；工具拼写之外回落归一化拼写，
    // 覆盖 grok 配置层合法但 Reins 不写出的 "max" 等取值，与收编前各
    // 适配器的反读行为一致。
    pub(crate) fn parse(&self, value: &str) -> Option<ReasoningLevel> {
        let normalized = value.trim().to_lowercase();
        if let Some((level, _)) = self.spells.iter().find(|(_, spell)| *spell == normalized) {
            return Some(*level);
        }
        ReasoningLevel::parse(value)
    }
}

pub(crate) struct ProtocolDialect {
    pub(crate) spells: &'static [(ProviderProtocol, &'static str)],
    // 只出现在反读里的别名（如 opencode 的 v1 npm 包名）：正向写入不产出。
    pub(crate) parse_aliases: &'static [(&'static str, ProviderProtocol)],
}

impl ProtocolDialect {
    pub(crate) fn spell(&self, protocol: ProviderProtocol) -> &'static str {
        self.spells
            .iter()
            .find(|(known, _)| *known == protocol)
            .map(|(_, spell)| *spell)
            .expect("方言表必须覆盖该协议")
    }

    pub(crate) fn parse(&self, value: &str) -> Option<ProviderProtocol> {
        if let Some((protocol, _)) = self.spells.iter().find(|(_, spell)| *spell == value) {
            return Some(*protocol);
        }
        self.parse_aliases
            .iter()
            .find(|(alias, _)| *alias == value)
            .map(|(_, protocol)| *protocol)
    }
}

// ---------------------------------------------------------------------------
// 各工具方言表（static：共享方言保证单一实例，roundtrip 测试按地址守卫）
// ---------------------------------------------------------------------------

// 恒等方言：拼写与归一化枚举一致（off 档写 "none"）。
pub(crate) static IDENTITY_REASONING: ReasoningDialect = ReasoningDialect {
    spells: &[
        (ReasoningLevel::Off, "none"),
        (ReasoningLevel::Minimal, "minimal"),
        (ReasoningLevel::Low, "low"),
        (ReasoningLevel::Medium, "medium"),
        (ReasoningLevel::High, "high"),
        (ReasoningLevel::Xhigh, "xhigh"),
        (ReasoningLevel::Max, "max"),
    ],
    lossy: &[],
};

// pi-ai 方言：dsh 的 LLM 路由层就是 llm-pi-ai，与 Pi 共用同一套 KnownApi
// 枚举与思考档位，两个适配器引用同一份实例，防止再长出两份拷贝。
// api 取值按 pi 命名习惯写为 kebab-case（pi-ai 包 types.d.ts）；思考档位
// 首档是 "off"（pi.dev/docs/latest/settings），与归一化集合的 "none"
// 写法不同。
pub(crate) static PI_AI_PROTOCOL: ProtocolDialect = ProtocolDialect {
    spells: &[
        (ProviderProtocol::OpenaiResponses, "openai-responses"),
        (
            ProviderProtocol::OpenaiChatCompletions,
            "openai-completions",
        ),
        (ProviderProtocol::AnthropicMessages, "anthropic-messages"),
    ],
    parse_aliases: &[],
};

pub(crate) static PI_AI_REASONING: ReasoningDialect = ReasoningDialect {
    spells: &[
        (ReasoningLevel::Off, "off"),
        (ReasoningLevel::Minimal, "minimal"),
        (ReasoningLevel::Low, "low"),
        (ReasoningLevel::Medium, "medium"),
        (ReasoningLevel::High, "high"),
        (ReasoningLevel::Xhigh, "xhigh"),
        (ReasoningLevel::Max, "max"),
    ],
    lossy: &[],
};

// Grok 1.0.40 实测 + 官方 settings-reference 确认：api_backend 合法取值为
// chat_completions / responses / messages。
pub(crate) static GROK_PROTOCOL: ProtocolDialect = ProtocolDialect {
    spells: &[
        (ProviderProtocol::OpenaiResponses, "responses"),
        (ProviderProtocol::OpenaiChatCompletions, "chat_completions"),
        (ProviderProtocol::AnthropicMessages, "messages"),
    ],
    parse_aliases: &[],
};

// Grok 的 default_reasoning_effort 配置层接受 max，但交互 UI 只渲染
// low/medium/high/xhigh 四档，且 messages 后端请求层 max 与 xhigh 的
// thinking 参数完全相同（1.0.41 实测）；映射后弹窗所选档位在 Grok 界面
// 可见、请求等价。
pub(crate) static GROK_REASONING: ReasoningDialect = ReasoningDialect {
    spells: &[
        (ReasoningLevel::Off, "none"),
        (ReasoningLevel::Minimal, "minimal"),
        (ReasoningLevel::Low, "low"),
        (ReasoningLevel::Medium, "medium"),
        (ReasoningLevel::High, "high"),
        (ReasoningLevel::Xhigh, "xhigh"),
        (ReasoningLevel::Max, "xhigh"),
    ],
    lossy: &[(ReasoningLevel::Max, ReasoningLevel::Xhigh)],
};

// codex 的 wire_api 目前只有 responses 一种取值（仅接受 openai_responses）。
pub(crate) static CODEX_PROTOCOL: ProtocolDialect = ProtocolDialect {
    spells: &[(ProviderProtocol::OpenaiResponses, "responses")],
    parse_aliases: &[],
};

// OpenCode v2 按协议选择原生运行时包写入：OpenAI 请求格式（chat
// completions）走 openai-compatible，Responses 协议走 openai/responses
// （compatible/responses 有包解析缺陷，不采用），Anthropic 协议走
// anthropic。反读额外识别 v1 遗留 npm 包名与 anthropic-compatible
// （面向第三方兼容端点，仅反读）。
pub(crate) static OPENCODE_PROTOCOL: ProtocolDialect = ProtocolDialect {
    spells: &[
        (
            ProviderProtocol::OpenaiChatCompletions,
            "@opencode/ai/providers/openai-compatible",
        ),
        (
            ProviderProtocol::OpenaiResponses,
            "@opencode/ai/providers/openai/responses",
        ),
        (
            ProviderProtocol::AnthropicMessages,
            "@opencode/ai/providers/anthropic",
        ),
    ],
    parse_aliases: &[
        (
            "@ai-sdk/openai-compatible",
            ProviderProtocol::OpenaiChatCompletions,
        ),
        (
            "@opencode/ai/providers/openai-compatible/responses",
            ProviderProtocol::OpenaiResponses,
        ),
        (
            "@opencode/ai/providers/anthropic-compatible",
            ProviderProtocol::AnthropicMessages,
        ),
        ("@ai-sdk/anthropic", ProviderProtocol::AnthropicMessages),
    ],
};

#[cfg(test)]
#[path = "../../tests/provider_dialects.rs"]
mod provider_dialects;
