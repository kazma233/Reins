<script setup lang="ts">
import { computed } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import { formatSourceAppName } from "../source-app";
import type { DeletePlan, DeletePlanAction, SessionOverview } from "../types";

type SessionDetailDialogsProps = {
  overview: SessionOverview;
  deletePlan: DeletePlan;
  deleteDialogOpen: boolean;
  deleteLoading: boolean;
};

const props = defineProps<SessionDetailDialogsProps>();

const emit = defineEmits<{
  closeDeleteDialog: [];
  confirmDelete: [];
}>();

// 删除预览只渲染 plan 里的动作；命令行拼写是纯展示层行为。
// POSIX shell 引号：用 '\'' 转义单引号。
function shellQuote(value: string): string {
  return `'${value.replace(/'/g, "'\\''")}'`;
}

function formatDeletePlanAction(action: DeletePlanAction): string {
  switch (action.kind) {
    case "remove_file":
      return `rm ${shellQuote(action.path)}`;
    case "remove_directory":
      // ~/ 前缀目录(claude sidecar)渲染为 $HOME 展开;绝对路径(grokbuild
      // 子会话目录)用单引号。
      return action.path.startsWith("~/")
        ? `rm -rf "$HOME/${action.path.slice(2)}"`
        : `rm -rf ${shellQuote(action.path)}`;
    case "run_cli":
      return [action.program, ...action.args].join(" ");
    case "sqlite":
      return `sqlite3 ${homeQuoted(action.db_path)} "${action.sql}"`;
  }
}

function homeQuoted(path: string): string {
  return path.startsWith("~/") ? `"$HOME/${path.slice(2)}"` : `"${path}"`;
}

const deleteCommands = computed(() =>
  props.deletePlan.actions.map(formatDeletePlanAction)
);
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
      <p>{{ deletePlan.description }}</p>
      <ul class="delete-dialog-method-list">
        <li
          v-for="(item, index) in deletePlan.details"
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
        {{ deletePlan.commandLabel }}
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
