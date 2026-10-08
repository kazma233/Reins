<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import AppFieldError from "@shared/ui/AppFieldError.vue";
import AppInput from "@shared/ui/AppInput.vue";
import AppSelect from "@shared/ui/AppSelect.vue";
import AppSecretInput from "@shared/ui/AppSecretInput.vue";
import type { FetchedModel, ModelsDevMeta, ProviderModelInput } from "../../generated";
import {
  applyModelsDevMeta,
  emptyModelForm,
  mergeFetchedModels,
  normalizeProviderIdInput,
  PROTOCOL_OPTIONS,
  toggleReasoning,
  type ProviderFormState,
} from "../../model";
import ModelsDevCompleteDialog from "./ModelsDevCompleteDialog.vue";
import ProviderFetchModelsDialog from "./ProviderFetchModelsDialog.vue";
import ProviderModelDialog from "./ProviderModelDialog.vue";
import ReasoningLevelsSelect from "./ReasoningLevelsSelect.vue";

type ProviderEditDialogProps = {
  open: boolean;
  loading: boolean;
  form: ProviderFormState;
  // 仅编辑态有意义：该提供商当前是否已设 Key；明文已随 form.apiKey 下发。
  keyPresent: boolean;
  // 新增流程分两步：step 1 服务商元数据，step 2 模型目录。
  // 编辑态不走两步控制，元数据与模型一屏编辑。
  creating: boolean;
  step: 1 | 2;
};

const props = defineProps<ProviderEditDialogProps>();

const emit = defineEmits<{
  close: [];
  confirm: [];
}>();

// models.dev 补全的查询状态：弹窗打开后自行发请求，回填落回对应模型行。
const modelsDevDialog = reactive({
  open: false,
  rowKey: "",
  providerId: "",
  modelId: "",
});
// 可选密钥：随表单状态一并落盘，弹窗不再单独拉取。
const fetchDialogOpen = ref(false);
const modelDialogOpen = ref(false);
// 加入时被跳过的重名模型 ID，非空时弹窗提示。
const skippedPromptIds = ref<string[]>([]);

// 新增 step 1 只展示服务商元数据；step 2（及编辑态）展示模型目录。
const showMeta = computed(() => !props.creating || props.step === 1);
const showModels = computed(() => !props.creating || props.step === 2);
const confirmLabel = computed(() => {
  if (props.creating && props.step === 1) {
    return "下一步";
  }
  return props.creating ? "完成" : "保存";
});

const apiKeyHint = computed(() => {
  if (props.form.originalProviderId === null) {
    return "明文写入 providers.yaml，不加密；留空表示不设置。";
  }
  return props.keyPresent
    ? "当前已存 Key（明文显示）；修改后保存将覆盖，清空保存即删除。"
    : "未设置。填写后随保存明文写入 providers.yaml。";
});

const dialogTitle = computed(() => {
  if (!props.creating) {
    return "编辑提供商";
  }
  // 新增第二步提供商已落库，但流程仍是「新增」，标题不切到编辑。
  return props.step === 1 ? "新增提供商" : "新增提供商 · 模型目录";
});

// 新增流程第一步的成功结果没有弹窗外的反馈位置：进入第二步后把已保存的
// 提供商 ID 常驻在弹窗内，提示当前处于哪一步、接下来做什么。
const flowHint = computed(() => {
  if (props.creating && props.step === 2 && props.form.originalProviderId) {
    return `提供商 ${props.form.originalProviderId} 已保存，请继续设置模型目录。`;
  }
  return null;
});

// models.dev 补全的查询键：已保存的提供商用落库 ID（originalProviderId），
// 新增第一步还没保存时退回表单 ID。
const modelsDevProviderId = computed(() =>
  (props.form.originalProviderId ?? props.form.providerId).trim()
);

type ProviderFormErrorField = keyof NonNullable<ProviderFormState["errors"]>;

// 字段错误只在用户改到该字段时消失：改完重新具备提交条件。
function clearFieldError(field: ProviderFormErrorField) {
  const errors = props.form.errors;
  if (errors?.[field]) {
    errors[field] = undefined;
  }
}

watch(() => props.form.providerId, () => clearFieldError("providerId"));
watch(() => props.form.label, () => clearFieldError("label"));
watch(() => props.form.baseUrl, () => clearFieldError("baseUrl"));
watch(
  () => props.form.models.length,
  (length) => {
    if (length > 0) {
      clearFieldError("models");
    }
  }
);

watch(
  () => props.open,
  (open) => {
    if (open) {
      resetTransient();
    }
  }
);

function resetTransient() {
  modelsDevDialog.open = false;
  modelsDevDialog.rowKey = "";
  modelsDevDialog.providerId = "";
  modelsDevDialog.modelId = "";
  fetchDialogOpen.value = false;
  modelDialogOpen.value = false;
  skippedPromptIds.value = [];
}

