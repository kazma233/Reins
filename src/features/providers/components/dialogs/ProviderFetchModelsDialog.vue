<script setup lang="ts">
import { computed, ref, watch } from "vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import { extractErrorMessage } from "@shared/lib/errors";
import { fetchProviderModels, fetchProviderModelsDirect } from "../../api";
import type { FetchedModel, ProviderProtocol } from "../../generated";

type ProviderFetchModelsDialogProps = {
  open: boolean;
  // 已保存提供商传其 ID；新建态传空串。是否按 ID 拉取还要看 keyPresent。
  providerId: string;
  // 该提供商当前是否已存 Key：有则按提供商 ID 拉取（读 providers.yaml），
  // 没有则走直连，用当次输入的 apiKey，不报「未设 Key」。
  keyPresent: boolean;
  protocol: ProviderProtocol;
  baseUrl: string;
  // 直连拉取使用的当次输入密钥。
  apiKey: string;
};

const props = defineProps<ProviderFetchModelsDialogProps>();

const emit = defineEmits<{
  close: [];
  confirm: [models: FetchedModel[]];
}>();

const fetching = ref(false);
const errorText = ref("");
const models = ref<FetchedModel[]>([]);
const fetchedUrl = ref("");
const selected = ref<Record<string, boolean>>({});

watch(
  () => props.open,
  (open) => {
    if (open) {
      models.value = [];
      fetchedUrl.value = "";
      selected.value = {};
      errorText.value = "";
      void runFetch();
    }
  }
);

async function runFetch() {
  fetching.value = true;
  errorText.value = "";
  try {
    // 已存 Key 时按已保存提供商拉取，与编辑提供商行为一致；
    // 没有时退回直连，用表单 Base URL + 当次输入的 Key。
    // 后端可能经历多级候选路径回退，url 是实际命中的那一个。
    const result =
      props.providerId && props.keyPresent
        ? await fetchProviderModels(props.providerId)
        : await fetchProviderModelsDirect(
            props.protocol,
            props.baseUrl.trim(),
            props.apiKey.trim()
          );
    models.value = result.models;
    fetchedUrl.value = result.url;
    selected.value = {};
  } catch (error) {
    errorText.value = extractErrorMessage(error, "拉取模型列表失败。");
  } finally {
    fetching.value = false;
  }
}

const selectedModels = computed(() => models.value.filter((model) => selected.value[model.id]));

function confirm() {
  // 按声明 emit FetchedModel[]（含 name），显示名的取舍交给父层。
  emit("confirm", selectedModels.value);
}
</script>

<template>
  <DialogShell
    :open="open"
    dialog-class-name="providers-fetch-dialog"
    eyebrow="从提供商拉取"
    title="选择要加入的模型"
    title-id="provider-fetch-models-dialog-title"
    @close="$emit('close')"
  >
    <template #actions>
      <button
        class="primary-button"
        :disabled="fetching || selectedModels.length === 0"
        type="button"
        @click="confirm"
      >
        加入已选（{{ selectedModels.length }}）
      </button>
    </template>

    <div v-if="fetching" class="providers-loading">拉取中…</div>

    <template v-else-if="errorText">
      <p class="providers-empty">{{ errorText }}</p>
      <button class="secondary-button" type="button" @click="runFetch">重试</button>
    </template>

    <template v-else-if="models.length === 0">
      <p class="providers-empty">提供商返回了空的模型列表。</p>
      <button class="secondary-button" type="button" @click="runFetch">重试</button>
    </template>

    <div v-else class="providers-fetched-list">
      <p class="providers-section-hint">拉取地址：{{ fetchedUrl }}</p>
      <label
        v-for="model in models"
        :key="model.id"
        class="providers-fetched-item"
      >
        <input v-model="selected[model.id]" type="checkbox" />
        <span>{{ model.id }}<template v-if="model.name"> · {{ model.name }}</template></span>
      </label>
    </div>
  </DialogShell>
</template>
