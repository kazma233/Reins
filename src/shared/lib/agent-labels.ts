// 自动生成：cargo test export_bindings_agent_labels 触发（pnpm codegen）。
// 来源：src-tauri/src/agents.rs 的 AGENTS 表（工具静态清单单一来源）；请勿手改。

// agent 的产品名集中维护：会话来源、workspace target、providers 的界面文案
// 都从这里取，避免同一个工具在不同界面出现 codex / Codex / claude 等不同写法。
export const AGENT_LABELS: Record<string, string> = {
  codex: "Codex",
  claude: "Claude Code",
  opencode: "OpenCode",
  zcode: "ZCode",
  grokbuild: "Grok Build",
  pi: "Pi",
  dsh: "DeepSeek Harness",
};

// 跨域 id 映射（wire id → 规范键）：session 来源等 wire 写法与规范键不同时
// 显式列出；其余域 id 与规范键一致，不需要映射。
export const AGENT_ID_ALIASES: Record<string, string> = {
  claude_code: "claude",
};
