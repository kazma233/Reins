import { describe, expect, it } from "vitest";

import type { SyncTargetOption } from "./generated/SyncTargetOption";
import { groupSyncTargets } from "./syncTargetGroups";

function target(id: string, projectId: string | null): SyncTargetOption {
  return {
    id,
    projectId,
    agentId: projectId ? id.slice(projectId.length + 1) : id,
    label: id,
    skillDir: `/targets/${id}`,
    linkedTargetId: null,
    links: [],
  };
}

describe("sync target grouping", () => {
  it("groups by the structured project id", () => {
    const groups = groupSyncTargets([
      target("codex", null),
      target("my-app:claude", "my-app"),
      target("my-app:codex", "my-app"),
      target("other:pi", "other"),
    ]);

    expect(groups.map((group) => group.id)).toEqual(["global", "project:my-app", "project:other"]);
    expect(groups[0].targets.map((item) => item.id)).toEqual(["codex"]);
    expect(groups[1].targets.map((item) => item.id)).toEqual(["my-app:claude", "my-app:codex"]);
  });

  it("keeps a project id containing the separator in one group", () => {
    // 项目 id 里出现分隔符时不能靠拆 id 反推项目名（后端已禁止写入，但历史配置
    // 仍可能读到）：这里锁住「分组只看 projectId 字段」这条不变量。
    const groups = groupSyncTargets([target("a:b:codex", "a:b")]);

    expect(groups).toHaveLength(1);
    expect(groups[0].id).toBe("project:a:b");
    expect(groups[0].label).toBe("项目 a:b");
  });

  it("omits the global group when every target belongs to a project", () => {
    const groups = groupSyncTargets([target("my-app:claude", "my-app")]);

    expect(groups.map((group) => group.id)).toEqual(["project:my-app"]);
  });
});
