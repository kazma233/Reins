import type { SyncTargetOption } from "./generated/SyncTargetOption";

export type SyncTargetGroup = {
  id: string;
  label: string;
  targets: SyncTargetOption[];
};

// 同步弹窗的目标分组：项目归属直接用后端给的 projectId，不从 id 里拆。
// id 只承担「关联键」职责，此前按第一个冒号反解项目名，项目名里出现分隔符
// 就会静默归错组，且这条假设与展示 label 的格式耦合。
export function groupSyncTargets(targets: SyncTargetOption[]): SyncTargetGroup[] {
  const globalTargets: SyncTargetOption[] = [];
  const projectGroups = new Map<string, SyncTargetOption[]>();
  for (const target of targets) {
    const projectId = target.projectId;
    if (!projectId) {
      globalTargets.push(target);
      continue;
    }
    const existing = projectGroups.get(projectId);
    if (existing) existing.push(target);
    else projectGroups.set(projectId, [target]);
  }

  const groups: SyncTargetGroup[] = [];
  if (globalTargets.length > 0) {
    groups.push({ id: "global", label: "全局目标", targets: globalTargets });
  }
  for (const [projectId, projectTargets] of projectGroups) {
    groups.push({ id: `project:${projectId}`, label: `项目 ${projectId}`, targets: projectTargets });
  }
  return groups;
}
