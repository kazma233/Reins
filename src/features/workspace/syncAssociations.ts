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
    return otherSourceIds.length > 0
      ? `当前来源 · ${otherSourceIds.map(getSourceLabel).join("、")}`
      : "当前来源";
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

// 该 target 的技能目录属于谁：能命名就写属主的地址，否则写链接指向的目录
// （叫不出名字时，路径是唯一能回答「内容在哪」的信息）。
export function skillDirOwnerLabel(target: SyncTargetOption): string | null {
  if (!skillDirIsAlias(target)) return null;
  return target.linkedTargetId
    ? `内容属 ${target.linkedTargetId}`
    : `内容在 ${target.skillDirLinkTarget}`;
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

function normalizePathForDisplay(path: string): { value: string; windows: boolean } {
  let value = path.trim().replace(/\\/g, "/");
  if (value.startsWith("//?/UNC/")) value = `//${value.slice("//?/UNC/".length)}`;
  else if (value.startsWith("//?/")) value = value.slice("//?/".length);

  const windows = /^[A-Za-z]:\//.test(value) || value.startsWith("//");
  if (value.length > 1) value = value.replace(/\/+$/, "");
  return { value, windows };
}

export function skillLinkSourcePath(sourcePath: string, sourceRoot?: string): string {
  if (!sourceRoot?.trim()) return sourcePath;

  const source = normalizePathForDisplay(sourcePath);
  const root = normalizePathForDisplay(sourceRoot);
  const compareSource = source.windows || root.windows ? source.value.toLowerCase() : source.value;
  const compareRoot = source.windows || root.windows ? root.value.toLowerCase() : root.value;

  if (compareSource === compareRoot) return ".";
  if (compareRoot === "/") {
    return compareSource.startsWith("/") ? source.value.slice(1) || "." : sourcePath;
  }
  if (!compareSource.startsWith(`${compareRoot}/`)) return sourcePath;
  return source.value.slice(root.value.length + 1) || ".";
}

export function pendingSkillLinkCleanupCount(links: SkillLinkAssociation[]): number {
  return links.filter((link) => skillLinkStateInfo(link.state).pendingCleanup).length;
}
