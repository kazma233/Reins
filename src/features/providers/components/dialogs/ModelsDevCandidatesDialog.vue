<script setup lang="ts">
import { ref, watch } from "vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import type { ModelsDevCandidate } from "../../generated";

// models.dev 返回多个候选时的选择弹窗：单选一个来源后回填空缺元数据。
type ProviderCandidatesDialogProps = {
  open: boolean;
  modelId: string;
  candidates: ModelsDevCandidate[];
};

const props = defineProps<ProviderCandidatesDialogProps>();

const emit = defineEmits<{
  close: [];
  confirm: [candidate: ModelsDevCandidate];
}>();

const selected = ref(0);

watch(
  () => props.open,
  (open) => {
    if (open) {
      selected.value = 0;
    }
  }
);

function confirm() {
  const candidate = props.candidates[selected.value];
  if (candidate) {
    emit("confirm", candidate);
  }
}
</script>

<template>
  <DialogShell
    :open="open"
    dialog-class-name="providers-candidates-dialog"
    eyebrow="从 models.dev 补全"
    :title="`「${modelId}」有多个匹配来源`"
    title-id="modelsdev-candidates-dialog-title"
    @close="$emit('close')"
  >
    <p class="providers-section-hint">
      models.dev 返回多个匹配，请选择实际的来源提供商；确认后预填该行的空缺元数据。
    </p>

    <div class="providers-model-list">
      <label
        v-for="(candidate, index) in candidates"
        :key="`${candidate.provider}-${candidate.modelId}`"
        class="providers-fetched-item"
      >
        <input v-model="selected" type="radio" :value="index" />
        <span>
          {{ candidate.provider }} · {{ candidate.modelId
          }}<template v-if="candidate.label"> · {{ candidate.label }}</template>
        </span>
      </label>
    </div>

    <template #actions>
      <button class="primary-button" type="button" @click="confirm">
        使用该来源补全
      </button>
    </template>
  </DialogShell>
</template>
