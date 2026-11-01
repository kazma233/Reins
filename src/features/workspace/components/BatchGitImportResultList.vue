<script setup lang="ts">
import type { BatchGitSkillImportResult } from "../types";

type BatchGitImportResultListProps = {
  result: BatchGitSkillImportResult;
};

defineProps<BatchGitImportResultListProps>();
</script>

<template>
  <div class="manager-import-skill-list">
    <div class="manager-import-skill-row manager-import-skill-row--readonly">
      <div class="manager-skill-headline">
        <div class="manager-skill-headline__meta">
          <span class="manager-skill-name">导入结果</span>
        </div>
        <span :class="result.failedCount ? 'pill danger-pill' : 'pill success-pill'">
          {{ result.failedCount ? `${result.failedCount} 个失败` : "全部完成" }}
        </span>
      </div>
      <p class="manager-skill-description">
        成功 {{ result.succeededCount }} 个来源，导入 {{ result.importedCount }} 个 skills。
      </p>
    </div>

    <div
      v-for="(item, index) in result.items"
      :key="`${item.repo}-${item.ref ?? 'default'}-${index}`"
      class="manager-import-skill-row manager-import-skill-row--readonly"
    >
      <div class="manager-skill-headline">
        <div class="manager-skill-headline__meta">
          <span class="manager-skill-name">{{ item.repo }}</span>
          <span class="manager-skill-updated">{{ item.ref || "default" }}</span>
        </div>
        <span :class="item.error ? 'pill danger-pill' : 'pill success-pill'">
          {{ item.error ? "失败" : `${item.importedCount} 个` }}
        </span>
      </div>
      <p v-if="item.error" class="error-text">{{ item.error }}</p>
      <p v-else class="manager-skill-description">
        {{ item.skillNames.join(", ") || "没有匹配的 skills" }}
      </p>
    </div>
  </div>
</template>
