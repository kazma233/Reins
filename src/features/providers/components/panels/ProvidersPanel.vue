<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import AppCard from "@shared/ui/AppCard.vue";
import AppFieldError from "@shared/ui/AppFieldError.vue";
import AppLoadError from "@shared/ui/AppLoadError.vue";
import AppResultBadge from "@shared/ui/AppResultBadge.vue";
import { extractErrorMessage } from "@shared/lib/errors";
import { applyProviderToApp, deleteProvider, upsertProvider } from "../../api";
import {
  APP_LABELS,
  formFromProvider,
  formToInput,
  hasApiKey,
  protocolLabel,
  providerSyncPlan,
  emptyProviderForm,
  validateProviderForm,
  type ProviderFormState,
  type ProviderSyncPlanItem,
  type ProviderSyncSkip,
  type ProviderSyncTarget,
} from "../../model";
import { useProvidersAction } from "../../composables/useProvidersAction";
import { useProvidersState } from "../../composables/useProvidersState";
import ProviderEditDialog from "../dialogs/ProviderEditDialog.vue";

const { state, error, loadingState, runningAction, retryProvidersState } = useProvidersState();
const { runProvidersAction } = useProvidersAction();

const editOpen = ref(false);
const editKeyPresent = ref(false);
// 新增流程分两步：step 1 服务商元数据，保存后进入 step 2 模型目录。
const editCreating = ref(false);
const editStep = ref<1 | 2>(1);
const form = reactive<ProviderFormState>(emptyProviderForm());
const deleteDialog = reactive({ open: false, providerId: "", providerLabel: "" });
const deleteError = ref<string | null>(null);
// 同步结果常驻在卡片内直到下次同步：异步任务结束后没有其他反馈位置。
const syncResults = reactive<
  Record<string, { message: string; tone: "body" | "danger" }>
>({});

// 已应用 + 配置有偏差的条目都算同步目标，计划按提供商缓存一份供按钮与同步执行共用。
const syncPlans = computed(() => {
  const plans = new Map<string, ProviderSyncPlanItem[]>();
  const current = state.value;
  if (current) {
    for (const provider of current.providers) {
      plans.set(provider.id, providerSyncPlan(current, provider));
    }
  }
  return plans;
});

function syncPlanFor(providerId: string): ProviderSyncPlanItem[] {
  return syncPlans.value.get(providerId) ?? [];
}

function syncTargetsFor(providerId: string): ProviderSyncTarget[] {
  return syncPlanFor(providerId).filter((item): item is ProviderSyncTarget => !item.skip);
}

function syncSkipsFor(providerId: string): ProviderSyncSkip[] {
  return syncPlanFor(providerId).filter((item): item is ProviderSyncSkip => item.skip);
}

// 阻断原因是完整句子，嵌入提示时去掉句尾句号再统一收尾。
function reasonText(reason: string): string {
  return reason.replace(/。+$/, "");
}

function syncButtonTitle(providerId: string): string {
  const plan = syncPlanFor(providerId);
  if (plan.length === 0) {
    return "尚未应用到任何 Agent。";
  }
  if (syncTargetsFor(providerId).length === 0) {
    const reasons = syncSkipsFor(providerId)
      .map((item) => `${APP_LABELS[item.app]} ${reasonText(item.reason)}`)
      .join("；");
    return `无法同步：${reasons}。`;
  }
  return "";
}

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
  editKeyPresent.value = hasApiKey(provider);
  editCreating.value = false;
  editStep.value = 2;
  editOpen.value = true;
}

function confirmEdit() {
  // 新增第一步只落库元数据，模型目录在第二步才填，不能在这一步拦住用户。
  const isCreateStepOne = editCreating.value && editStep.value === 1;
  form.errors = validateProviderForm(form, {
    allowEmptyModels: isCreateStepOne,
    existingProviderIds: (state.value?.providers ?? []).map((provider) => provider.id),
  });
  if (Object.keys(form.errors).length > 0) {
    return;
  }

  // 新增与编辑走同一条写入命令，意图显式下发：新增撞名会被后端拒绝。
  const input = formToInput(form, form.originalProviderId === null ? "create" : "update");
  runProvidersAction({
    // 元数据与明文 API Key 在同一份 providers.yaml 里，一次 upsert 原子落盘。
    action: () => upsertProvider(input),
    reload: true,
    onSuccess: (providerId) => {
      if (isCreateStepOne) {
        // 新增第一步：不关弹窗，原地进入模型目录设置；
        // 填上 originalProviderId 后，拉取即按已保存提供商走。
        form.originalProviderId = providerId;
        editKeyPresent.value = Boolean(form.apiKey.trim());
        editStep.value = 2;
        return;
      }
      editOpen.value = false;
    },
    onError: (message) => {
      // 后端拒绝时留在弹窗里，不再静默失败
      form.errors = { form: message };
    },
  });
}

function requestDeleteProvider(providerId: string) {
  const provider = state.value?.providers.find((item) => item.id === providerId);
  deleteDialog.providerId = providerId;
  deleteDialog.providerLabel = provider?.label ?? providerId;
  deleteError.value = null;
  deleteDialog.open = true;
}

