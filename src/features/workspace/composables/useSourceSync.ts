import { reactive, ref } from "vue";
import { previewSourceSyncConflicts, syncSourceToTargets } from "../api";
import type {
  SkillSourceConfigView,
  SourceSyncConflict,
  SourceSyncInput,
  SourceSyncResult,
  SyncSkillOption,
} from "../types";
import { extractErrorMessage } from "@shared/lib/errors";
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
  const { reloadWorkspaceState } = useWorkspaceState();

  // 同步结果回到弹窗底部同步按钮旁：弹窗保持打开，用户能直接看到结果并接着操作。
  const sourceSyncDialogRef = ref<{ reportResult: (result: { text: string; failed: boolean }) => void } | null>(
    null,
  );

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
    error: string | null;
    conflicts: SourceSyncConflict[];
    pending: PendingSourceSync | null;
  }>({
    open: false,
    loading: false,
    error: null,
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

  // 同步结果只回写到弹窗按钮旁；面板列表由 reload 自行更新，
  // 不再额外往来源行上挂一份结果。
  async function runSourceSync(pending: PendingSourceSync, overwriteExisting: boolean) {
    const result: SourceSyncResult = await syncSourceToTargets(
      buildSourceSyncInput(pending, overwriteExisting),
    );
    sourceSyncDialogRef.value?.reportResult({
      text:
        result.warnings.length > 0
          ? result.warnings.join("\n")
          : `同步完成：移除 ${result.removed.length} 个旧链接，新建 ${result.applied.length} 个软链接。`,
      failed: result.warnings.length > 0,
    });
    // 覆盖确认弹窗只在覆盖路径上叠在同步弹窗之上，同步结束就该收起来；
    // 同步弹窗保持打开，结果与后续操作都在原地。
    closeSourceSyncOverwriteDialog();
    await reloadWorkspaceState({ background: true });
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
        return;
      }
      await runSourceSync(pending, false);
    } catch (error) {
      sourceSyncDialogRef.value?.reportResult({
        text: extractErrorMessage(error, "同步失败。"),
        failed: true,
      });
    } finally {
      // 弹窗不关，同步结束后必须解开按钮，不然用户接着操作会被锁住。
      sourceSyncDialog.loading = false;
    }
  }

  function closeSourceSyncOverwriteDialog() {
    sourceSyncOverwriteDialog.open = false;
    sourceSyncOverwriteDialog.loading = false;
    sourceSyncOverwriteDialog.error = null;
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
    } catch (error) {
      sourceSyncOverwriteDialog.error = extractErrorMessage(error, "覆盖并同步失败。");
    } finally {
      sourceSyncOverwriteDialog.loading = false;
      sourceSyncDialog.loading = false;
    }
  }

  return {
    sourceSyncDialog,
    sourceSyncDialogRef,
    sourceSyncOverwriteDialog,
    handleOpenSourceSync,
    handleConfirmSourceSync,
    closeSourceSyncDialog,
    closeSourceSyncOverwriteDialog,
    confirmSourceSyncOverwrite,
  };
}
