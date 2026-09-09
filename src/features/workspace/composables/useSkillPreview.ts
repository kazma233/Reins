import { watch } from "vue";
import { discoverGitSkills, discoverLocalSkills, filterDiscoveredSkills } from "../api";
import { parseCommaSeparatedList, type SkillDiscoveryPreviewState } from "../model";
import type { SkillDiscoveryResult, SkillSourceKind } from "../types";
import { extractErrorMessage } from "@shared/lib/errors";
import { createRequestGuard } from "@shared/lib/request-guard";
import { useWorkspaceNotice } from "./useWorkspaceNotice";

const SKILL_PREVIEW_FILTER_DEBOUNCE_MS = 300;

export type SkillSourcePreviewInput = {
  sourceType: SkillSourceKind;
  repo: string;
  rootPath: string;
  ref: string;
};

// Shared contract for the import dialog and the source-edit dialog: both host
// a discovery preview that is refreshed through the same discover/filter flow.
type SkillPreviewHost = {
  open: boolean;
  preview: SkillDiscoveryPreviewState;
};

function hasSameStringItems(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((item, index) => item === right[index]);
}

async function discoverSkillsFromSource(source: SkillSourcePreviewInput): Promise<SkillDiscoveryResult> {
  if (source.sourceType === "git") {
    if (!source.repo.trim()) {
      throw new Error("缺少 git 仓库地址。");
    }
    return discoverGitSkills(source.repo, source.ref.trim() || undefined);
  }
  if (!source.rootPath.trim()) {
    throw new Error("缺少本地来源目录。");
  }
  return discoverLocalSkills(source.rootPath);
}

// Discovery preview controller: owns the latest-wins guard, the discover ->
// filter pipeline and the debounced pattern re-filter, all bound to the host
// dialog's preview state. Host must be a reactive dialog object so the watch
// stays connected across state resets (Object.assign replaces `preview`).
export function useSkillPreview(host: SkillPreviewHost, refreshErrorText: string) {
  const { showNotice } = useWorkspaceNotice();
  const {
    next: nextPreviewRequestId,
    isLatest: isLatestPreviewRequest,
    invalidate: invalidatePreviewRequests,
  } = createRequestGuard();

  function setPreviewLoading(loading: boolean) {
    host.preview.previewLoading = loading;
  }

  function applyDiscoveryResult(
    result: SkillDiscoveryResult,
    expectedNameText: string,
    expectedPathText: string,
  ) {
    if (
      host.preview.includeNamePatternsText !== expectedNameText ||
      host.preview.includePathPatternsText !== expectedPathText
    ) {
      return;
    }
    host.preview.previewLoading = false;
    host.preview.discovery = result;
  }

  async function discoverAndFilterSkills(
    source: SkillSourcePreviewInput,
    nameText: string,
    pathText: string,
  ): Promise<void> {
    const requestId = nextPreviewRequestId();

    try {
      const discovery = await discoverSkillsFromSource(source);
      if (!isLatestPreviewRequest(requestId)) return;

      const namePatterns = parseCommaSeparatedList(nameText);
      const pathPatterns = parseCommaSeparatedList(pathText);
      if (namePatterns.length === 0 && pathPatterns.length === 0) {
        applyDiscoveryResult(discovery, nameText, pathText);
        return;
      }

      const filtered = await filterDiscoveredSkills(discovery.discoveryId, namePatterns, pathPatterns);
      if (
        isLatestPreviewRequest(requestId) &&
        hasSameStringItems(filtered.includeNamePatterns, namePatterns) &&
        hasSameStringItems(filtered.includePathPatterns, pathPatterns)
      ) {
        applyDiscoveryResult(filtered, nameText, pathText);
      }
    } catch (error) {
      if (isLatestPreviewRequest(requestId)) {
        setPreviewLoading(false);
        throw error;
      }
    }
  }

  async function filterSkillPreview(
    discoveryId: string,
    nameText: string,
    pathText: string,
  ): Promise<void> {
    const requestId = nextPreviewRequestId();
    const namePatterns = parseCommaSeparatedList(nameText);
    const pathPatterns = parseCommaSeparatedList(pathText);

    setPreviewLoading(true);

    try {
      const result = await filterDiscoveredSkills(discoveryId, namePatterns, pathPatterns);
      if (
        isLatestPreviewRequest(requestId) &&
        hasSameStringItems(result.includeNamePatterns, namePatterns) &&
        hasSameStringItems(result.includePathPatterns, pathPatterns)
      ) {
        applyDiscoveryResult(result, nameText, pathText);
      }
    } catch (error) {
      if (isLatestPreviewRequest(requestId)) {
        setPreviewLoading(false);
        throw error;
      }
    }
  }

  // Debounced filter: re-runs whenever the patterns change while a discovery
  // is loaded. Stale patterns are skipped by comparing them against the latest
  // filtered discovery state.
  watch(
    () => [
      host.open,
      host.preview.discovery?.discoveryId,
      host.preview.includeNamePatternsText,
      host.preview.includePathPatternsText,
    ],
    (_new, _old, onCleanup) => {
      const discoveryId = host.preview.discovery?.discoveryId;
      const nameText = host.preview.includeNamePatternsText;
      const pathText = host.preview.includePathPatternsText;

      if (!host.open || !discoveryId) return;

      const namePatterns = parseCommaSeparatedList(nameText);
      const pathPatterns = parseCommaSeparatedList(pathText);

      if (
        hasSameStringItems(host.preview.discovery?.includeNamePatterns ?? [], namePatterns) &&
        hasSameStringItems(host.preview.discovery?.includePathPatterns ?? [], pathPatterns)
      ) {
        setPreviewLoading(false);
        return;
      }

      const timeoutId = setTimeout(() => {
        void filterSkillPreview(discoveryId, nameText, pathText).catch((error) => {
          showNotice(extractErrorMessage(error, refreshErrorText), "error");
        });
      }, SKILL_PREVIEW_FILTER_DEBOUNCE_MS);

      onCleanup(() => clearTimeout(timeoutId));
    },
  );

  return {
    discoverAndFilterSkills,
    invalidatePreviewRequests,
    setPreviewLoading,
  };
}
