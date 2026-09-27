<script setup lang="ts">
import { computed, ref, watch } from "vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import AppSelect from "@shared/ui/AppSelect.vue";
import { extractErrorMessage } from "@shared/lib/errors";
import { previewProviderApply } from "../../api";
import type {
  ApplyProviderInput,
  DiffLine,
  ProviderApplyPreview,
  ProviderAppId,
  ProviderView,
  ProvidersState,
  ReasoningLevel,
} from "../../generated";
import {
  APP_LABELS,
  applyBlockers,
  entriesForProvider,
  findAppState,
  reasoningEffortWrite,
  reasoningLevelOptions,
  REASONING_LEVEL_LABELS,
  unwrittenModelFieldsText,
} from "../../model";
import { useProvidersNotice } from "../../composables/useProvidersNotice";

type ProviderApplyDialogProps = {
  open: boolean;
  loading: boolean;
  app: ProviderAppId;
  provider: ProviderView;
  state: ProvidersState | null;
};

const props = defineProps<ProviderApplyDialogProps>();

const emit = defineEmits<{
  close: [];
  confirm: [input: ApplyProviderInput];
}>();

const { showNotice } = useProvidersNotice();

const selectedModelIds = ref<string[]>([]);
const defaultModelId = ref("");
const defaultReasoningLevel = ref<ReasoningLevel | null>(null);
// 可空枚举在 AppSelect 的 string 契约下用 '' 表示不设置。
const defaultReasoningLevelValue = computed({
  get: () => defaultReasoningLevel.value ?? "",
  set: (value: string) => {
    defaultReasoningLevel.value = value === "" ? null : (value as ReasoningLevel);
  },
});
const preview = ref<ProviderApplyPreview | null>(null);
const previewLoading = ref(false);

// 父组件在打开时才挂载本弹窗，且 open 与 app/providerId 在同一次更新里置位，
// 首次挂载时 open 已是 true 而 watch 不会触发；必须 immediate 才能做首次初始化。
watch(
  () => props.open,
  (open) => {
    if (!open) {
      return;
    }
    preview.value = null;
    defaultReasoningLevel.value = null;
    const appState = findAppState(props.state, props.app);
    const previouslyApplied = appState
      ? entriesForProvider(appState, props.provider.id).flatMap(
          (entry) => entry.modelIds
        )
      : [];
    const known = previouslyApplied.filter((modelId) =>
      props.provider.models.some((model) => model.id === modelId)
    );
    selectedModelIds.value = known.length > 0 ? known : props.provider.models.map((model) => model.id);
    defaultModelId.value = selectedModelIds.value[0] ?? "";
  },
  { immediate: true }
);

const appState = computed(() => findAppState(props.state, props.app));
const additive = computed(() => appState.value?.additive ?? true);
// 该工具不写入的模型元数据提示；无此类字段时为 null 不展示。
const unwrittenFieldsText = computed(() =>
  appState.value ? unwrittenModelFieldsText(appState.value) : null
);

const blockers = computed(() =>
  appState.value ? applyBlockers(appState.value, props.provider, selectedModelIds.value) : []
);

const levelOptions = computed<ReasoningLevel[]>(() => {
  if (!appState.value) {
    return [];
  }
  const selected = props.provider.models.filter((model) =>
    selectedModelIds.value.includes(model.id)
  );
  // 取所有选中模型可用等级的交集；任一模型未约束则不收紧。
  let options: ReasoningLevel[] | null = null;
  for (const model of selected) {
    const modelOptions = reasoningLevelOptions(appState.value, model);
    options = options === null ? modelOptions : options.filter((level) => modelOptions.includes(level));
  }
  return options ?? [];
});

// 档位展示跟随当前提供商：写入值与档位同名时只显示档位，存在映射
// （如 Grok max→xhigh、Pi off→off）时附加实际写入值。
function levelOptionLabel(level: ReasoningLevel): string {
  const base = `${REASONING_LEVEL_LABELS[level]}（${level}）`;
  if (!appState.value) {
    return base;
  }
  const writes = reasoningEffortWrite(appState.value, level);
  return writes === level ? base : `${REASONING_LEVEL_LABELS[level]}（${level}，写入 ${writes}）`;
}

function toggleModel(modelId: string, checked: boolean) {
  if (checked) {
    if (!selectedModelIds.value.includes(modelId)) {
      selectedModelIds.value.push(modelId);
    }
    if (!defaultModelId.value) {
      defaultModelId.value = modelId;
    }
  } else {
    selectedModelIds.value = selectedModelIds.value.filter((id) => id !== modelId);
    if (defaultModelId.value === modelId) {
      defaultModelId.value = selectedModelIds.value[0] ?? "";
    }
  }
}

