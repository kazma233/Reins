import { describe, expect, it } from "vitest";
import { AGENT_ID_ALIASES, AGENT_LABELS } from "./agent-labels";

// 生成物等价断言：spec 表变化时经 pnpm codegen 重新生成，本快照同步
// 更新；两侧不一致即说明生成物过期。
describe("agent-labels (generated)", () => {
  it("labels every managed agent with its product name", () => {
    expect(AGENT_LABELS).toEqual({
      codex: "Codex",
      claude: "Claude Code",
      opencode: "OpenCode",
      zcode: "ZCode",
      grokbuild: "Grok Build",
      pi: "Pi",
      dsh: "DeepSeek Harness",
    });
  });

  it("maps session-source wire ids onto canonical keys", () => {
    expect(AGENT_ID_ALIASES).toEqual({ claude_code: "claude" });
    expect(AGENT_LABELS[AGENT_ID_ALIASES.claude_code]).toBe("Claude Code");
  });
});
