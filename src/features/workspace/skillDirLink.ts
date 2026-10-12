import { computed } from "vue";

import type { WorkspaceInspection } from "./generated/WorkspaceInspection";
import type { SkillDirLinkView } from "./generated/SkillDirLinkView";

export type SkillDirLinkLookup = {
  note: string;
};

// 目标列表上的落点归属：全局 target 用 agent id，项目级用 `project:agent`，
// 与后端 inspection 的 id 口径一致（后端也是这么拼的）。
export function buildSkillDirLinkMap(
  inspection: WorkspaceInspection | null,
): Map<string, SkillDirLinkView> {
  const map = new Map<string, SkillDirLinkView>();
  if (!inspection) return map;
  for (const target of inspection.targets) {
    if (target.skillDirLink) map.set(target.id, target.skillDirLink);
  }
  for (const project of inspection.projects) {
    for (const agent of project.agents) {
      if (agent.skillDirLink) map.set(`${project.id}:${agent.id}`, agent.skillDirLink);
    }
  }
  return map;
}

// 归属文案：能命名就写属主的地址，否则写链接指向的目录。与同步弹窗同一口径
// （都取自后端的落点归属）。行里已经有了自己的落点路径，所以这里不再重复
// 「文件实际在哪」——属主地址本身已经回答了内容归谁。
export function skillDirLinkLookup(
  map: Map<string, SkillDirLinkView>,
  targetId: string,
): SkillDirLinkLookup | null {
  const link = map.get(targetId);
  if (!link) return null;
  return {
    note: link.linkedTargetId ? `内容属 ${link.linkedTargetId}` : `内容在 ${link.linkTarget}`,
  };
}

// 面板里按 target 地址取一次的便捷封装。
export function useSkillDirLinks(inspection: () => WorkspaceInspection | null) {
  const map = computed(() => buildSkillDirLinkMap(inspection()));
  return (targetId: string) => skillDirLinkLookup(map.value, targetId);
}
