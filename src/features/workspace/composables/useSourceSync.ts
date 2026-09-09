import { reactive } from "vue";
import {
  getSyncTargetOptions,
  previewSourceSyncConflicts,
  removeSourceSync,
  syncSourceToTargets,
} from "../api";
import type {
  SkillSourceConfigView,
  SourceSyncConflict,
  SourceSyncInput,
  SourceSyncResult,
  SyncSkillOption,
  SyncTargetOption,
} from "../types";
import { extractErrorMessage } from "@shared/lib/errors";
import { useWorkspaceNotice } from "./useWorkspaceNotice";
import { useWorkspaceState } from "./useWorkspaceState";
import { useWorkspaceAction } from "./useWorkspaceAction";

type SourceSyncSnapshot = {
  sourceRoot: string;
  skills: SyncSkillOption[];
};

type PendingSourceSync = {
  source: SkillSourceConfigView;
  skillPaths: string[];
  targetIds: string[];
  snapshot: SourceSyncSnapshot;
};

export function useSourceSync() {
  const { showNotice } = useWorkspaceNotice();
  const { reloadWorkspaceState } = useWorkspaceState();
  const { runWorkspaceAction } = useWorkspaceAction();

  // SourceSyncDialog is self-managing — we only track its open state and the
  // source it is operating on. The skill/target selection lives inside the
  // dialog itself.
  const sourceSyncDialog = reactive<{
    open: boolean;
    source: SkillSourceConfigView | null;
    loading: boolean;
  }>({
    open: false,
    source: null,
    loading: false,
  });

  const sourceSyncOverwriteDialog = reactive<{
    open: boolean;
    loading: boolean;
    conflicts: SourceSyncConflict[];
    pending: PendingSourceSync | null;
  }>({
    open: false,
    loading: false,
    conflicts: [],
    pending: null,
  });

  // RemoveSyncDialog state is parent-managed because the target options are
  // loaded asynchronously by the parent before the dialog opens.
  const removeSyncDialog = reactive<{
    open: boolean;
    loading: boolean;
    source: SkillSourceConfigView | null;
    targets: SyncTargetOption[];
    targetsLoading: boolean;
    selectedTargetIds: Set<string>;
  }>({
    open: false,
    loading: false,
    source: null,
    targets: [],
    targetsLoading: false,
    selectedTargetIds: new Set(),
  });

  function buildSourceSyncInput(
    pending: PendingSourceSync,
    overwriteExisting: boolean,
  ): SourceSyncInput {
    return {
      sourceId: pending.source.id,
      skillPaths: pending.skillPaths,
      targetIds: pending.targetIds,
      overwriteExisting,
      sourceRoot: pending.snapshot.sourceRoot,
      skills: pending.snapshot.skills,
    };
  }

  async function runSourceSync(pending: PendingSourceSync, overwriteExisting: boolean) {
    const result: SourceSyncResult = await syncSourceToTargets(
      buildSourceSyncInput(pending, overwriteExisting),
    );
    if (result.warnings.length > 0) {
      showNotice(result.warnings.join("\n"), "info");
    } else {
      showNotice(
        `同步完成：移除 ${result.removed.length} 个旧链接，新建 ${result.applied.length} 个软链接。`,
        "success",
      );
    }
    closeSourceSyncDialog();
    await reloadWorkspaceState({ preserveNotice: true });
  }

  function closeSourceSyncDialog() {
    sourceSyncDialog.open = false;
    sourceSyncDialog.source = null;
    sourceSyncDialog.loading = false;
  }

  function handleOpenSourceSync(source: SkillSourceConfigView) {
    sourceSyncDialog.open = true;
    sourceSyncDialog.source = source;
    sourceSyncDialog.loading = false;
  }

  async function handleConfirmSourceSync(
    skillPaths: string[],
    targetIds: string[],
    snapshot: SourceSyncSnapshot,
  ) {
    const source = sourceSyncDialog.source;
    if (!source) return;

    sourceSyncDialog.loading = true;
    const pending: PendingSourceSync = { source, skillPaths, targetIds, snapshot };

    try {
      const conflicts = await previewSourceSyncConflicts(buildSourceSyncInput(pending, false));
      if (conflicts.length > 0) {
        sourceSyncOverwriteDialog.open = true;
        sourceSyncOverwriteDialog.loading = false;
        sourceSyncOverwriteDialog.conflicts = conflicts;
        sourceSyncOverwriteDialog.pending = pending;
        sourceSyncDialog.loading = false;
        return;
      }
      await runSourceSync(pending, false);
    } catch (error) {
      showNotice(extractErrorMessage(error, "同步失败。"), "error");
      sourceSyncDialog.loading = false;
    }
  }

  function closeSourceSyncOverwriteDialog() {
    sourceSyncOverwriteDialog.open = false;
    sourceSyncOverwriteDialog.loading = false;
    sourceSyncOverwriteDialog.conflicts = [];
    sourceSyncOverwriteDialog.pending = null;
  }

  async function confirmSourceSyncOverwrite() {
    const pending = sourceSyncOverwriteDialog.pending;
    if (!pending) {
      closeSourceSyncOverwriteDialog();
      return;
    }

    sourceSyncOverwriteDialog.loading = true;
    sourceSyncDialog.loading = true;

    try {
      await runSourceSync(pending, true);
      closeSourceSyncOverwriteDialog();
    } catch (error) {
      showNotice(extractErrorMessage(error, "覆盖并同步失败。"), "error");
      sourceSyncOverwriteDialog.loading = false;
      sourceSyncDialog.loading = false;
    }
  }

  function handleOpenSourceRemoveSync(source: SkillSourceConfigView) {
    removeSyncDialog.open = true;
    removeSyncDialog.loading = false;
    removeSyncDialog.source = source;
    removeSyncDialog.targets = [];
    removeSyncDialog.targetsLoading = true;
    removeSyncDialog.selectedTargetIds = new Set();

    void getSyncTargetOptions()
      .then((options) => {
        if (removeSyncDialog.source?.id !== source.id) return;
        const enabledIds = options
          .filter((t) => t.enabled && !t.linkedTargetId)
          .map((t) => t.id);
        removeSyncDialog.targets = options;
        removeSyncDialog.targetsLoading = false;
        removeSyncDialog.selectedTargetIds = new Set(enabledIds);
      })
      .catch((error) => {
        if (removeSyncDialog.source?.id !== source.id) return;
        removeSyncDialog.targetsLoading = false;
        showNotice(extractErrorMessage(error, "读取同步目标失败。"), "error");
      });
  }

  function closeRemoveSyncDialog() {
    removeSyncDialog.open = false;
    removeSyncDialog.loading = false;
    removeSyncDialog.source = null;
    removeSyncDialog.targets = [];
    removeSyncDialog.targetsLoading = false;
    removeSyncDialog.selectedTargetIds = new Set();
  }

  function handleToggleRemoveSyncTarget(id: string) {
    const next = new Set(removeSyncDialog.selectedTargetIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    removeSyncDialog.selectedTargetIds = next;
  }

  function handleSetRemoveSyncTargets(targetIds: string[], selected: boolean) {
    const next = new Set(removeSyncDialog.selectedTargetIds);
    for (const id of targetIds) {
      if (selected) next.add(id);
      else next.delete(id);
    }
    removeSyncDialog.selectedTargetIds = next;
  }

  async function handleConfirmRemoveSourceSync() {
    const source = removeSyncDialog.source;
    if (!source || removeSyncDialog.selectedTargetIds.size === 0) return;

    removeSyncDialog.loading = true;
    await runWorkspaceAction({
      action: () => removeSourceSync(source.id, Array.from(removeSyncDialog.selectedTargetIds)),
      success: (result) =>
        result.removed.length > 0
          ? { message: `已移除 ${result.removed.length} 个软链接。` }
          : { message: "没有需要移除的软链接。", tone: "info" },
      error: `移除 ${source.label} 同步失败。`,
      after: () => closeRemoveSyncDialog(),
    });
    removeSyncDialog.loading = false;
  }

  return {
    sourceSyncDialog,
    sourceSyncOverwriteDialog,
    removeSyncDialog,
    handleOpenSourceSync,
    handleConfirmSourceSync,
    closeSourceSyncDialog,
    closeSourceSyncOverwriteDialog,
    confirmSourceSyncOverwrite,
    handleOpenSourceRemoveSync,
    closeRemoveSyncDialog,
    handleToggleRemoveSyncTarget,
    handleSetRemoveSyncTargets,
    handleConfirmRemoveSourceSync,
  };
}
