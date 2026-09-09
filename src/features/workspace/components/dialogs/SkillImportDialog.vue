<script setup lang="ts">
import { computed } from "vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import BatchGitImportResultList from "../BatchGitImportResultList.vue";
import SkillDiscoveryPreview from "../SkillDiscoveryPreview.vue";
import type { SkillDiscoveryResult } from "../../types";
import type { SkillImportDialogState } from "../../model";

type SkillImportDialogProps = {
  dialogState: SkillImportDialogState;
  loading: boolean;
};

const props = defineProps<SkillImportDialogProps>();

defineEmits<{
  close: [];
  confirm: [];
  discoverGitSkills: [];
  pickLocalSkillDirectory: [];
}>();

const BATCH_GIT_SKILL_IMPORT_PLACEHOLDER = [
  "- repo: https://github.com/anthropics/skills.git",
  "  ref: main",
  "  include_name_patterns:",
  "    - agent-*",
  "  include_path_patterns:",
  "    - python/*",
].join("\n");

function skillImportConfirmLabel(mode: SkillImportDialogState["mode"]): string {
  if (mode === "batch-git") return "开始导入";
  return "确认导入";
}

function skillImportDialogEyebrow(
  mode: SkillImportDialogState["mode"],
  sourceType: SkillImportDialogState["sourceType"],
): string {
  if (mode === "batch-git") return "Batch Git";
  if (sourceType === "git") return "Git";
  return "Local";
}

function skillImportDialogTitle(mode: SkillImportDialogState["mode"]): string {
  if (mode === "batch-git") return "批量导入 git skills";
  return "选择要导入的 skills";
}

function canConfirmSkillImport(state: SkillImportDialogState): boolean {
  if (state.mode === "batch-git") return state.batchYamlText.trim().length > 0;
  return Boolean(state.preview.discovery?.skills.length);
}

const eyebrow = computed(() =>
  skillImportDialogEyebrow(props.dialogState.mode, props.dialogState.sourceType),
);
const title = computed(() => skillImportDialogTitle(props.dialogState.mode));
const confirmLabel = computed(() => skillImportConfirmLabel(props.dialogState.mode));
const canConfirm = computed(() => canConfirmSkillImport(props.dialogState));

function handleBatchYamlInput(event: Event) {
  const value = (event.target as HTMLTextAreaElement).value;
  props.dialogState.batchYamlText = value;
  props.dialogState.batchResult = null;
}

function handleRepoInput(event: Event) {
  props.dialogState.repo = (event.target as HTMLInputElement).value;
}

function handleRefInput(event: Event) {
  props.dialogState.ref = (event.target as HTMLInputElement).value;
}

function handleNamePatternsInput(value: string) {
  const preview = props.dialogState.preview;
  preview.previewLoading = Boolean(preview.discovery);
  preview.includeNamePatternsText = value;
}

function handlePathPatternsInput(value: string) {
  const preview = props.dialogState.preview;
  preview.previewLoading = Boolean(preview.discovery);
  preview.includePathPatternsText = value;
}

function renderDiscoverySummary(discovery: SkillDiscoveryResult | null, loading = false) {
  if (!discovery || loading) return null;
  const includeCount = discovery.includeNamePatterns.length + discovery.includePathPatterns.length;
  const excludedCount = discovery.excludedSkills?.length ?? 0;
  return `将导入 ${discovery.skills.length} 个 skills${includeCount ? ` · 匹配 ${includeCount} 项` : ""}${excludedCount ? ` · 排除 ${excludedCount} 项` : ""}`;
}

const summaryText = computed(() =>
  renderDiscoverySummary(props.dialogState.preview.discovery, props.dialogState.preview.previewLoading),
);
</script>

