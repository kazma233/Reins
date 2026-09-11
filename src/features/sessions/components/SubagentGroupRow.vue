<script setup lang="ts">
import type { SubagentGroupItem } from "../timeline-group";

// family 聚合型子代理（Codex/Claude Code/OpenCode）的入口行：
// 只暴露一行(标题),点击由父组件按需取该 agent 的完整消息后打开过程弹窗。
type SubagentGroupRowProps = {
  item: SubagentGroupItem;
};

defineProps<SubagentGroupRowProps>();

const emit = defineEmits<{ open: [sessionId: string, label: string] }>();
</script>

<template>
  <div class="flow-subagent">
    <button
      class="flow-subagent-row"
      type="button"
      @click="emit('open', item.sessionId, item.label)"
    >
      <span class="subagent-badge strong">子代理</span>
      <strong class="flow-subagent-name">{{ item.label }}</strong>
      <span class="flow-tool-chevron">▸</span>
    </button>
  </div>
</template>
