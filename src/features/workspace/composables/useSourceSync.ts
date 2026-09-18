import { reactive } from "vue";
import { previewSourceSyncConflicts, syncSourceToTargets } from "../api";
import type {
  SkillSourceConfigView,
  SourceSyncConflict,
  SourceSyncInput,
  SourceSyncResult,
  SyncSkillOption,
} from "../types";
import { extractErrorMessage } from "@shared/lib/errors";
import { useWorkspaceNotice } from "./useWorkspaceNotice";
import { useWorkspaceState } from "./useWorkspaceState";

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

  // SourceSyncDialog is self-managing — we only track its open state and the
  // source it is operating on. Skill/target selection and the remove-sync
  // action for the picked targets all live inside the dialog itself.
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

  return {
    sourceSyncDialog,
    sourceSyncOverwriteDialog,
    handleOpenSourceSync,
    handleConfirmSourceSync,
    closeSourceSyncDialog,
    closeSourceSyncOverwriteDialog,
    confirmSourceSyncOverwrite,
  };
}
