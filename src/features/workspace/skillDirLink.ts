import { computed } from "vue";

import { formatTargetId } from "./model";
import type { WorkspaceInspection } from "./generated/WorkspaceInspection";
import type { SkillDirLinkView } from "./generated/SkillDirLinkView";

export type SkillDirLinkLookup = {
  // 链接指向的原始目的地：界面显示的就是「这个链接挂在哪」。
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
      if (agent.skillDirLink) map.set(formatTargetId(project.id, agent.id), agent.skillDirLink);
    }
  }
  return map;
}

// 落点归属只给链接目的地：它就是「这个别名挂在哪儿」的答案，属主名字是多余的
// 一层翻译（要看属主，目的地路径里已经写着）。
export function skillDirLinkLookup(
  map: Map<string, SkillDirLinkView>,
  targetId: string,
): SkillDirLinkLookup | null {
  const link = map.get(targetId);
  if (!link) return null;
  return { place: link.linkTarget };
}

// 面板里按 target 地址取一次的便捷封装。
export function useSkillDirLinks(inspection: () => WorkspaceInspection | null) {
  const map = computed(() => buildSkillDirLinkMap(inspection()));
  return (targetId: string) => skillDirLinkLookup(map.value, targetId);
}
