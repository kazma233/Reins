<script setup lang="ts">
import { ref } from "vue";
import type { SubagentEntryItem } from "../timeline-group";
import SubagentProcessDialog from "./SubagentProcessDialog.vue";

type SubagentEntryRowProps = {
  item: SubagentEntryItem;
};

defineProps<SubagentEntryRowProps>();

const dialogOpen = ref(false);
</script>

<template>
  <div class="flow-subagent">
    <button class="flow-subagent-row" type="button" @click="dialogOpen = true">
      <span class="subagent-badge strong">子代理</span>
      <strong class="flow-subagent-name">{{ item.label }}</strong>
      <span
        :class="
          item.status === 'error'
            ? 'subagent-run-state error'
            : 'subagent-run-state ok'
        "
      >
        {{ item.status === "error" ? "失败" : "完成" }}
      </span>
      <span v-if="item.title" class="flow-subagent-title">{{ item.title }}</span>
      <span class="flow-tool-chevron">▸</span>
    </button>
    <SubagentProcessDialog
      :open="dialogOpen"
      :run="item.run"
      @close="dialogOpen = false"
    />
  </div>
</template>
