<script setup lang="ts">
import { computed } from "vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import SyncTargetGroups from "../SyncTargetGroups.vue";
import type { SyncTargetOption } from "../../types";

type RemoveSyncDialogProps = {
  open: boolean;
  loading: boolean;
  sourceId: string;
  sourceLabels: Record<string, string>;
  targetsLoading: boolean;
  sourceLabel: string;
  targets: SyncTargetOption[];
  selectedTargetIds: Set<string>;
};

const props = defineProps<RemoveSyncDialogProps>();

defineEmits<{
  close: [];
  confirm: [];
  setTargets: [targetIds: string[], selected: boolean];
  toggleTarget: [id: string];
}>();

function getSelectableTargetIds(targets: SyncTargetOption[]): string[] {
  return targets.filter((t) => t.enabled && !t.linkedTargetId).map((t) => t.id);
}

const selectableCount = computed(() => getSelectableTargetIds(props.targets).length);
const canConfirm = computed(() => props.selectedTargetIds.size > 0 && !props.loading);
</script>

<template>
  <DialogShell
    v-if="targets.length === 0"
    :open="open"
    dialog-class-name="manager-import-dialog"
    eyebrow="Remove Sync"
    :title="`移除同步: ${sourceLabel}`"
    title-id="remove-sync-dialog-title"
    :close-disabled="loading"
    @close="$emit('close')"
  >
    <div class="empty-state">
      {{ targetsLoading ? "正在读取目标..." : "暂无可移除的目标。" }}
    </div>
  </DialogShell>

  <DialogShell
    v-else
    :open="open"
    dialog-class-name="manager-import-dialog"
    eyebrow="Remove Sync"
    :title="`移除同步: ${sourceLabel}`"
    title-id="remove-sync-dialog-title"
    :close-disabled="loading"
    @close="$emit('close')"
  >
    <template #actions>
      <button class="secondary-button" :disabled="loading" type="button" @click="$emit('close')">
        取消
      </button>
      <button class="danger-button" :disabled="!canConfirm" type="button" @click="$emit('confirm')">
        {{ loading ? "移除中..." : `移除 ${selectedTargetIds.size} 个目标` }}
      </button>
    </template>

    <p class="manager-sync-source-location">
      选择要清理该来源已分发软链的目标。未勾选的目标保持原样。
    </p>

    <SyncTargetGroups
      :busy="loading"
      :current-source-id="sourceId"
      :source-labels="sourceLabels"
      :selected-target-ids="selectedTargetIds"
      :targets="targets"
      @set-targets="(ids: string[], sel: boolean) => $emit('setTargets', ids, sel)"
      @toggle-target="(id: string) => $emit('toggleTarget', id)"
    />

    <p v-if="selectableCount === 0" class="empty-state">没有可操作的目标。</p>
  </DialogShell>
</template>
