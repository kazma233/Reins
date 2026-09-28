<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import AppInput from "@shared/ui/AppInput.vue";
import AppSelect from "@shared/ui/AppSelect.vue";
import type { ModelsDevMeta, ProviderModelInput } from "../../generated";
import {
  applyModelsDevMeta,
  parseReasoningLevels,
  reasoningLevelsText,
  toggleReasoning,
} from "../../model";
import { useProvidersNotice } from "../../composables/useProvidersNotice";
import ModelsDevCompleteDialog from "./ModelsDevCompleteDialog.vue";

type ProviderModelDialogProps = {
  open: boolean;
  // models.dev 补全按提供商名 + 模型 ID 查公共目录，新增/编辑态都用表单里的提供商 ID。
  providerId: string;
};

const props = defineProps<ProviderModelDialogProps>();

const emit = defineEmits<{
  close: [];
  confirm: [model: ProviderModelInput];
}>();

const { showNotice } = useProvidersNotice();

function emptyDraft() {
  return {
    id: "",
    label: "",
    contextWindow: null as number | null,
    maxOutputTokens: null as number | null,
    supportsImages: null as boolean | null,
    reasoning: null as boolean | null,
    reasoningLevels: null as ProviderModelInput["reasoningLevels"],
  };
}

const draft = reactive(emptyDraft());
// models.dev 补全查询由弹窗自持，这里只保留开关。
const modelsDevOpen = ref(false);

// 可空布尔三态在 AppSelect 的 string 契约下用 '' 表示未设置。
const TRI_STATE_OPTIONS = [
  { value: "", label: "未设置" },
  { value: "true", label: "支持" },
  { value: "false", label: "不支持" },
];

const supportsImagesValue = computed({
  get: () => (draft.supportsImages === null ? "" : String(draft.supportsImages)),
  set: (value: string) => {
    draft.supportsImages = value === "" ? null : value === "true";
  },
});

const reasoningValue = computed({
  get: () => (draft.reasoning === null ? "" : String(draft.reasoning)),
  set: (value: string) => {
    if (value === "") {
      draft.reasoning = null;
      return;
    }
    toggleReasoning(draft, value === "true");
  },
});

watch(
  () => props.open,
  (open) => {
    if (open) {
      Object.assign(draft, emptyDraft());
      modelsDevOpen.value = false;
    }
  }
);

// 弹窗命中的元数据回填当前草稿的空缺字段；请求失败/未命中的重试也在弹窗内。
function applyModelsDevCompletion(meta: ModelsDevMeta) {
  modelsDevOpen.value = false;
  applyModelsDevMeta(draft, meta);
  showNotice("已从 models.dev 预填空缺字段。", "success");
}

function confirm() {
  const id = draft.id.trim();
  if (!id) {
    showNotice("请先填写模型 ID。", "error");
    return;
  }
  emit("confirm", {
    id,
    label: draft.label.trim(),
    contextWindow: draft.contextWindow,
    maxOutputTokens: draft.maxOutputTokens,
    supportsImages: draft.supportsImages,
    reasoning: draft.reasoning,
    reasoningLevels: draft.reasoningLevels,
  });
}
</script>

<template>
  <DialogShell
    :open="open"
    dialog-class-name="providers-model-dialog"
    eyebrow="手工新增"
    title="新增模型"
    title-id="provider-model-dialog-title"
    @close="$emit('close')"
  >
    <template #actions>
      <button class="primary-button" type="button" @click="confirm">加入</button>
    </template>

    <div class="providers-form-grid">
      <label class="providers-field">
        <span>模型 ID</span>
        <AppInput v-model="draft.id" data-autofocus />
      </label>
      <label class="providers-field">
        <span>显示名</span>
        <AppInput v-model="draft.label" />
      </label>
    </div>

    <div class="providers-card__actions" style="margin-top: 12px">
      <button
        class="secondary-button"
        :disabled="!providerId.trim() || !draft.id.trim()"
        type="button"
        @click="modelsDevOpen = true"
      >
        从 models.dev 补全
      </button>
    </div>
    <p class="providers-section-hint">
      元数据（上下文窗口、最大输出、图像、思考等级）会随应用写入目标工具；缺失时对应工具可能展示能力警告。
    </p>

    <div class="providers-model-row__grid" style="margin-top: 12px">
      <label class="providers-field">
        <span>上下文窗口</span>
        <AppInput v-model.number="draft.contextWindow" size="sm" type="number" />
      </label>
      <label class="providers-field">
        <span>最大输出</span>
        <AppInput v-model.number="draft.maxOutputTokens" size="sm" type="number" />
      </label>
      <div class="providers-field">
        <AppSelect
          v-model="supportsImagesValue"
          variant="stacked"
          label="图像输入"
          :options="TRI_STATE_OPTIONS"
        />
      </div>
    </div>

    <div class="providers-model-row__grid" style="margin-top: 12px">
      <div class="providers-field">
        <AppSelect
          v-model="reasoningValue"
          variant="stacked"
          label="推理能力"
          :options="TRI_STATE_OPTIONS"
        />
      </div>
      <label class="providers-field" style="grid-column: span 2">
        <span>思考等级（逗号分隔：off/minimal/low/medium/high/xhigh/max）</span>
        <AppInput
          :model-value="reasoningLevelsText(draft.reasoningLevels)"
          size="sm"
          @update:model-value="draft.reasoningLevels = parseReasoningLevels(String($event))"
        />
      </label>
    </div>

    <ModelsDevCompleteDialog
      :open="modelsDevOpen"
      :provider-id="providerId.trim()"
      :model-id="draft.id.trim()"
      @close="modelsDevOpen = false"
      @apply="applyModelsDevCompletion"
    />
  </DialogShell>
</template>
