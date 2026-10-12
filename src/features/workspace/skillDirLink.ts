import { computed } from "vue";

import type { WorkspaceInspection } from "./generated/WorkspaceInspection";
import type { SkillDirLinkView } from "./generated/SkillDirLinkView";

export type SkillDirLinkLookup = {
  // 属主地址（叫不出属主时为空）。
  note: string | null;
  // 链接指向的原始目的地：显示给用户的是「这个链接指向哪里」。
  place: string;
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

// 落点归属：属主地址与链接的原始目的地分开给，界面各自成行——属主回答「内容归
// 谁」，目的地回答「这个链接指向哪里」。不再合成一句话，也不再省略目的地。
export function skillDirLinkLookup(
  map: Map<string, SkillDirLinkView>,
  targetId: string,
): SkillDirLinkLookup | null {
  const link = map.get(targetId);
  if (!link) return null;
  return {
    note: link.linkedTargetId ? `内容属 ${link.linkedTargetId}` : null,
    place: link.linkTarget,
  };
}

// 面板里按 target 地址取一次的便捷封装。
export function useSkillDirLinks(inspection: () => WorkspaceInspection | null) {
  const map = computed(() => buildSkillDirLinkMap(inspection()));
  return (targetId: string) => skillDirLinkLookup(map.value, targetId);
}
