<script setup lang="ts">
import SourceRow from "./SourceRow.vue";
import type { SkillSourceConfigView } from "../types";

type SkillSourceGroupProps = {
  heading: string;
  headingId: string;
  sources: SkillSourceConfigView[];
  loading: boolean;
};

defineProps<SkillSourceGroupProps>();

defineEmits<{
  edit: [source: SkillSourceConfigView];
  delete: [source: SkillSourceConfigView];
  sync: [source: SkillSourceConfigView];
  removeSync: [source: SkillSourceConfigView];
  refresh: [source: SkillSourceConfigView];
}>();
</script>

<template>
  <section v-if="sources.length" class="manager-skill-source-group" :aria-labelledby="headingId">
    <header class="manager-skill-source-group__header">
      <h2 :id="headingId">{{ heading }}</h2>
      <span>{{ sources.length }} 项</span>
    </header>
    <div class="manager-skill-source-group__list">
      <SourceRow
        v-for="source in sources"
        :key="source.id"
        :loading="loading"
        :source="source"
        @edit="$emit('edit', $event)"
        @delete="$emit('delete', $event)"
        @sync="$emit('sync', $event)"
        @remove-sync="$emit('removeSync', $event)"
        @refresh="$emit('refresh', $event)"
      />
    </div>
  </section>
</template>
