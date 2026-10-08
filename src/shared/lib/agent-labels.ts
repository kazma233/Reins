// agent 的产品名集中维护：会话来源、workspace target、providers 的界面文案
// 都从这里取，避免同一个工具在不同界面出现 codex / Codex / claude 等不同写法。
const AGENT_LABELS: Record<string, string> = {
  codex: "Codex",
  claude: "Claude Code",
  opencode: "OpenCode",
  pi: "Pi",
  grokbuild: "Grok Build",
  zcode: "ZCode",
  dsh: "DeepSeek Harness",
};

// 各域对同一个工具的 id 写法不同（会话来源用 claude_code，target 用 claude），
// 别名在这里归一到同一个产品名，调用方不需要各自维护映射。
const AGENT_ID_ALIASES: Record<string, string> = {
  claude_code: "claude",
};

// 未知 id（用户自定义 target / provider）原样返回，不编造产品名。
export function agentDisplayName(agentId: string): string {
  const canonicalId = AGENT_ID_ALIASES[agentId] ?? agentId;
  return AGENT_LABELS[canonicalId] ?? agentId;
}
