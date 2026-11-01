<script setup lang="ts">
import { computed, ref, watch } from "vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import { extractErrorMessage } from "@shared/lib/errors";
import { fetchModelsdevCatalog } from "../../api";
import type { ModelsDevCandidate, ModelsDevMeta } from "../../generated";

// 从 models.dev 补全弹窗：打开即发起查询，弹窗自己承载加载/失败中间态
// （网络慢时按钮点下去不能没有反馈），命中的元数据回填由父层落到模型行。
type ModelsDevCompleteDialogProps = {
  open: boolean;
  providerId: string;
  modelId: string;
};

const props = defineProps<ModelsDevCompleteDialogProps>();

const emit = defineEmits<{
  close: [];
  apply: [meta: ModelsDevMeta];
}>();

type DialogState = "loading" | "candidates" | "none";

const state = ref<DialogState>("loading");
const errorText = ref("");
const candidates = ref<ModelsDevCandidate[]>([]);
const selected = ref(0);
// 关闭弹窗后到达的响应不再回填：请求编号在每次打开/关闭时自增。
let requestToken = 0;

watch(
  () => props.open,
  (open) => {
    requestToken += 1;
    if (open) {
      void runFetch();
    }
  }
);

const dialogTitle = computed(() => {
  if (state.value === "candidates") {
    return `「${props.modelId}」有多个匹配来源`;
  }
  return `「${props.modelId}」`;
});

async function runFetch() {
  const token = (requestToken += 1);
  state.value = "loading";
  errorText.value = "";
  candidates.value = [];
  selected.value = 0;
  try {
    const result = await fetchModelsdevCatalog(props.providerId, props.modelId);
    if (token !== requestToken) {
      return;
    }
    // 唯一匹配直接回填并关闭，调用方负责提示。
    if (result.status === "exact" && result.meta) {
      emit("apply", result.meta);
      return;
    }
    if (result.status === "candidates" && result.candidates.length > 0) {
      candidates.value = result.candidates;
      state.value = "candidates";
      return;
    }
    state.value = "none";
  } catch (error) {
    if (token !== requestToken) {
      return;
    }
    errorText.value = extractErrorMessage(error, "models.dev 查询失败。");
    state.value = "none";
  }
}

function confirm() {
  const candidate = candidates.value[selected.value];
  if (candidate) {
    emit("apply", candidate.meta);
  }
}
</script>

<template>
  <DialogShell
    :open="open"
    dialog-class-name="providers-modelsdev-dialog"
    eyebrow="从 models.dev 补全"
    :title="dialogTitle"
    title-id="modelsdev-complete-dialog-title"
    @close="$emit('close')"
  >
    <div v-if="state === 'loading'" class="providers-loading">获取数据中…</div>

    <template v-else-if="state === 'candidates'">
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
    </template>

    <template v-else>
      <p class="providers-empty">{{ errorText || "models.dev 未找到该模型。" }}</p>
      <button class="secondary-button" type="button" @click="runFetch">重试</button>
    </template>

    <template #actions>
      <button
        class="primary-button"
        :disabled="state !== 'candidates'"
        type="button"
        @click="confirm"
      >
        使用该来源补全
      </button>
    </template>
  </DialogShell>
</template>
