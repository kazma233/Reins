import { describe, expect, it } from "vitest";

import type { WorkspaceInspection } from "./generated/WorkspaceInspection";
import { buildSkillDirLinkMap, skillDirLinkLookup } from "./skillDirLink";

function inspection(overrides: Partial<WorkspaceInspection> = {}): WorkspaceInspection {
  return {
    configPath: "/home/.reins/config.yaml",
    targets: [],
    mcps: [],
    projects: [],
    warnings: [],
    ...overrides,
  };
}

describe("target list skill dir attribution", () => {
  it("maps global and project targets to their inspection ids", () => {
    const map = buildSkillDirLinkMap(
      inspection({
        targets: [
          {
            id: "codex",
            skillDir: { path: "/home/.codex/skills", exists: true },
            skillDirLink: null,
            configPath: null,
          },
          {
            id: "alias",
            skillDir: { path: "/home/alias/skills", exists: true },
            skillDirLink: { linkedTargetId: "codex", linkTarget: "/home/.codex/skills" },
            configPath: null,
          },
        ],
        projects: [
          {
            id: "my-app",
            path: { path: "/code/my-app", exists: true },
            agents: [
              {
                id: "claude",
                skillDir: { path: "/code/my-app/.claude/skills", exists: true },
                skillDirLink: { linkedTargetId: null, linkTarget: "/external/skills" },
                configPath: null,
              },
            ],
          },
        ],
      }),
    );

    expect(skillDirLinkLookup(map, "codex")).toBeNull();
    expect(skillDirLinkLookup(map, "alias")).toEqual({
      note: "内容属 codex",
      place: "/home/.codex/skills",
    });
    // 项目级按 `project:agent` 取，与后端 inspection 的 id 口径一致；链接另一端
    // 不是受管 target 时也要有说明，否则用户看不到它为什么不参与同步。
    // 叫不出属主时没有归属文案，但链接目的地照样给出：用户仍需要知道它指向哪。
    expect(skillDirLinkLookup(map, "my-app:claude")).toEqual({
      note: null,
      place: "/external/skills",
    });
  });

  it("returns nothing when the inspection is unavailable", () => {
    const map = buildSkillDirLinkMap(null);

    expect(map.size).toBe(0);
    expect(skillDirLinkLookup(map, "codex")).toBeNull();
  });
});
