<script setup lang="ts">
import { computed } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import { formatSourceAppName } from "../source-app";
import {
  deleteCommandPreview,
  deleteMethodCopy,
  formatImportLevel,
  importTargetCopy,
  manualOpenCommand,
  translateWarning,
  IMPORT_TARGET_APPS
} from "../model";
import type { ImportPreview, ImportResult, SessionOverview, SourceApp } from "../types";

type SessionDetailDialogsProps = {
  overview: SessionOverview;
  deleteDialogOpen: boolean;
  deleteLoading: boolean;
  importDialogOpen: boolean;
  importLoading: boolean;
  importResult: ImportResult | null;
  preview: ImportPreview | null;
  previewError: string | null;
  previewLoading: boolean;
  previewReady: boolean;
  targetApp: SourceApp;
};

const props = defineProps<SessionDetailDialogsProps>();

const emit = defineEmits<{
  closeDeleteDialog: [];
  closeImportDialog: [];
  closeImportResultDialog: [];
  confirmDelete: [];
  confirmImport: [];
  targetAppChange: [app: SourceApp];
}>();

const selectedDeleteCopy = computed(() =>
  deleteMethodCopy(props.overview.summary.sourceApp)
);
const selectedTargetCopy = computed(() => importTargetCopy(props.targetApp));
const importResultCopy = computed(() =>
  props.importResult ? importTargetCopy(props.importResult.targetApp) : null
);
const targetAppOptions = computed(() =>
  IMPORT_TARGET_APPS.filter((app) => app !== props.overview.summary.sourceApp)
);
const deleteCommands = computed(() => deleteCommandPreview(props.overview));
const hasStalePreview = computed(
  () => Boolean(props.preview) && !props.previewReady
);
const previewRegionClassName = computed(() =>
  [
    "import-preview-region",
    props.previewLoading ? "is-refreshing" : "",
    hasStalePreview.value ? "has-stale-preview" : ""
  ]
    .filter(Boolean)
    .join(" ")
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

  <!-- Import preview dialog -->
  <DialogShell
    actions-class-name="import-dialog-actions"
    center-title
    :close-disabled="importLoading"
    dialog-class-name="import-dialog"
    :open="importDialogOpen"
    title="确认导入？"
    title-id="import-session-dialog-title"
    @close="emit('closeImportDialog')"
  >
    <template #actions>
      <button
        class="primary-button"
        :disabled="
          previewLoading ||
          importLoading ||
          !previewReady ||
          !preview?.supported
        "
        type="button"
        @click="emit('confirmImport')"
      >
        {{ importLoading ? "导入中..." : "导入" }}
      </button>
    </template>

    <div class="dialog-summary">
      <strong>{{ overview.summary.title }}</strong>
      <code>{{ overview.summary.sourceSessionId }}</code>
    </div>
    <div class="import-row import-dialog-target-row">
      <div class="import-dialog-target-field">
        <span class="muted-text">目标程序</span>
        <div class="import-dialog-target-buttons" role="group" aria-label="目标程序">
          <button
            v-for="app in targetAppOptions"
            :key="app"
            :class="`import-target-button${targetApp === app ? ' active' : ''}`"
            type="button"
            @click="emit('targetAppChange', app)"
          >
            {{ importTargetCopy(app).optionLabel }}
          </button>
        </div>
      </div>
    </div>
    <div class="import-method-box">
      <span class="pill">{{ selectedTargetCopy.methodLabel }}</span>
      <p class="muted-text">{{ selectedTargetCopy.methodDescription }}</p>
    </div>
    <div :class="previewRegionClassName">
      <div
        v-if="previewError"
        class="preview-box import-preview-status-box error-box"
      >
        <strong>预览失败</strong>
        <pre class="error-text">{{ previewError }}</pre>
      </div>
      <div v-else-if="preview" class="preview-box">
        <strong>
          {{
            preview.supported
              ? `导入级别：${formatImportLevel(preview.importLevel)}`
              : "暂不支持导入"
          }}
        </strong>
        <div v-if="preview.createdPaths.length > 0" class="path-list">
          <code
            v-for="(path, index) in preview.createdPaths"
            :key="`${path}-${index}`"
          >{{ path }}</code>
        </div>
        <ul v-if="preview.warnings.length > 0">
          <li
            v-for="(warning, index) in preview.warnings"
            :key="`${warning}-${index}`"
          >
            {{ translateWarning(warning) }}
          </li>
        </ul>
      </div>
      <div v-else class="import-preview-loading">
        {{
          previewLoading
            ? "正在加载导入预览，检查兼容性和目标路径..."
            : "正在准备导入预览..."
        }}
      </div>
      <div v-if="previewLoading" class="import-preview-overlay" role="status">
        正在更新 {{ importTargetCopy(targetApp).optionLabel }} 预览...
      </div>
    </div>
  </DialogShell>

  <!-- Import result dialog -->
  <DialogShell
    dialog-class-name="import-dialog"
    eyebrow="导入完成"
    :open="Boolean(importResult)"
    title="新会话已经写入目标程序"
    title-id="import-result-dialog-title"
    @close="emit('closeImportResultDialog')"
  >
    <div v-if="importResult && importResultCopy" class="preview-box success-box">
      <strong>{{ importResultCopy.successTitle }}</strong>
      <p>会话 ID：{{ importResult.createdSessionId }}</p>
      <p v-if="importResultCopy.successNote" class="muted-text">
        {{ importResultCopy.successNote }}
      </p>
      <p class="muted-text">
        推荐工作目录：{{ importResult.resumeCwd ?? "未知" }}
      </p>
      <p class="muted-text">{{ importResultCopy.manualLabel }}：</p>
      <div class="path-list">
        <code>
          {{
            manualOpenCommand(
              importResult.targetApp,
              importResult.createdSessionId,
              importResult.resumeCwd,
              importResult.createdPaths[0]
            )
          }}
        </code>
      </div>
      <div class="path-list">
        <code
          v-for="(path, index) in importResult.createdPaths"
          :key="`${path}-${index}`"
        >{{ path }}</code>
      </div>
    </div>
  </DialogShell>
</template>
