<script setup lang="ts">
import { computed } from "vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import type { AgentTargetId, TargetConfigView } from "../../types";

type ProjectAgentPickerDialogProps = {
  open: boolean;
  loading: boolean;
  agents: TargetConfigView[];
  contextName: string;
  projectId: string;
  installedAgentIds: Set<AgentTargetId>;
  selectedAgentId: AgentTargetId | null;
};

const props = defineProps<ProjectAgentPickerDialogProps>();

const emit = defineEmits<{
  close: [];
  confirm: [agentId: AgentTargetId];
  selectedAgentChange: [agentId: AgentTargetId];
}>();

type PickerTone = "installed" | "warning" | "idle";

const TONE_BUTTON_CLASS: Record<PickerTone, string> = {
  installed: "is-installed",
  warning: "is-warning",
  idle: "",
};

const TONE_PILL_CLASS: Record<PickerTone, string> = {
  installed: "success-pill",
  warning: "danger-pill",
  idle: "",
};

function stateLabel(state: string | undefined): { label: string; tone: PickerTone } {
  if (!state || state === "missing") {
    return { label: "未安装", tone: "idle" };
  }
  if (state === "installed" || state === "present") {
    return { label: "已安装", tone: "installed" };
  }
  if (state === "conflict" || state === "broken" || state === "error") {
    return { label: "异常", tone: "warning" };
  }
  return { label: "未就绪", tone: "idle" };
}

const effectiveSelected = computed<AgentTargetId | null>(
  () => props.selectedAgentId ?? props.agents[0]?.id ?? null,
);
const effectiveCompositeId = computed<AgentTargetId | null>(() =>
  effectiveSelected.value
    ? (`${props.projectId}:${effectiveSelected.value}` as AgentTargetId)
    : null,
);
const isInstalledSelected = computed(
  () =>
    effectiveCompositeId.value !== null &&
    props.installedAgentIds.has(effectiveCompositeId.value),
);
const canConfirm = computed(
  () => effectiveSelected.value !== null && !props.loading,
);
const confirmButtonClassName = computed(() =>
  isInstalledSelected.value ? "danger-button" : "primary-button",
);

const title = computed(
  () => `${props.contextName} · MCP 项目目标:${props.projectId}`,
);
const eyebrow = "MCP · Project";
const confirmLabel = computed(() => {
  if (!effectiveSelected.value) return "请选择 agent";
  const compositeId = `${props.projectId}:${effectiveSelected.value}` as AgentTargetId;
  const alreadyInstalled = props.installedAgentIds.has(compositeId);
  return alreadyInstalled
    ? `从 ${effectiveSelected.value} 移除 MCP`
    : `应用 MCP 到 ${effectiveSelected.value}`;
});

function agentStateInfo(agent: TargetConfigView) {
  const compositeId = `${props.projectId}:${agent.id}` as AgentTargetId;
  const state = props.installedAgentIds.has(compositeId) ? "installed" : undefined;
  return stateLabel(state);
}

function agentButtonClass(agent: TargetConfigView): string {
  const isSelected = effectiveSelected.value === agent.id;
  const stateInfo = agentStateInfo(agent);
  const stateClass = TONE_BUTTON_CLASS[stateInfo.tone];
  return [
    "secondary-button",
    "manager-target-button",
    isSelected && "is-active",
    stateClass,
  ]
    .filter(Boolean)
    .join(" ");
}

function agentPillClass(agent: TargetConfigView): string {
  const stateInfo = agentStateInfo(agent);
  const pillToneClass = TONE_PILL_CLASS[stateInfo.tone];
  return ["pill", pillToneClass].filter(Boolean).join(" ");
}

function handleConfirm() {
  if (!effectiveSelected.value) return;
  emit("confirm", effectiveSelected.value);
}
</script>

<template>
  <DialogShell
    :open="open"
    dialog-class-name="manager-import-dialog"
    :eyebrow="eyebrow"
    :title="title"
    title-id="project-agent-picker-dialog-title"
    :close-disabled="loading"
    @close="$emit('close')"
  >
    <template #actions>
      <button
        :class="confirmButtonClassName"
        :disabled="!canConfirm"
        type="button"
        @click="handleConfirm"
      >
        {{ confirmLabel }}
      </button>
    </template>

    <div class="manager-stack">
      <p class="manager-field__hint">
        同步到项目内的目标,会写入该项目目录下的 MCP 配置文件。
      </p>
      <div class="manager-target-buttons">
        <button
          v-for="agent in agents"
          :key="agent.id"
          :class="agentButtonClass(agent)"
          :disabled="loading"
          type="button"
          @click="$emit('selectedAgentChange', agent.id)"
        >
          <span class="manager-target-button__label">{{ agent.id }}</span>
          <span :class="agentPillClass(agent)">
            {{ agentStateInfo(agent).label }}
          </span>
        </button>
      </div>
    </div>
  </DialogShell>
</template>
