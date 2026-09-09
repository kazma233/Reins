<script setup lang="ts">
import { computed } from "vue";
import { formatTimestamp } from "@shared/lib/format";
import type { SkillSourceConfigView } from "../types";

type SourceRowProps = {
  source: SkillSourceConfigView;
  loading: boolean;
};

const props = defineProps<SourceRowProps>();

defineEmits<{
  edit: [source: SkillSourceConfigView];
  delete: [source: SkillSourceConfigView];
  sync: [source: SkillSourceConfigView];
  removeSync: [source: SkillSourceConfigView];
  refresh: [source: SkillSourceConfigView];
}>();

const isGit = computed(() => props.source.type === "git");
const label = computed(() =>
  props.source.type === "git" ? props.source.repo : props.source.rootPath,
);
const namePatterns = computed(() => props.source.includeNamePatterns);
const pathPatterns = computed(() => props.source.includePathPatterns);
const hasPatterns = computed(
  () => namePatterns.value.length > 0 || pathPatterns.value.length > 0,
);
const lastFetchedAtLabel = computed(() => {
  if (props.source.type !== "git") return null;
  return props.source.lastFetchedAt ? formatTimestamp(props.source.lastFetchedAt) : "尚未拉取";
});
</script>

<template>
  <div class="manager-skill-source-row">
    <span :class="isGit ? 'manager-skill-source-row__icon--git' : 'manager-skill-source-row__icon--local'">
      {{ isGit ? "⎇" : "/" }}
    </span>
    <div class="manager-skill-source-row__main">
      <span class="manager-skill-source-row__label">{{ label }}</span>
      <span v-if="isGit" class="manager-skill-source-row__meta">
        分支：{{ source.type === "git" ? source.ref ?? "main" : "" }} · 上次拉取：{{ lastFetchedAtLabel }}
      </span>
      <div class="manager-skill-source-row__patterns">
        <template v-if="hasPatterns">
          <div v-if="namePatterns.length > 0" class="manager-skill-source-row__pattern-group">
            <span class="manager-skill-source-row__pattern-label">name</span>
            <code
              v-for="pattern in namePatterns"
              :key="pattern"
              class="manager-skill-source-row__pattern-chip"
            >{{ pattern }}</code>
          </div>
          <div v-if="pathPatterns.length > 0" class="manager-skill-source-row__pattern-group">
            <span class="manager-skill-source-row__pattern-label">path</span>
            <code
              v-for="pattern in pathPatterns"
              :key="pattern"
              class="manager-skill-source-row__pattern-chip"
            >{{ pattern }}</code>
          </div>
        </template>
        <span v-else class="manager-skill-source-row__pattern-empty">匹配全部</span>
      </div>
    </div>
    <div class="manager-skill-source-row__actions">
      <div class="manager-skill-source-row__actions-row">
        <button
          class="secondary-button manager-skill-source-row__action"
          :disabled="loading"
          type="button"
          @click="$emit('edit', source)"
        >
          编辑
        </button>
        <button
          class="danger-button manager-skill-source-row__action"
          :disabled="loading"
          type="button"
          @click="$emit('delete', source)"
        >
          删除
        </button>
      </div>
      <div class="manager-skill-source-row__actions-row">
        <button
          class="primary-button manager-skill-source-row__action"
          :disabled="loading"
          type="button"
          @click="$emit('sync', source)"
        >
          同步
        </button>
        <button
          class="warning-button manager-skill-source-row__action"
          :disabled="loading"
          type="button"
          @click="$emit('removeSync', source)"
        >
          移除同步
        </button>
        <button
          v-if="isGit"
          class="secondary-button manager-skill-source-row__action"
          :disabled="loading"
          title="忽略 24 小时自动更新间隔"
          type="button"
          @click="$emit('refresh', source)"
        >
          强制拉取
        </button>
      </div>
    </div>
  </div>
</template>
