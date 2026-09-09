<script setup lang="ts">
import { computed } from "vue";
import { joinClasses } from "@shared/lib/join-classes";
import { formatTargetLabel } from "../model";
import type { AgentTargetId, McpTargetInspection } from "../types";

type McpTargetButtonProps = {
  serverName: string;
  targetId: AgentTargetId;
  targetItem: McpTargetInspection | null;
  loading?: boolean;
};

const props = withDefaults(defineProps<McpTargetButtonProps>(), {
  loading: false,
});

defineEmits<{
  toggle: [serverName: string, targetId: AgentTargetId];
}>();

const installed = computed(() => props.targetItem?.state === "present");
const warning = computed(
  () => props.targetItem?.state === "error" || props.targetItem?.state === "unconfigured",
);
const buttonStateClass = computed(() => {
  if (installed.value) return " is-installed";
  if (warning.value) return " is-warning";
  return "";
});
const label = computed(() => formatTargetLabel(props.targetId));
const detail = computed(
  () => props.targetItem?.detail ?? `${props.serverName} 在 ${label.value} 的状态未知`,
);
</script>

<template>
  <button
    :class="joinClasses('secondary-button', 'manager-target-button', buttonStateClass)"
    :disabled="loading"
    :title="detail"
    type="button"
    @click="$emit('toggle', serverName, targetId)"
  >
    {{ label }}
  </button>
</template>
