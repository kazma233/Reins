<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import AppFieldError from "@shared/ui/AppFieldError.vue";
import AppInput from "@shared/ui/AppInput.vue";
import AppSelect from "@shared/ui/AppSelect.vue";
import type { ModelsDevMeta, ProviderModelInput } from "../../generated";
import {
  applyModelsDevMeta,
  hasModelId,
  parseReasoningLevels,
  reasoningLevelsText,
  toggleReasoning,
} from "../../model";
import ModelsDevCompleteDialog from "./ModelsDevCompleteDialog.vue";

type ProviderModelDialogProps = {
  open: boolean;
  // models.dev 补全按提供商名 + 模型 ID 查公共目录，新增/编辑态都用表单里的提供商 ID。
  providerId: string;
  // 当前模型目录里已有的模型 ID：命中时拒绝加入并弹窗提示。
  existingIds: string[];
};

const props = defineProps<ProviderModelDialogProps>();

const emit = defineEmits<{
  close: [];
  confirm: [model: ProviderModelInput];
}>();

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
// 提交的重名提示，内容是命中的模型 ID。
const duplicatePromptId = ref("");
// 模型 ID 缺失的字段错误：用户开始输入就清空。
const idError = ref<string | null>(null);

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
      duplicatePromptId.value = "";
      idError.value = null;
    }
  }
);

watch(
  () => draft.id,
  () => {
    idError.value = null;
  }
);

// 弹窗命中的元数据回填当前草稿的空缺字段；请求失败/未命中的重试也在弹窗内。
function applyModelsDevCompletion(meta: ModelsDevMeta) {
  modelsDevOpen.value = false;
  applyModelsDevMeta(draft, meta);
}

function confirm() {
  const id = draft.id.trim();
  if (!id) {
    idError.value = "请先填写模型 ID。";
    return;
  }
  if (hasModelId(props.existingIds, id)) {
    duplicatePromptId.value = id;
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
        <AppInput
          v-model="draft.id"
          :aria-describedby="idError ? 'provider-model-id-error' : undefined"
          data-autofocus
        />
        <AppFieldError id="provider-model-id-error" :message="idError" />
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
        :title="
          !providerId.trim() || !draft.id.trim() ? '请先填写提供商 ID 与模型 ID。' : ''
        "
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

    <ConfirmDialog
      :open="duplicatePromptId !== ''"
      title-id="provider-model-duplicate-title"
      eyebrow="模型目录"
      title="模型 ID 已存在"
      :description="`「${duplicatePromptId}」已在当前模型目录里，请换一个 ID，或直接编辑已有条目。`"
      confirm-label="知道了"
      confirm-button-class-name="primary-button"
      @close="duplicatePromptId = ''"
      @confirm="duplicatePromptId = ''"
    />
  </DialogShell>
</template>
