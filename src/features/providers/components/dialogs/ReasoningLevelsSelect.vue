<script setup lang="ts">
import { computed } from "vue";
import AppMultiSelect from "@shared/ui/AppMultiSelect.vue";
import type { AppMultiSelectOption } from "@shared/ui/AppMultiSelect.vue";
import { REASONING_LEVEL_LABELS } from "../../model";
import type { ReasoningLevel } from "../../generated";

// 思考等级多选：模型可支持多个档位；null/undefined 表示未设置（应用时不写该元数据）。
type ReasoningLevelsSelectProps = {
  disabled?: boolean;
};

const props = defineProps<ReasoningLevelsSelectProps>();

const levels = defineModel<ReasoningLevel[] | null | undefined>();

const ORDER: ReasoningLevel[] = [
  "off",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
];

const OPTIONS: AppMultiSelectOption[] = ORDER.map((level) => ({
  value: level,
  label: `${REASONING_LEVEL_LABELS[level]}（${level}）`,
  summary: REASONING_LEVEL_LABELS[level],
}));

// AppMultiSelect 契约是 string[]；这里负责与「空数组 = 未设置(null)」的领域语义互转。
const selected = computed<string[]>({
  get: () => levels.value ?? [],
  set: (value) => {
    levels.value = value.length > 0 ? (value as ReasoningLevel[]) : null;
  },
});
</script>

<template>
  <AppMultiSelect
    v-model="selected"
    aria-label="思考等级"
    size="sm"
    placeholder="未设置"
    :options="OPTIONS"
    :disabled="props.disabled"
  />
</template>
