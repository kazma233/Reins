import { computed } from "vue";

import { formatTargetId } from "./model";
import type { WorkspaceInspection } from "./generated/WorkspaceInspection";

export type SkillDirLinkLookup = {
  // 链接指向的原始目的地：界面显示的就是「这个链接挂在哪」。落点不是
  // 软链接时为空串。
  place: string;
  // 配置里的 skills 落点在盘上是否存在：可能是待安装，也可能是路径写错——
  // 界面中性提示，不下结论。
  exists: boolean;
};

// 目标列表的 inspection 行索引：按 target 地址查落点归属与存在性，
// 与后端 inspection 的 id 口径一致（后端也是这么拼的）。
export function buildSkillDirLinkMap(
  inspection: WorkspaceInspection | null,
): Map<string, SkillDirLinkLookup> {
  const map = new Map<string, SkillDirLinkLookup>();
  if (!inspection) return map;
  for (const target of inspection.targets) {
    map.set(target.id, {
      place: target.skillDirLink?.linkTarget ?? "",
      exists: target.skillDirExists,
    });
  }
  for (const project of inspection.projects) {
    for (const agent of project.agents) {
      map.set(formatTargetId(project.id, agent.id), {
        place: agent.skillDirLink?.linkTarget ?? "",
        exists: agent.skillDirExists,
      });
    }
  }
  return map;
}

// 落点归属只给链接目的地：它就是「这个别名挂在哪儿」的答案，属主名字是多余的
// 一层翻译（要看属主，目的地路径里已经写着）。
export function skillDirLinkLookup(
  map: Map<string, SkillDirLinkLookup>,
  targetId: string,
): SkillDirLinkLookup | null {
  return map.get(targetId) ?? null;
}

// 面板里按 target 地址取一次的便捷封装。
export function useSkillDirLinks(inspection: () => WorkspaceInspection | null) {
  const map = computed(() => buildSkillDirLinkMap(inspection()));
  return (targetId: string) => skillDirLinkLookup(map.value, targetId);
}