// Key 可选：留空时面板只保存提供商元数据，同时清除已存 Key。
function confirm() {
  emit("confirm");
}

// 拉取/新增都在子弹窗内完成，确认后把结果落回模型目录。
function addFetchedModels(models: FetchedModel[]) {
  const { added, skipped } = mergeFetchedModels(props.form.models, models);
  props.form.models.push(...added);
  fetchDialogOpen.value = false;
  // 拉取结果内部重名（弹窗标记覆盖不到）时，被忽略项也弹窗提示。
  skippedPromptIds.value = skipped.map((model) => model.id);
}

function addModelRow(model: ProviderModelInput) {
  props.form.models.push({ ...emptyModelForm(model.id), ...model });
  modelDialogOpen.value = false;
}

function removeModel(index: number) {
  // 移除行时同步关掉该行的补全弹窗，避免弹窗回填找不到目标行。
  const rowKey = props.form.models[index]?.rowKey;
  if (rowKey && modelsDevDialog.rowKey === rowKey) {
    modelsDevDialog.open = false;
  }
  props.form.models.splice(index, 1);
}

function openModelsDevDialog(row: ProviderFormState["models"][number]) {
  // 缺失查询键时按钮已禁用，这里再做一次防御非按钮路径的调用。
  const providerId = modelsDevProviderId.value;
  const modelId = row.id.trim();
  if (!providerId || !modelId) {
    return;
  }
  modelsDevDialog.rowKey = row.rowKey;
  modelsDevDialog.providerId = providerId;
  modelsDevDialog.modelId = modelId;
  modelsDevDialog.open = true;
}

// 弹窗命中的元数据回填到发起查询的模型行。
function applyModelsDevCompletion(meta: ModelsDevMeta) {
  const row = props.form.models.find((item) => item.rowKey === modelsDevDialog.rowKey);
  modelsDevDialog.open = false;
  if (row) {
    applyModelsDevMeta(row, meta);
  }
}
</script>

