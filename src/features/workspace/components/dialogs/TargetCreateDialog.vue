<script setup lang="ts">
import { computed } from "vue";
import { BUILTIN_TARGET_PRESETS, formatTargetName, type BuiltinTargetPresetId } from "../../model";
import AppFieldError from "@shared/ui/AppFieldError.vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import type { TargetFormState } from "../../model";

type TargetCreateDialogProps = {
  form: TargetFormState;
  open: boolean;
  loading: boolean;
  error: string | null;
};

const props = defineProps<TargetCreateDialogProps>();

defineEmits<{
  close: [];
  confirm: [];
  clearFieldError: [field: string];
  applyBuiltinPreset: [presetId: BuiltinTargetPresetId];
  pickMcpConfigFile: [];
  pickSkillDirectory: [];
}>();

const dialogTitle = computed(() => (props.form.originalTargetId ? "修改 target" : "新增 target"));
const confirmLabel = computed(() => (props.form.originalTargetId ? "保存" : "添加"));
const presetIds = computed(() => Object.keys(BUILTIN_TARGET_PRESETS) as BuiltinTargetPresetId[]);
// 内置预设按展示名列出，不再直接显示配置里的 id。
const presetLabels = Object.fromEntries(
  presetIds.value.map((presetId) => [presetId, formatTargetName(presetId)]),
) as Record<BuiltinTargetPresetId, string>;
</script>

<template>
  <DialogShell
    :open="open"
    dialog-class-name="manager-import-dialog"
    eyebrow="Target"
    :title="dialogTitle"
    title-id="target-create-dialog-title"
    :close-disabled="loading"
    @close="$emit('close')"
  >
    <template #actions>
      <button
        class="primary-button"
        :disabled="loading"
        type="button"
        @click="$emit('confirm')"
      >
        {{ confirmLabel }}
      </button>
    </template>

    <div class="manager-target-form-shell">
      <AppFieldError :message="error" />

      <div class="manager-stack">
        <span class="manager-field__label">内置默认值</span>
        <div class="manager-card-actions manager-card-actions--start">
          <button
            v-for="presetId in presetIds"
            :key="presetId"
            class="secondary-button"
            :disabled="loading"
            type="button"
            @click="$emit('applyBuiltinPreset', presetId)"
          >
            {{ presetLabels[presetId] }}
          </button>
        </div>
      </div>

      <div class="manager-stack manager-target-form-fields">
        <label class="manager-field">
          <span>
            ID
            <span class="manager-required-mark"> *</span>
          </span>
          <small class="manager-field__hint">唯一标识。只允许小写字母、数字和 `-`。</small>
          <input
            v-model="form.targetId"
            :aria-describedby="form.errors.targetId ? 'target-id-error' : undefined"
            :aria-invalid="Boolean(form.errors.targetId)"
            type="text"
            @input="$emit('clearFieldError', 'targetId')"
          />
          <AppFieldError id="target-id-error" :message="form.errors.targetId ?? null" />
        </label>

        <div class="manager-picker-row">
          <label class="manager-field manager-picker-row__field">
            <span>skills 目录</span>
            <small class="manager-field__hint">skill 会软链接到这个目录。</small>
            <input
              v-model="form.skillDir"
              :aria-describedby="form.errors.skillDir ? 'target-skill-dir-error' : undefined"
              :aria-invalid="Boolean(form.errors.skillDir)"
              type="text"
              @input="$emit('clearFieldError', 'skillDir')"
            />
            <AppFieldError id="target-skill-dir-error" :message="form.errors.skillDir ?? null" />
          </label>
          <div class="manager-actions manager-picker-row__actions">
            <button
              class="secondary-button"
              :disabled="loading"
              type="button"
              @click="$emit('pickSkillDirectory')"
            >
              选择目录
            </button>
          </div>
        </div>

        <div class="manager-picker-row">
          <label class="manager-field manager-picker-row__field">
            <span>MCP 配置文件（可选）</span>
            <small class="manager-field__hint">
              target 的 MCP 配置文件路径。不需要 MCP 分发的 target 可留空，留空时不写入任何 MCP
              配置。
            </small>
            <input
              v-model="form.configPath"
              :aria-describedby="form.errors.configPath ? 'target-config-path-error' : undefined"
              :aria-invalid="Boolean(form.errors.configPath)"
              type="text"
              @input="$emit('clearFieldError', 'configPath')"
            />
            <AppFieldError id="target-config-path-error" :message="form.errors.configPath ?? null" />
          </label>
          <div class="manager-actions manager-picker-row__actions">
            <button
              class="secondary-button"
              :disabled="loading"
              type="button"
              @click="$emit('pickMcpConfigFile')"
            >
              选择配置
            </button>
          </div>
        </div>

        <label class="manager-field">
          <span>configPrefix</span>
          <small class="manager-field__hint">
            写入 MCP 节点的路径，比如 `mcpServers` 或 `mcp`。与 MCP 配置文件路径成对填写。
          </small>
          <input
            v-model="form.mcpConfigPrefix"
            :aria-describedby="form.errors.mcpConfigPrefix ? 'target-config-prefix-error' : undefined"
            :aria-invalid="Boolean(form.errors.mcpConfigPrefix)"
            type="text"
            @input="$emit('clearFieldError', 'mcpConfigPrefix')"
          />
          <AppFieldError id="target-config-prefix-error" :message="form.errors.mcpConfigPrefix ?? null" />
        </label>

        <div class="manager-stack">
          <span class="manager-field__label">configType</span>
          <small class="manager-field__hint">
            Common：command + args + env；OpenCode：command（数组，含参数）+
            environment；DeepSeek Harness：Cordis patch YAML 条目（无 configPrefix）
          </small>
          <div class="manager-segmented">
            <button
              :class="`manager-segmented__button${form.mcpConfigType === 'common' ? ' is-active' : ''}`"
              type="button"
              @click="form.mcpConfigType = 'common'"
            >
              Common
            </button>
            <button
              :class="`manager-segmented__button${form.mcpConfigType === 'opencode' ? ' is-active' : ''}`"
              type="button"
              @click="form.mcpConfigType = 'opencode'"
            >
              OpenCode
            </button>
            <button
              :class="`manager-segmented__button${form.mcpConfigType === 'grokbuild' ? ' is-active' : ''}`"
              type="button"
              @click="form.mcpConfigType = 'grokbuild'"
            >
              Grok Build
            </button>
            <button
              :class="`manager-segmented__button${form.mcpConfigType === 'dsh' ? ' is-active' : ''}`"
              type="button"
              @click="form.mcpConfigType = 'dsh'"
            >
              DeepSeek Harness
            </button>
          </div>
        </div>
      </div>
    </div>
  </DialogShell>
</template>
