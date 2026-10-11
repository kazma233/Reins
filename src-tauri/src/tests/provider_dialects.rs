// 方言表 roundtrip 守卫：empty_state 的映射文案、apply 写入、inspect 反读
// 三者同源的前提是正反互逆。收编前 dsh 因缺一份覆写把 off 档的映射文案
// 下发成 "none"（实际写 "off"），这里按适配器遍历堵住同类脱钩。

use crate::providers::apps::adapter_for;
use crate::providers::apps::dialect::PI_AI_REASONING;
use crate::providers::types::{ProviderAppId, ProviderProtocol, ReasoningLevel};

const APPS: [ProviderAppId; 6] = [
    ProviderAppId::Codex,
    ProviderAppId::Claude,
    ProviderAppId::Opencode,
    ProviderAppId::Pi,
    ProviderAppId::Grokbuild,
    ProviderAppId::Dsh,
];

// 支持档位的拼写能反读回原档位；有损折叠（grok max→xhigh）按声明豁免。
#[test]
fn reasoning_dialect_roundtrip() {
    for app in APPS {
        let adapter = adapter_for(app);
        let dialect = adapter.reasoning_dialect();
        for level in adapter.capability().supported_reasoning_levels {
            let spelled = dialect.spell(*level);
            let folded = dialect
                .lossy
                .iter()
                .find(|(from, _)| *from == *level)
                .map(|(_, to)| *to);
            let expected = folded.unwrap_or(*level);
            assert_eq!(
                dialect.parse(spelled),
                Some(expected),
                "{app:?} 档位 {level:?} 拼写 {spelled} 反读不闭合"
            );
        }
        assert_eq!(dialect.parse("banana"), None, "{app:?} 未知拼写应留空");
    }
}

// dsh 回归：off 档写入 "off"（pi-ai 方言），映射文案不得再出现 off→none。
#[test]
fn dsh_off_level_spells_off() {
    let dialect = adapter_for(ProviderAppId::Dsh).reasoning_dialect();
    assert_eq!(dialect.spell(ReasoningLevel::Off), "off");
    assert_eq!(dialect.parse("off"), Some(ReasoningLevel::Off));
}

// dsh 的 LLM 层就是 pi-ai：两个适配器必须引用同一份方言常量，防止再长出
// 两份拷贝。
#[test]
fn pi_and_dsh_share_pi_ai_dialect() {
    let pi = adapter_for(ProviderAppId::Pi);
    let dsh = adapter_for(ProviderAppId::Dsh);
    assert!(std::ptr::eq(
        pi.reasoning_dialect(),
        dsh.reasoning_dialect()
    ));
    assert!(std::ptr::eq(
        pi.protocol_dialect().expect("Pi 协议方言必填"),
        dsh.protocol_dialect().expect("dsh 协议方言必填")
    ));
    assert!(std::ptr::eq(dsh.reasoning_dialect(), &PI_AI_REASONING));
}

// 支持协议的拼写能反读回原协议；opencode 的 v1 遗留包名只出现在反读里。
#[test]
fn protocol_dialect_roundtrip_and_aliases() {
    for app in APPS {
        let adapter = adapter_for(app);
        let Some(dialect) = adapter.protocol_dialect() else {
            // Claude 的协议语义固定，无方言可声明。
            assert_eq!(app, ProviderAppId::Claude);
            continue;
        };
        for protocol in adapter.capability().supported_protocols {
            assert_eq!(
                dialect.parse(dialect.spell(*protocol)),
                Some(*protocol),
                "{app:?} 协议 {protocol:?} 拼写反读不闭合"
            );
        }
    }

    let opencode = adapter_for(ProviderAppId::Opencode)
        .protocol_dialect()
        .expect("OpenCode 协议方言必填");
    assert_eq!(
        opencode.parse("@ai-sdk/openai-compatible"),
        Some(ProviderProtocol::OpenaiChatCompletions)
    );
    assert_eq!(
        opencode.parse("@opencode/ai/providers/openai-compatible/responses"),
        Some(ProviderProtocol::OpenaiResponses)
    );
    assert_eq!(
        opencode.parse("@opencode/ai/providers/anthropic-compatible"),
        Some(ProviderProtocol::AnthropicMessages)
    );
    assert_eq!(
        opencode.parse("@ai-sdk/anthropic"),
        Some(ProviderProtocol::AnthropicMessages)
    );
    assert_eq!(opencode.parse("banana"), None);
}