function buildInput(): ApplyProviderInput {
  return {
    providerId: props.provider.id,
    app: props.app,
    modelIds: selectedModelIds.value,
    defaultModelId: defaultModelId.value,
    defaultReasoningLevel: levelOptions.value.includes(defaultReasoningLevel.value as ReasoningLevel)
      ? defaultReasoningLevel.value
      : null,
  };
}

async function runPreview() {
  previewLoading.value = true;
  try {
    preview.value = await previewProviderApply(buildInput());
  } catch (error) {
    // Tauri invoke 的错误是字符串，extractErrorMessage 才能透出后端原因。
    showNotice(extractErrorMessage(error, "生成预览失败。"), "error");
    preview.value = null;
  } finally {
    previewLoading.value = false;
  }
}

function confirm() {
  emit("confirm", buildInput());
}

const previewText = computed(() => {
  if (!preview.value) {
    return "";
  }
  return preview.value.files
    .map((file) => `===== ${file.path} =====\n${renderDiff(file.diff)}`)
    .join("\n\n");
});

function renderDiff(lines: DiffLine[]): string {
  return lines
    .map((line) => {
      if (line.kind === "add") {
        return `+ ${line.text}`;
      }
      if (line.kind === "remove") {
        return `- ${line.text}`;
      }
      return `  ${line.text}`;
    })
    .join("\n");
}
</script>

<template>
  <DialogShell
    :open="open"
    dialog-class-name="providers-apply-dialog"
    eyebrow="应用到工具"
    :title="`应用 ${provider.label} 到 ${APP_LABELS[app]}`"
    title-id="provider-apply-dialog-title"
    :close-disabled="loading"
    @close="$emit('close')"
  >
    <template #actions>
      <button
        class="secondary-button"
        :disabled="loading || blockers.length > 0 || previewLoading"
        type="button"
        @click="runPreview"
      >
        {{ previewLoading ? "生成中…" : "预览变更" }}
      </button>
      <button
        class="primary-button"
        :disabled="loading || blockers.length > 0"
        type="button"
        @click="confirm"
      >
        确认应用
      </button>
    </template>

    <div class="providers-form-grid providers-form-grid--full">
      <p class="providers-section-hint">
        {{
          additive
            ? "该工具支持多个聚合提供商并存，本次应用只新增/更新本提供商的条目。"
            : "该工具只保留一个活动聚合提供商，本次应用会替换旧提供商的配置。"
        }}
      </p>

      <p v-if="unwrittenFieldsText" class="providers-section-hint providers-unwritten-fields">
        {{ unwrittenFieldsText }}
      </p>

      <ul v-if="blockers.length > 0" class="providers-blockers">
        <li v-for="blocker in blockers" :key="blocker">{{ blocker }}</li>
      </ul>

      <div class="providers-model-list">
        <label
          v-for="model in provider.models"
          :key="model.id"
          class="providers-fetched-item"
        >
          <input
            type="checkbox"
            :checked="selectedModelIds.includes(model.id)"
            :disabled="loading"
            @change="toggleModel(model.id, ($event.target as HTMLInputElement).checked)"
          />
          <span>
            {{ model.id }}<template v-if="model.label && model.label !== model.id"> · {{ model.label }}</template>
          </span>
        </label>
      </div>

      <div class="providers-form-grid">
        <div class="providers-field">
          <AppSelect
            v-model="defaultModelId"
            variant="stacked"
            label="默认模型"
            :options="selectedModelIds.map((modelId) => ({ value: modelId, label: modelId }))"
            :disabled="loading"
          />
        </div>

        <div class="providers-field">
          <AppSelect
            v-model="defaultReasoningLevelValue"
            variant="stacked"
            label="默认思考等级（可选）"
            :options="[
              { value: '', label: '不设置' },
              ...levelOptions.map((level) => ({
                value: level,
                label: levelOptionLabel(level),
              })),
            ]"
            :disabled="loading || levelOptions.length === 0"
          />
          <small class="providers-field__hint">
            {{ levelOptions.length === 0 ? "当前选中的模型在该工具下没有可用的思考等级，将不写入。" : "取模型等级与工具支持集的交集；标注「写入」的档位按该工具的实际取值落盘。" }}
          </small>
        </div>
      </div>

      <div v-if="preview">
        <h4 class="providers-section-title">变更预览（密钥已脱敏）</h4>
        <pre class="providers-diff">{{ previewText }}</pre>
        <ul v-if="preview.warnings.length > 0" class="providers-warnings">
          <li v-for="warning in preview.warnings" :key="warning">{{ warning }}</li>
        </ul>
      </div>
    </div>
  </DialogShell>
</template>
