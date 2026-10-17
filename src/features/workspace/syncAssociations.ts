import type { SkillLinkAssociation, SkillSourceConfigView, SyncTargetOption } from "./types";

export type SkillLinkStateInfo = {
  label: string;
  tone: "linked" | "cleanup" | "unmanaged";
  pendingCleanup: boolean;
};

const STATE_INFO: Record<SkillLinkAssociation["state"], SkillLinkStateInfo> = {
  linked: { label: "已关联", tone: "linked", pendingCleanup: false },
  excluded: { label: "待清理: 已排除", tone: "cleanup", pendingCleanup: true },
  sourceMissing: { label: "待清理: 源已删除", tone: "cleanup", pendingCleanup: true },
  unmanaged: { label: "非受管来源", tone: "unmanaged", pendingCleanup: false },
};

export function skillLinkStateInfo(state: SkillLinkAssociation["state"]): SkillLinkStateInfo {
  return STATE_INFO[state];
}

export function skillLinkSourceLabel(
  link: SkillLinkAssociation,
  currentSourceId?: string,
  sourceLabels: Record<string, string> = {},
): string {
  if (link.matchedSourceIds.length === 0) return "";

  const getSourceLabel = (sourceId: string) => sourceLabels[sourceId] ?? sourceId;

  if (currentSourceId && link.matchedSourceIds.includes(currentSourceId)) {
    const otherSourceIds = link.matchedSourceIds.filter((sourceId) => sourceId !== currentSourceId);
    if (otherSourceIds.length === 0) {
      // 纯当前来源的链接:行底色已标出归属,不再重复一行「当前来源」。
      // 混合归属时仍要报出其它来源的名字,那是别处看不到的信息。
      return "";
    }
    return `当前来源 · ${otherSourceIds.map(getSourceLabel).join("、")}`;
  }

  return link.matchedSourceIds.map(getSourceLabel).join("、");
}

function gitOwnerRepo(repo: string): string {
  const normalized = repo
    .trim()
    .replace(/[\\/]+$/, "")
    .replace(/\.git$/i, "");
  const parts = normalized.split(/[\\/:]/).filter(Boolean);
  return parts.length >= 2 ? `${parts[parts.length - 2]}/${parts[parts.length - 1]}` : normalized;
}

export function buildSkillLinkSourceLabels(
  sources: SkillSourceConfigView[],
): Record<string, string> {
  return Object.fromEntries(
    sources.map((source) => [source.id, source.type === "git" ? gitOwnerRepo(source.repo) : source.label]),
  );
}

export function hasUnmanagedSkillLinks(links: SkillLinkAssociation[]): boolean {
  return links.some((link) => link.state === "unmanaged");
}

export function hasCurrentSourceSkillLinks(
  links: SkillLinkAssociation[],
  currentSourceId?: string,
): boolean {
  return Boolean(
    currentSourceId && links.some((link) => link.matchedSourceIds.includes(currentSourceId)),
  );
}

// 落点是软链接的 target：那个目录不属于它，内容由链接另一端负责，因此不参与
// 同步选择。判定只看「是不是链接」——能叫出归属（linkedTargetId）与叫不出
// （协议目录、外部目录、断链）一样不可选，后者此前会静默失效。
export function skillDirIsAlias(target: SyncTargetOption): boolean {
  return Boolean(target.skillDirLinkTarget ?? target.linkedTargetId);
}

// 能被勾选的 target：落点是软链接（整目录继承或指向别处）的都不参与同步选择。
export function isSelectableSyncTarget(target: SyncTargetOption): boolean {
  return !skillDirIsAlias(target);
}

// 打开同步弹窗时的 targets 预选：只挑已经装了当前来源内容的那批（列表里带
// 「关联」标签的 target），让用户接着把来源新增的 skill 补过去，不用每次重新勾。
export function defaultSelectedSyncTargetIds(
  targets: SyncTargetOption[],
  currentSourceId?: string,
): string[] {
  return targets
    .filter(isSelectableSyncTarget)
    .filter((target) => hasCurrentSourceSkillLinks(target.links, currentSourceId))
    .map((target) => target.id);
}

// 只有非受管来源的链接可以逐条移除：它们不属于任何已配置来源，没有来源级的
// 清理入口；待清理项仍归属当前来源，由移除同步或下次同步统一处理。
export function canRemoveSkillLink(link: SkillLinkAssociation): boolean {
  return link.state === "unmanaged";
}

// 展示链接的源路径:按当前来源取后端统一下发的相对路径(与安装面同一套
// Windows/大小写口径),查不到(非归属来源、未指定)退回绝对路径。此前的
// 前端归一化(Windows 前缀剥离、大小写折叠、尾斜杠清理)是对后端 classify
// 已算出值的重实现,已删。
export function skillLinkSourcePath(
  link: SkillLinkAssociation,
  currentSourceId?: string,
): string {
  if (!currentSourceId) return link.sourcePath;
  return (
    link.sourceRelatives.find((relative) => relative.sourceId === currentSourceId)
      ?.relativePath ?? link.sourcePath
  );
}

export function pendingSkillLinkCleanupCount(links: SkillLinkAssociation[]): number {
  return links.filter((link) => skillLinkStateInfo(link.state).pendingCleanup).length;
}
