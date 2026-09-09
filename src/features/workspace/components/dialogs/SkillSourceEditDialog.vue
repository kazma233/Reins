<script setup lang="ts">
import { computed } from "vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import SkillDiscoveryPreview from "../SkillDiscoveryPreview.vue";
import type { SkillDiscoveryResult } from "../../types";
import type { SkillSourceEditDialogState } from "../../model";

type SkillSourceEditDialogProps = {
  state: SkillSourceEditDialogState;
  loading: boolean;
};

const props = defineProps<SkillSourceEditDialogProps>();

defineEmits<{
  close: [];
  confirm: [];
  refreshPreview: [];
}>();

function renderDiscoverySummary(discovery: SkillDiscoveryResult | null, loading = false) {
  if (!discovery || loading) return null;
  const includeCount = discovery.includeNamePatterns.length + discovery.includePathPatterns.length;
  const excludedCount = discovery.excludedSkills?.length ?? 0;
  return `将导入 ${discovery.skills.length} 个 skills${includeCount ? ` · 匹配 ${includeCount} 项` : ""}${excludedCount ? ` · 排除 ${excludedCount} 项` : ""}`;
}

const summaryText = computed(() =>
  renderDiscoverySummary(props.state.preview.discovery, props.state.preview.previewLoading),
);

function handleRefInput(event: Event) {
  props.state.ref = (event.target as HTMLInputElement).value;
}

function handleNamePatternsInput(value: string) {
  const preview = props.state.preview;
  preview.previewLoading = Boolean(preview.discovery);
  preview.includeNamePatternsText = value;
}

function handlePathPatternsInput(value: string) {
  const preview = props.state.preview;
  preview.previewLoading = Boolean(preview.discovery);
  preview.includePathPatternsText = value;
}
</script>

<template>
  <DialogShell
    :open="state.open"
    dialog-class-name="manager-import-dialog"
    eyebrow="Source"
    :title="state.title || '编辑导入来源'"
    title-id="skill-source-edit-dialog-title"
    :close-disabled="loading"
    @close="$emit('close')"
  >
    <template #actions>
      <button
        class="primary-button"
        :disabled="loading"
        type="button"
        @click="$emit('confirm')"
      >
        {{ loading ? "保存中..." : "保存并同步" }}
      </button>
    </template>

    <div class="manager-import-shell manager-import-shell--compact">
      <template v-if="state.sourceType === 'git'">
        <div class="manager-import-row manager-import-row--git-source">
          <label class="manager-field manager-import-row__field manager-import-row__field--repo">
            <span>Git 仓库</span>
            <input readonly :value="state.repo" type="text" />
          </label>
          <label class="manager-field manager-import-row__field manager-import-row__field--ref">
            <span>分支</span>
            <input :value="state.ref" type="text" @input="handleRefInput" />
          </label>
        </div>
      </template>
      <template v-else>
        <label class="manager-field">
          <span>来源目录</span>
          <input readonly :value="state.rootPath" type="text" />
        </label>
      </template>

      <div class="manager-import-row manager-import-row--include-patterns">
        <label class="manager-field manager-import-row__field">
          <span>匹配名称</span>
          <input
            placeholder="按技能名称匹配，如 agent-*,draft-*"
            :value="state.preview.includeNamePatternsText"
            type="text"
            @input="(event: Event) => handleNamePatternsInput((event.target as HTMLInputElement).value)"
          />
        </label>
        <label class="manager-field manager-import-row__field">
          <span>匹配路径</span>
          <input
            placeholder="按技能路径匹配，如 python/*,docs/*"
            :value="state.preview.includePathPatternsText"
            type="text"
            @input="(event: Event) => handlePathPatternsInput((event.target as HTMLInputElement).value)"
          />
        </label>
      </div>

      <div class="manager-actions">
        <button
          class="secondary-button"
          :disabled="loading || state.preview.previewLoading"
          type="button"
          @click="$emit('refreshPreview')"
        >
          {{ state.preview.previewLoading ? "刷新中..." : "刷新预览" }}
        </button>
        <span v-if="summaryText" class="manager-inline-hint">{{ summaryText }}</span>
      </div>
    </div>

    <SkillDiscoveryPreview
      :discovery="state.preview.discovery"
      :empty-text="state.preview.previewLoading ? '正在读取来源...' : '先刷新预览。'"
      :loading="state.preview.previewLoading"
    />
  </DialogShell>
</template>
