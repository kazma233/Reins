<script setup lang="ts">
import { computed } from "vue";
import { joinClasses } from "@shared/lib/join-classes";
import type { AgentTargetId, McpInspection } from "../types";

type ProjectTargetEntry = {
  id: string;
  agents: { id: AgentTargetId }[];
};

type ProjectMcpTargetButtonProps = {
  serverName: string;
  projectEntry: ProjectTargetEntry;
  inspection: McpInspection | null;
  loading?: boolean;
};

const props = withDefaults(defineProps<ProjectMcpTargetButtonProps>(), {
  loading: false,
});

defineEmits<{
  click: [serverName: string, projectId: string];
}>();

const projectTargetItems = computed(() => {
  const prefix = `${props.projectEntry.id}:`;
  return (props.inspection?.targets ?? []).filter((target) =>
    target.targetId.startsWith(prefix),
  );
});

const enabledAgentCount = computed(() => props.projectEntry.agents.length);
const installedCount = computed(
  () => projectTargetItems.value.filter((item) => item.state === "present").length,
);
const warning = computed(() =>
  projectTargetItems.value.some(
    (item) => item.state === "error" || item.state === "unconfigured",
  ),
);
const installed = computed(
  () => enabledAgentCount.value > 0 && installedCount.value >= enabledAgentCount.value,
);
const partial = computed(() => !installed.value && installedCount.value > 0);

const buttonStateClass = computed(() => {
  if (installed.value) return " is-installed";
  if (warning.value) return " is-warning";
  if (partial.value) return " is-partial";
  return "";
});

const detail = computed(() =>
  projectTargetItems.value.map((item) => item.detail).join("\n"),
);
</script>

<template>
  <button
    :class="joinClasses('secondary-button', 'manager-target-button', buttonStateClass)"
    :disabled="loading"
    :title="detail || `点击选择 ${projectEntry.id} 项目内的 agent`"
    type="button"
    @click="$emit('click', serverName, projectEntry.id)"
  >
    {{ projectEntry.id }}
  </button>
</template>
