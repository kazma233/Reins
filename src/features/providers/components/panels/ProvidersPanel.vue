<script setup lang="ts">
import { reactive, ref } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import AppCard from "@shared/ui/AppCard.vue";
import { deleteProvider, setProviderKey, upsertProvider } from "../../api";
import {
  formFromProvider,
  formToInput,
  protocolLabel,
  emptyProviderForm,
  type ProviderFormState,
} from "../../model";
import { useProvidersAction } from "../../composables/useProvidersAction";
import { useProvidersState } from "../../composables/useProvidersState";
import ProviderEditDialog from "../dialogs/ProviderEditDialog.vue";

const { state, loadingState, runningAction } = useProvidersState();
const { runProvidersAction } = useProvidersAction();

const editOpen = ref(false);
const editKeyPresent = ref(false);
// 新增流程分两步：step 1 服务商元数据，保存后进入 step 2 模型目录。
const editCreating = ref(false);
const editStep = ref<1 | 2>(1);
const form = reactive<ProviderFormState>(emptyProviderForm());
const deleteDialog = reactive({ open: false, providerId: "", providerLabel: "" });

function openCreate() {
  Object.assign(form, emptyProviderForm());
  editCreating.value = true;
  editStep.value = 1;
  editOpen.value = true;
}

function openEdit(providerId: string) {
  const provider = state.value?.providers.find((item) => item.id === providerId);
  if (!provider) {
    return;
  }
  Object.assign(form, formFromProvider(provider));
  editKeyPresent.value = provider.keyPresent;
  editCreating.value = false;
  editStep.value = 2;
  editOpen.value = true;
}

function confirmEdit(apiKey: string) {
  const input = formToInput(form);
  runProvidersAction({
    // 先持久化提供商，再写密钥；upsert 幂等，密钥写入失败后重试安全。
    // upsert 返回归一化后的落库 ID，后续动作一律用它。
    action: async () => {
      const providerId = await upsertProvider(input);
      if (apiKey) {
        await setProviderKey(providerId, apiKey);
      }
      return providerId;
    },
    success: (providerId) => ({
      message:
        editCreating.value && editStep.value === 1
          ? `提供商 ${providerId} 已保存，请继续设置模型目录。`
          : apiKey
            ? `提供商 ${providerId} 已保存，API Key 已存入系统密钥管理。`
            : `提供商 ${providerId} 已保存。`,
    }),
    after: (providerId) => {
      if (editCreating.value && editStep.value === 1) {
        // 新增第一步：不关弹窗，原地进入模型目录设置；
        // 填上 originalProviderId 后，拉取即按已保存提供商走。
        form.originalProviderId = providerId;
        editKeyPresent.value = Boolean(apiKey);
        editStep.value = 2;
        return;
      }
      editOpen.value = false;
    },
  });
}

function requestDeleteProvider(providerId: string) {
  const provider = state.value?.providers.find((item) => item.id === providerId);
  deleteDialog.providerId = providerId;
  deleteDialog.providerLabel = provider?.label ?? providerId;
  deleteDialog.open = true;
}

function confirmDeleteProvider() {
  const providerId = deleteDialog.providerId;
  runProvidersAction({
    action: () => deleteProvider(providerId),
    success: (result) => ({ message: result.detail }),
    after: () => {
      deleteDialog.open = false;
    },
  });
}
</script>

<template>
  <section class="providers-panel">
    <div class="providers-section-header">
      <div>
        <h3 class="providers-section-title">聚合提供商</h3>
        <p class="providers-section-hint">
          提供商元数据保存在 {{ state?.configPath ?? "providers.yaml" }}；API Key 在编辑弹窗中设置，存系统密钥管理。官方
          OpenAI / Anthropic 不在此纳管。
        </p>
      </div>
      <div class="providers-card__actions">
        <button class="primary-button" type="button" @click="openCreate">新增提供商</button>
      </div>
    </div>

    <div v-if="loadingState" class="providers-loading">读取中…</div>

    <div v-else-if="state && state.providers.length > 0" class="providers-card-grid">
      <AppCard v-for="provider in state.providers" :key="provider.id">
        <template #header>
          <h4 class="providers-card__title">{{ provider.label }}</h4>
        </template>
        <template #ext>
          <span class="providers-card__models">{{ provider.models.length }} 个模型</span>
          <span class="providers-badge" :class="provider.keyPresent ? 'providers-badge--applied' : 'providers-badge--muted'">
            {{ provider.keyPresent ? "已设 Key" : "未设 Key" }}
          </span>
        </template>

        <dl class="providers-card__meta">
          <div>
            <dt>ID</dt>
            <dd>{{ provider.id }}</dd>
          </div>
          <div>
            <dt>协议</dt>
            <dd>{{ protocolLabel(provider.protocol) }}</dd>
          </div>
          <div>
            <dt>Base URL</dt>
            <dd class="providers-card__url">{{ provider.baseUrl }}</dd>
          </div>
        </dl>

        <template #actions>
          <button class="secondary-button" :disabled="runningAction" type="button" @click="openEdit(provider.id)">
            编辑
          </button>
          <button class="danger-button" :disabled="runningAction" type="button" @click="requestDeleteProvider(provider.id)">
            删除提供商
          </button>
        </template>
      </AppCard>
    </div>

    <div v-else class="providers-empty">
      还没有聚合提供商。点击「新增提供商」添加 OpenRouter、自建网关等。
    </div>

    <ProviderEditDialog
      :open="editOpen"
      :loading="runningAction"
      :form="form"
      :key-present="editKeyPresent"
      :creating="editCreating"
      :step="editStep"
      @close="editOpen = false"
      @confirm="confirmEdit"
    />

    <ConfirmDialog
      :open="deleteDialog.open"
      title-id="provider-delete-title"
      title="删除提供商"
      :description="`将删除提供商 ${deleteDialog.providerLabel} 的元数据与系统密钥条目。已写入工具的配置不会被自动清理；如 Claude Code 仍在引用该提供商，删除会被拒绝，请先在「Agent」页移除。`"
      confirm-label="删除"
      :loading="runningAction"
      @close="deleteDialog.open = false"
      @confirm="confirmDeleteProvider"
    />
  </section>
</template>
