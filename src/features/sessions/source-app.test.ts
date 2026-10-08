import { describe, expect, it } from "vitest";
import { agentDisplayName } from "@shared/lib/agent-labels";
import { formatSourceAppName } from "./source-app";
import type { SourceApp } from "./types";

// 来源名与 target / providers 共用同一份常量：新增来源时若忘了登记产品名，
// 这里会立刻失败，避免界面回退成裸 id。
const SOURCE_APPS: SourceApp[] = [
  "codex",
  "claude_code",
  "opencode",
  "pi",
  "grokbuild",
  "zcode",
  "dsh",
];

describe("formatSourceAppName", () => {
  it("renders every known source with a canonical name, never the raw id", () => {
    for (const app of SOURCE_APPS) {
      const label = formatSourceAppName(app);
      expect(label).toBe(agentDisplayName(app));
      expect(label).not.toBe(app);
    }
  });
});
