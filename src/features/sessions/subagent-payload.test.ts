import { describe, expect, it } from "vitest";
import { isFailedSubagentRun, parseSubagentRunPayload } from "./subagent-payload";

describe("subagent run block", () => {
  it("parses runs from the pi backend payload shape", () => {
    const payload = {
      runs: [
        {
          agent: "scout",
          agentSource: "project",
          task: "梳理代码库",
          exitCode: 0,
          usage: { input: 636783, turns: 18 },
          nestedMessages: [
            { id: "m0", role: "user", blocks: [] },
            { id: "m1", role: "assistant", blocks: [] }
          ]
        }
      ]
    };

    const parsed = parseSubagentRunPayload(payload);
    expect(parsed?.runs).toHaveLength(1);
    expect(parsed?.runs[0].agent).toBe("scout");
    expect(parsed?.runs[0].usage?.turns).toBe(18);
    expect(parsed?.runs[0].nestedMessages).toHaveLength(2);
  });

  it("treats any non-zero exit code or error stop reason as a failed run", () => {
    // 入口行的聚合状态与弹窗内每个 run 的状态共用这个判定，不能各自一套
    expect(isFailedSubagentRun({ exitCode: 0, nestedMessages: [] })).toBe(false);
    expect(isFailedSubagentRun({ nestedMessages: [] })).toBe(false);
    expect(isFailedSubagentRun({ exitCode: 1, nestedMessages: [] })).toBe(true);
    expect(isFailedSubagentRun({ exitCode: 127, nestedMessages: [] })).toBe(true);
    expect(
      isFailedSubagentRun({ exitCode: 0, stopReason: "error", nestedMessages: [] })
    ).toBe(true);
  });

  it("rejects payloads without runs", () => {
    expect(parseSubagentRunPayload(null)).toBeNull();
    expect(parseSubagentRunPayload({})).toBeNull();
    expect(parseSubagentRunPayload({ runs: [] })).toBeNull();
  });
});