<template>
  <DialogShell
    :open="open"
    dialog-class-name="providers-edit-dialog"
    eyebrow="聚合提供商"
    :title="dialogTitle"
    title-id="provider-edit-dialog-title"
    :close-disabled="loading"
    @close="$emit('close')"
  >
    <template #actions>
      <button class="primary-button" :disabled="loading" type="button" @click="confirm">
        {{ confirmLabel }}
      </button>
    </template>

    <p v-if="flowHint" class="providers-section-hint">{{ flowHint }}</p>

    <div v-if="showMeta" class="providers-form-grid">
      <label class="providers-field">
        <span>
          ID
          <span aria-hidden="true"> *</span>
        </span>
        <AppInput
          :model-value="form.providerId"
          :aria-describedby="form.errors?.providerId ? 'provider-id-error' : undefined"
          :disabled="loading || form.originalProviderId !== null"
          @update:model-value="form.providerId = normalizeProviderIdInput(String($event))"
        />
        <AppFieldError id="provider-id-error" :message="form.errors?.providerId ?? null" />
        <small class="providers-field__hint">
          唯一标识，保存后不可修改（纯本地标识，Key 存同一份 providers.yaml 的条目里）。只允许小写字母、数字和 `-`，不能以
          `reins-` 开头。
        </small>
      </label>

      <label class="providers-field">
        <span>
          名称
          <span aria-hidden="true"> *</span>
        </span>
        <AppInput
          v-model="form.label"
          :aria-describedby="form.errors?.label ? 'provider-label-error' : undefined"
          :disabled="loading"
        />
        <AppFieldError id="provider-label-error" :message="form.errors?.label ?? null" />
        <small class="providers-field__hint">提供商显示名，可随时修改。</small>
      </label>

      <div class="providers-field">
        <AppSelect
          v-model="form.protocol"
          variant="stacked"
          label="协议"
          :options="PROTOCOL_OPTIONS"
          :disabled="loading"
        />
        <small class="providers-field__hint">决定写入各工具时的接口形态。</small>
      </div>

      <label class="providers-field">
        <span>
          Base URL
          <span aria-hidden="true"> *</span>
        </span>
        <AppInput
          v-model="form.baseUrl"
          :aria-describedby="form.errors?.baseUrl ? 'provider-base-url-error' : undefined"
          :disabled="loading"
        />
        <AppFieldError id="provider-base-url-error" :message="form.errors?.baseUrl ?? null" />
        <small class="providers-field__hint">聚合提供商的 API 根地址，例如 https://openrouter.ai/api/v1。</small>
      </label>

      <label class="providers-field" style="grid-column: span 2">
        <span>API Key（可选）</span>
        <AppSecretInput v-model="form.apiKey" :disabled="loading" />
        <small class="providers-field__hint">{{ apiKeyHint }}</small>
      </label>
    </div>

    <div v-if="showModels">
      <div class="providers-section-header" style="margin-top: 18px">
        <h4 class="providers-section-title">模型目录</h4>
        <div class="providers-card__actions">
          <button class="secondary-button" :disabled="loading" type="button" @click="fetchDialogOpen = true">
            从提供商拉取
          </button>
          <button class="secondary-button" :disabled="loading" type="button" @click="modelDialogOpen = true">
            手工新增
          </button>
        </div>
      </div>
      <p class="providers-section-hint">
        元数据（上下文窗口、最大输出、图像、思考等级）会随应用写入目标工具；缺失时对应工具可能展示能力警告。
      </p>
      <AppFieldError id="provider-models-error" :message="form.errors?.models ?? null" />
    </div>

    <div v-if="showModels" class="providers-model-list">
      <div
        v-for="(row, index) in form.models"
        :key="row.rowKey"
        class="providers-model-row"
      >
        <div class="providers-model-row__head">
          <span class="providers-model-row__id">{{ row.id || "未命名模型" }}</span>
          <div class="providers-card__actions">
            <button
              class="secondary-button"
              :disabled="loading || !modelsDevProviderId || !row.id.trim()"
              type="button"
              :title="
                modelsDevProviderId && row.id.trim()
                  ? ''
                  : '请先填写提供商 ID 与模型 ID。'
              "
              @click="openModelsDevDialog(row)"
            >
              从 models.dev 补全
            </button>
            <button class="danger-button" :disabled="loading" type="button" @click="removeModel(index)">
              移除
            </button>
          </div>
        </div>

        <div class="providers-form-grid">
          <label class="providers-field">
            <span>模型 ID</span>
            <AppInput v-model="row.id" size="sm" :disabled="loading" />
          </label>
          <label class="providers-field">
            <span>显示名</span>
            <AppInput v-model="row.label" size="sm" :disabled="loading" />
          </label>
        </div>

        <div
          class="providers-model-row__grid"
          style="grid-template-columns: repeat(2, minmax(0, 1fr))"
        >
          <label class="providers-field">
            <span>上下文窗口</span>
            <AppInput v-model.number="row.contextWindow" size="sm" type="number" :disabled="loading" />
          </label>
          <label class="providers-field">
            <span>最大输出</span>
            <AppInput v-model.number="row.maxOutputTokens" size="sm" type="number" :disabled="loading" />
          </label>
        </div>

        <div
          class="providers-model-row__grid"
          style="grid-template-columns: auto auto minmax(0, 1fr)"
        >
          <div class="providers-field">
            <span>图像输入</span>
            <label class="providers-check">
              <input
                type="checkbox"
                :checked="row.supportsImages === true"
                :disabled="loading"
                @change="
                  row.supportsImages = ($event.target as HTMLInputElement).checked
                    ? true
                    : null
                "
              />
            </label>
          </div>
          <div class="providers-field">
            <span>推理能力</span>
            <label class="providers-check">
              <input
                type="checkbox"
                :checked="row.reasoning === true"
                :disabled="loading"
                @change="toggleReasoning(row, ($event.target as HTMLInputElement).checked)"
              />
            </label>
          </div>
          <div class="providers-field">
            <span>思考等级</span>
            <ReasoningLevelsSelect
              v-model="row.reasoningLevels"
              :disabled="loading || row.reasoning !== true"
            />
          </div>
        </div>

      </div>

      <p v-if="form.models.length === 0" class="providers-empty">
        还没有模型。可以从提供商拉取，或手工新增。
      </p>
    </div>

    <ProviderFetchModelsDialog
      v-if="showModels"
      :open="fetchDialogOpen"
      :provider-id="form.originalProviderId ?? ''"
      :key-present="keyPresent"
      :protocol="form.protocol"
      :base-url="form.baseUrl"
      :api-key="form.apiKey"
      :existing-ids="form.models.map((model) => model.id)"
      @close="fetchDialogOpen = false"
      @confirm="addFetchedModels"
    />

    <ProviderModelDialog
      v-if="showModels"
      :open="modelDialogOpen"
      :provider-id="form.originalProviderId ?? form.providerId"
      :existing-ids="form.models.map((model) => model.id)"
      @close="modelDialogOpen = false"
      @confirm="addModelRow"
    />

    <ModelsDevCompleteDialog
      :open="modelsDevDialog.open"
      :provider-id="modelsDevDialog.providerId"
      :model-id="modelsDevDialog.modelId"
      @close="modelsDevDialog.open = false"
      @apply="applyModelsDevCompletion"
    />

    <ConfirmDialog
      :open="skippedPromptIds.length > 0"
      title-id="provider-fetched-skipped-title"
      eyebrow="模型目录"
      title="部分模型未加入"
      :description="`以下模型已在模型目录中，已自动忽略：${skippedPromptIds.join('、')}。`"
      confirm-label="知道了"
      confirm-button-class-name="primary-button"
      @close="skippedPromptIds = []"
      @confirm="skippedPromptIds = []"
    />
  </DialogShell>
</template>
