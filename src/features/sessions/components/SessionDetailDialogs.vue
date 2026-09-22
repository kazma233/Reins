<script setup lang="ts">
import { computed } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import { formatSourceAppName } from "../source-app";
import { deleteCommandPreview, deleteMethodCopy } from "../model";
import type { SessionOverview } from "../types";

type SessionDetailDialogsProps = {
  overview: SessionOverview;
  deleteDialogOpen: boolean;
  deleteLoading: boolean;
};

const props = defineProps<SessionDetailDialogsProps>();

const emit = defineEmits<{
  closeDeleteDialog: [];
  confirmDelete: [];
}>();

const selectedDeleteCopy = computed(() =>
  deleteMethodCopy(props.overview.summary.sourceApp)
);
const deleteCommands = computed(() => deleteCommandPreview(props.overview));
</script>

<template>
  <!-- Delete dialog -->
  <ConfirmDialog
    :confirm-label="deleteLoading ? '删除中...' : '确认删除'"
    dialog-class-name="delete-dialog"
    eyebrow="删除会话"
    :loading="deleteLoading"
    :open="deleteDialogOpen"
    title="确认删除这个会话组？"
    title-id="delete-session-dialog-title"
    @close="emit('closeDeleteDialog')"
    @confirm="emit('confirmDelete')"
  >
    <div class="delete-dialog-summary">
      <strong>{{ overview.summary.title }}</strong>
      <code>{{ overview.summary.sourceSessionId }}</code>
    </div>
    <div class="delete-dialog-copy">
      <p>会删除当前会话以及它的子 Agent 会话。</p>
      <p>删除后不会自动恢复。</p>
    </div>
    <div class="delete-dialog-method-box">
      <p class="delete-dialog-method-title">删除方式</p>
      <p>{{ selectedDeleteCopy.description }}</p>
      <ul class="delete-dialog-method-list">
        <li
          v-for="(item, index) in selectedDeleteCopy.details"
          :key="`${item}-${index}`"
        >
          {{ item }}
        </li>
      </ul>
    </div>
    <div class="delete-dialog-meta">
      <span class="pill danger-pill">
        {{ formatSourceAppName(overview.summary.sourceApp) }}
      </span>
      <span class="muted-text">共 {{ overview.sourcePaths.length }} 个存储路径</span>
    </div>
    <div class="path-list compact">
      <code
        v-for="(path, index) in overview.sourcePaths"
        :key="`${path}-${index}`"
      >{{ path }}</code>
    </div>
    <div class="delete-dialog-command-block">
      <p class="delete-dialog-command-title">
        {{ selectedDeleteCopy.commandLabel }}
      </p>
      <div class="path-list">
        <code
          v-for="(command, index) in deleteCommands"
          :key="`${command}-${index}`"
        >{{ command }}</code>
      </div>
    </div>
  </ConfirmDialog>
</template>
