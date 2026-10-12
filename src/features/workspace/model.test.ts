import { describe, expect, it } from "vitest";
import { AGENT_IDS } from "@shared/lib/agent-labels";
import {
  DEFAULT_PROJECT_FORM,
  formatTargetLabel,
  formatTargetName,
  groupTargetIds,
  parseTargetId,
} from "./model";
import type { AgentTargetId } from "./types";

describe("groupTargetIds", () => {
  it("puts global targets first and each project in its own group", () => {
    const groups = groupTargetIds([
      "codex",
      "web-apps:claude",
      "claude",
      "web-apps:codex",
      "reins:pi",
    ] as AgentTargetId[]);

    expect(groups).toEqual([
      { key: "global", label: "全局", targetIds: ["codex", "claude"] },
      { key: "web-apps", label: "web-apps", targetIds: ["web-apps:claude", "web-apps:codex"] },
      { key: "reins", label: "reins", targetIds: ["reins:pi"] },
    ]);
  });

  it("drops the global group when only project targets are installed", () => {
    const groups = groupTargetIds(["web-apps:claude"] as AgentTargetId[]);

    expect(groups.map((group) => group.key)).toEqual(["web-apps"]);
  });

  it("keeps the input order of agents inside a group", () => {
    const groups = groupTargetIds([
      "web-apps:codex",
      "web-apps:claude",
    ] as AgentTargetId[]);

    expect(groups[0].targetIds).toEqual(["web-apps:codex", "web-apps:claude"]);
  });
});

describe("parseTargetId", () => {
  it("splits a project target into both segments", () => {
    expect(parseTargetId("web-apps:codex")).toEqual({ projectId: "web-apps", agentId: "codex" });
  });

  it("treats an address without a separator as a global target", () => {
    expect(parseTargetId("codex")).toEqual({ projectId: null, agentId: "codex" });
  });

  it("matches the backend for empty segments and extra separators", () => {
    // 这组地址与后端 types.rs::target_ref_rejects_empty_segments 是同一份
    // 用例集，两边必须给出同一答案。
    expect(parseTargetId(":codex")).toEqual({ projectId: null, agentId: ":codex" });
    expect(parseTargetId("demo:")).toEqual({ projectId: null, agentId: "demo:" });
    expect(parseTargetId(":")).toEqual({ projectId: null, agentId: ":" });

    // 第二段含 `:` 后端照样接受：split_once 后两段都非空即为项目地址。
    expect(parseTargetId("a:b:codex")).toEqual({ projectId: "a", agentId: "b:codex" });
  });
});

describe("formatTargetLabel", () => {
  it("renders canonical product names instead of the raw config ids", () => {
    expect(formatTargetLabel("claude")).toBe("Claude Code");
    expect(formatTargetLabel("zcode")).toBe("ZCode");
    expect(formatTargetLabel("dsh")).toBe("DeepSeek Harness");
  });

  it("keeps the project prefix on project targets", () => {
    expect(formatTargetLabel("web-apps:codex")).toBe("web-apps/Codex");
  });

  it("falls back to the raw id for user-defined targets", () => {
    expect(formatTargetLabel("my-gateway")).toBe("my-gateway");
  });
});

describe("formatTargetName", () => {
  it("renders every built-in agent with a canonical name, never the raw id", () => {
    for (const agentId of AGENT_IDS) {
      const label = formatTargetName(agentId);
      expect(label).not.toBe(agentId);
    }
  });
});

describe("DEFAULT_PROJECT_FORM", () => {
  it("starts without any agent enabled", () => {
    expect(DEFAULT_PROJECT_FORM.agents).toEqual([]);
  });
});