function closeDeleteDialog() {
  deleteDialog.open = false;
  deleteError.value = null;
}

// 同步是显式操作，点按即执行：逐个 Agent 应用，跳过与失败都汇总成一句
// 常驻结果写回卡片；执行前先清掉该提供商的旧结果。
function runSync(providerId: string) {
  const targets = syncTargetsFor(providerId);
  const skips = syncSkipsFor(providerId);
  delete syncResults[providerId];
  runProvidersAction({
    action: async () => {
      const synced: string[] = [];
      const failures: string[] = [];
      for (const item of targets) {
        try {
          await applyProviderToApp(item.input);
          synced.push(APP_LABELS[item.app]);
        } catch (error) {
          failures.push(`${APP_LABELS[item.app]}：${extractErrorMessage(error, "同步失败。")}`);
        }
      }
      return { synced, failures };
    },
    reload: true,
    onSuccess: ({ synced, failures }) => {
      syncResults[providerId] = {
        message: syncResultText(synced, failures, skips),
        tone: failures.length > 0 ? "danger" : "body",
      };
    },
    onError: (message) => {
      syncResults[providerId] = { message, tone: "danger" };
    },
  });
}

// 同步结果句：已同步 / 跳过原因 / 逐项失败明细合成一句，常驻在卡片上。
function syncResultText(synced: string[], failures: string[], skips: ProviderSyncSkip[]): string {
  const parts = [synced.length > 0 ? `已同步到 ${synced.join("、")}` : "没有可同步的 Agent"];
  if (skips.length > 0) {
    const skipped = skips
      .map((item) => `${APP_LABELS[item.app]}（${reasonText(item.reason)}）`)
      .join("、");
    parts.push(`未同步：${skipped}`);
  }
  if (failures.length > 0) {
    parts.push(`失败：${failures.join("；")}`);
  }
  return `${parts.join("；")}。`;
}

function confirmDeleteProvider() {
  const providerId = deleteDialog.providerId;
  deleteError.value = null;
  runProvidersAction({
    action: () => deleteProvider(providerId),
    reload: true,
    onSuccess: () => {
      deleteDialog.open = false;
    },
    onError: (message) => {
      deleteError.value = message;
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
          提供商元数据与 API Key（明文）保存在 {{ state?.configPath ?? "providers.yaml" }}。官方
          OpenAI / Anthropic 不在此纳管。
        </p>
      </div>
      <div class="providers-card__actions">
        <button class="primary-button" type="button" @click="openCreate">新增提供商</button>
      </div>
    </div>

    <AppLoadError
      v-if="error"
      :message="error"
      :retrying="loadingState"
      @retry="retryProvidersState"
    />

    <div v-if="loadingState" class="providers-loading">读取中…</div>

    <div v-else-if="state && state.providers.length > 0" class="providers-card-grid">
      <AppCard v-for="provider in state.providers" :key="provider.id">
        <template #header>
          <h4 class="providers-card__title">{{ provider.label }}</h4>
        </template>
        <template #ext>
          <span class="providers-card__models">{{ provider.models.length }} 个模型</span>
          <span class="providers-badge" :class="hasApiKey(provider) ? 'providers-badge--applied' : 'providers-badge--muted'">
            {{ hasApiKey(provider) ? "已设 Key" : "未设 Key" }}
          </span>
        </template>

        <dl class="providers-card__meta">
          <div>
            <dt>ID：</dt>
            <dd>{{ provider.id }}</dd>
          </div>
          <div>
            <dt>协议：</dt>
            <dd>{{ protocolLabel(provider.protocol) }}</dd>
          </div>
          <div class="providers-card__meta--wide">
            <dt>Base URL：</dt>
            <dd class="providers-card__url">{{ provider.baseUrl }}</dd>
          </div>
        </dl>

        <template #actions>
          <!-- 同步结果只占一个图标：文案在 hover 气泡里，操作行宽度不受文案长度影响 -->
          <AppResultBadge
            v-if="syncResults[provider.id]"
            :message="syncResults[provider.id].message"
            :tone="syncResults[provider.id].tone === 'danger' ? 'danger' : 'success'"
          />
          <button
            class="secondary-button"
            :disabled="runningAction || syncTargetsFor(provider.id).length === 0"
            type="button"
            :title="syncButtonTitle(provider.id)"
            @click="runSync(provider.id)"
          >
            同步
          </button>
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
      :description="`将删除提供商 ${deleteDialog.providerLabel} 的元数据与其中的明文 API Key。已写入工具的配置不会被自动清理；如 Claude Code 仍在引用该提供商，删除会被拒绝，请先在「Agent」页移除。`"
      confirm-label="删除"
      :loading="runningAction"
      @close="closeDeleteDialog"
      @confirm="confirmDeleteProvider"
    >
      <AppFieldError :message="deleteError" />
    </ConfirmDialog>
  </section>
</template>
