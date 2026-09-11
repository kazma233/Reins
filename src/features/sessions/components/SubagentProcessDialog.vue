<script setup lang="ts">
import { computed } from "vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import { formatTokenCount } from "@shared/lib/format";
import { isFailedSubagentRun, type SubagentRun } from "../subagent-payload";
import { buildTimelineItems } from "../timeline-group";
import SubagentFlow from "./SubagentFlow.vue";

// 单次子代理运行的过程弹窗：任务、用量、失败原因与完整嵌套时间线。
// 时间线里每个 run 各有一行入口，所以这里只呈现一个 run。
type SubagentProcessDialogProps = {
  open: boolean;
  run: SubagentRun;
};

const props = defineProps<SubagentProcessDialogProps>();

const emit = defineEmits<{ close: [] }>();

const titleId = "subagent-process-dialog-title";

const items = computed(() => buildTimelineItems(props.run.nestedMessages));
const failed = computed(() => isFailedSubagentRun(props.run));

const dialogTitle = computed(
  () =>
    `子代理过程 · ${props.run.agent ?? "子代理"}（${
      props.run.nestedMessages.length
    } 条消息）`
);

const usageChips = computed<Array<{ label: string; value: string }>>(() => {
  const usage = props.run.usage;
  if (!usage) {
    return [];
  }
  const chips: Array<{ label: string; value: string }> = [];
  if (usage.input) {
    chips.push({ label: "输入", value: formatTokenCount(usage.input) });
  }
  if (usage.output) {
    chips.push({ label: "输出", value: formatTokenCount(usage.output) });
  }
  if (usage.cacheRead) {
    chips.push({ label: "缓存读", value: formatTokenCount(usage.cacheRead) });
  }
  if (usage.turns) {
    chips.push({ label: "轮次", value: `${usage.turns}` });
  }
  if (usage.contextTokens) {
    chips.push({ label: "上下文", value: formatTokenCount(usage.contextTokens) });
  }
  const cost = typeof usage.cost === "number" ? usage.cost : usage.cost?.total;
  if (cost) {
    chips.push({ label: "成本", value: `$${cost.toFixed(4)}` });
  }
  return chips;
});
</script>

<template>
  <DialogShell
    :open="open"
    :title="dialogTitle"
    :title-id="titleId"
    dialog-class-name="subagent-run-dialog"
    @close="emit('close')"
  >
    <div class="subagent-run-header">
      <span class="subagent-badge strong">{{ run.agent ?? "子代理" }}</span>
      <span :class="failed ? 'subagent-run-state error' : 'subagent-run-state ok'">
        {{ failed ? "失败" : "完成" }}
      </span>
      <span v-if="failed && run.exitCode !== undefined" class="subagent-run-exit">
        退出码 {{ run.exitCode }}
      </span>
      <span v-if="run.model" class="subagent-usage-chip">
        <span class="subagent-usage-label">模型</span>
        <span class="subagent-usage-value">{{ run.model }}</span>
      </span>
    </div>
    <p v-if="run.task" class="subagent-dialog-task">
      <span class="subagent-dialog-task-label">任务</span>
      <span>{{ run.task }}</span>
    </p>
    <div
      v-if="usageChips.length > 0"
      class="subagent-run-usage subagent-dialog-usage"
    >
      <span v-for="chip in usageChips" :key="chip.label" class="subagent-usage-chip">
        <span class="subagent-usage-label">{{ chip.label }}</span>
        <span class="subagent-usage-value">{{ chip.value }}</span>
      </span>
    </div>
    <pre v-if="run.errorMessage" class="subagent-run-stderr">{{ run.errorMessage }}</pre>
    <pre v-if="run.stderr" class="subagent-run-stderr">{{ run.stderr }}</pre>
    <SubagentFlow v-if="items.length > 0" :items="items" />
    <p v-else class="muted-text">该运行没有留下过程消息。</p>
  </DialogShell>
</template>
