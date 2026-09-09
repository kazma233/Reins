<script setup lang="ts">
import type { SkillDiscoveryResult } from "../types";

type SkillDiscoveryPreviewProps = {
  discovery: SkillDiscoveryResult | null;
  emptyText: string;
  loading: boolean;
};

const props = defineProps<SkillDiscoveryPreviewProps>();
</script>

<template>
  <div v-if="loading || !discovery" class="empty-state">{{ emptyText }}</div>
  <template v-else>
    <div class="manager-import-skill-list manager-import-skill-list--compact">
      <template v-if="discovery.skills.length > 0">
        <div
          v-for="skill in discovery.skills"
          :key="`${discovery.discoveryId}-${skill.relativePath}-${skill.name}`"
          class="manager-import-skill-row manager-import-skill-row--compact"
        >
          <span class="manager-skill-name">{{ skill.name }}</span>
          <span class="manager-skill-path-label">{{ skill.relativePath }}</span>
        </div>
      </template>
      <div v-else class="empty-state">没有匹配 include 条件的 skills。</div>
    </div>
    <section
      class="manager-excluded-skill-section manager-excluded-skill-section--compact"
      aria-label="被排除的列表"
    >
      <div class="manager-excluded-skill-section__header">
        <span class="manager-excluded-skill-section__title">被排除的列表</span>
        <span class="manager-excluded-skill-section__count">
          {{ discovery.excludedSkills?.length ?? 0 }} 项
        </span>
      </div>
      <template v-if="(discovery.excludedSkills?.length ?? 0) > 0">
        <div class="manager-excluded-skill-section__list manager-import-skill-list--compact">
          <div
            v-for="skill in discovery.excludedSkills"
            :key="`${discovery.discoveryId}-excluded-${skill.relativePath}-${skill.name}`"
            class="manager-import-skill-row manager-import-skill-row--compact"
          >
            <span class="manager-skill-name">{{ skill.name }}</span>
            <span class="manager-skill-path-label">{{ skill.relativePath }}</span>
          </div>
        </div>
      </template>
      <div v-else class="empty-state manager-excluded-skill-section__empty">暂无被排除的 skills。</div>
    </section>
  </template>
</template>