<template>
  <DialogShell
    :open="dialogState.open"
    dialog-class-name="manager-import-dialog"
    :eyebrow="eyebrow"
    :title="title"
    title-id="skill-import-dialog-title"
    :close-disabled="loading"
    @close="$emit('close')"
  >
    <template #actions>
      <button
        class="primary-button"
        :disabled="loading || !canConfirm"
        type="button"
        @click="$emit('confirm')"
      >
        {{ confirmLabel }}
      </button>
    </template>

    <div class="manager-import-shell manager-import-shell--compact">
      <div class="manager-stack">
        <span class="manager-field__label">导入模式</span>
        <div class="manager-segmented">
          <button
            :class="`manager-segmented__button${dialogState.mode === 'single' ? ' is-active' : ''}`"
            type="button"
            @click="dialogState.mode = 'single'"
          >
            单个导入
          </button>
          <button
            :class="`manager-segmented__button${dialogState.mode === 'batch-git' ? ' is-active' : ''}`"
            type="button"
            @click="dialogState.mode = 'batch-git'"
          >
            批量 Git
          </button>
        </div>
      </div>

      <template v-if="dialogState.mode === 'batch-git'">
        <label class="manager-field">
          <span>YAML 内容</span>
          <textarea
            class="manager-editor manager-textarea manager-batch-import-editor"
            :placeholder="BATCH_GIT_SKILL_IMPORT_PLACEHOLDER"
            :value="dialogState.batchYamlText"
            @input="handleBatchYamlInput"
          />
        </label>
      </template>

      <template v-else>
        <div class="manager-stack">
          <span class="manager-field__label">导入来源</span>
          <div class="manager-segmented">
            <button
              :class="`manager-segmented__button${dialogState.sourceType === 'git' ? ' is-active' : ''}`"
              type="button"
              @click="dialogState.sourceType = 'git'"
            >
              Git
            </button>
            <button
              :class="`manager-segmented__button${dialogState.sourceType === 'local' ? ' is-active' : ''}`"
              type="button"
              @click="dialogState.sourceType = 'local'"
            >
              本地目录
            </button>
          </div>
        </div>

        <template v-if="dialogState.sourceType === 'git'">
          <div class="manager-import-row manager-import-row--git-source">
            <label class="manager-field manager-import-row__field manager-import-row__field--repo">
              <span>Git 仓库</span>
              <input
                placeholder="https://github.com/anthropics/skills.git"
                :value="dialogState.repo"
                type="text"
                @input="handleRepoInput"
              />
            </label>
            <label class="manager-field manager-import-row__field manager-import-row__field--ref">
              <span>分支</span>
              <input
                :value="dialogState.ref"
                type="text"
                @input="handleRefInput"
              />
            </label>
          </div>
        </template>

        <template v-else>
          <div class="manager-import-row">
            <label class="manager-field manager-import-row__field">
              <span>导入目录</span>
              <input readonly :value="dialogState.rootPath" type="text" />
            </label>
            <div class="manager-actions manager-import-row__actions">
              <button
                class="secondary-button"
                :disabled="loading"
                type="button"
                @click="$emit('pickLocalSkillDirectory')"
              >
                选择目录
              </button>
            </div>
          </div>
        </template>

        <div class="manager-import-row manager-import-row--include-patterns">
          <label class="manager-field manager-import-row__field">
            <span>匹配名称</span>
            <input
              placeholder="按技能名称匹配，如 agent-*,draft-*"
              :value="dialogState.preview.includeNamePatternsText"
              type="text"
              @input="(event: Event) => handleNamePatternsInput((event.target as HTMLInputElement).value)"
            />
          </label>
          <label class="manager-field manager-import-row__field">
            <span>匹配路径</span>
            <input
              placeholder="按技能路径匹配，如 python/*,docs/*"
              :value="dialogState.preview.includePathPatternsText"
              type="text"
              @input="(event: Event) => handlePathPatternsInput((event.target as HTMLInputElement).value)"
            />
          </label>
        </div>

        <div v-if="dialogState.sourceType === 'git'" class="manager-actions">
          <button
            class="secondary-button"
            :disabled="loading || dialogState.preview.previewLoading"
            type="button"
            @click="$emit('discoverGitSkills')"
          >
            {{ dialogState.preview.previewLoading ? "扫描中..." : "扫描来源" }}
          </button>
          <span v-if="summaryText" class="manager-inline-hint">{{ summaryText }}</span>
        </div>
        <div v-else-if="dialogState.preview.previewLoading" class="manager-actions">
          <span class="manager-inline-hint">正在读取来源...</span>
        </div>
        <div v-else-if="dialogState.preview.discovery" class="manager-actions">
          <span class="manager-inline-hint">{{ summaryText }}</span>
        </div>
      </template>
    </div>

    <template v-if="dialogState.mode === 'batch-git'">
      <BatchGitImportResultList v-if="dialogState.batchResult" :result="dialogState.batchResult" />
    </template>

    <template v-else>
      <SkillDiscoveryPreview
        :discovery="dialogState.preview.discovery"
        :empty-text="dialogState.preview.previewLoading ? '正在读取来源...' : '先配置导入来源。'"
        :loading="dialogState.preview.previewLoading"
      />
    </template>
  </DialogShell>
</template>
