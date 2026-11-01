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
            skillDirExists: true,
            skillDirLink: null,
          },
          {
            id: "alias",
            skillDirExists: true,
            skillDirLink: { linkedTargetId: "codex", linkTarget: "/home/.codex/skills" },
          },
        ],
        projects: [
          {
            id: "my-app",
            agents: [
              {
                id: "claude",
                skillDirExists: true,
                skillDirLink: { linkedTargetId: null, linkTarget: "/external/skills" },
              },
            ],
          },
        ],
      }),
    );

    // 无软链接的行也在表里(place 为空):目录不存在的提示靠它,不依赖链接。
    expect(skillDirLinkLookup(map, "codex")).toEqual({ place: "", exists: true });
    expect(skillDirLinkLookup(map, "alias")).toEqual({
      place: "/home/.codex/skills",
      exists: true,
    });
    // 项目级按 `project:agent` 取，与后端 inspection 的 id 口径一致；链接另一端
    // 不是受管 target 时也要有说明，否则用户看不到它为什么不参与同步。
    // 叫不出属主时照样给出链接目的地：它就是「挂在哪」的答案。
    expect(skillDirLinkLookup(map, "my-app:claude")).toEqual({
      place: "/external/skills",
      exists: true,
    });
  });

  it("carries existence so the panel can flag a missing skill dir", () => {
    const map = buildSkillDirLinkMap(
      inspection({
        targets: [
          {
            id: "ghost",
            skillDirExists: false,
            skillDirLink: null,
          },
        ],
      }),
    );

    expect(skillDirLinkLookup(map, "ghost")).toEqual({ place: "", exists: false });
  });

  it("returns nothing when the inspection is unavailable", () => {
    const map = buildSkillDirLinkMap(null);

    expect(map.size).toBe(0);
    expect(skillDirLinkLookup(map, "codex")).toBeNull();
  });
});
