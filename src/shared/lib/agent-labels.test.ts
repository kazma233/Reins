import { describe, expect, it } from "vitest";
import { agentDisplayName } from "./agent-labels";

describe("agentDisplayName", () => {
  it("renders the canonical product names", () => {
    expect(agentDisplayName("codex")).toBe("Codex");
    expect(agentDisplayName("claude")).toBe("Claude Code");
    expect(agentDisplayName("opencode")).toBe("OpenCode");
    expect(agentDisplayName("pi")).toBe("Pi");
    expect(agentDisplayName("grokbuild")).toBe("Grok Build");
    expect(agentDisplayName("zcode")).toBe("ZCode");
    expect(agentDisplayName("dsh")).toBe("DeepSeek Harness");
  });

  it("normalizes the session-source alias to the same product name", () => {
    expect(agentDisplayName("claude_code")).toBe(agentDisplayName("claude"));
  });

  it("falls back to the raw id for user-defined agents", () => {
    expect(agentDisplayName("my-gateway")).toBe("my-gateway");
  });
});
