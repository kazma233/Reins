<script setup lang="ts">
import { computed } from "vue";
import { BUILTIN_TARGET_PRESETS, type BuiltinTargetPresetId } from "../../model";
import DialogShell from "@shared/ui/DialogShell.vue";
import type { TargetFormState } from "../../model";

type TargetCreateDialogProps = {
  form: TargetFormState;
  open: boolean;
  loading: boolean;
};

const props = defineProps<TargetCreateDialogProps>();

defineEmits<{
  close: [];
  confirm: [];
  applyBuiltinPreset: [presetId: BuiltinTargetPresetId];
  pickMcpConfigFile: [];
  pickSkillDirectory: [];
}>();

const dialogTitle = computed(() => (props.form.originalTargetId ? "修改 target" : "新增 target"));
const confirmLabel = computed(() => (props.form.originalTargetId ? "保存" : "添加"));
const presetIds = computed(() => Object.keys(BUILTIN_TARGET_PRESETS) as BuiltinTargetPresetId[]);
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
            {{ presetId === 'grokbuild' ? 'Grok Build' : presetId }}
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
          <input v-model="form.targetId" type="text" />
        </label>

        <div class="manager-picker-row">
          <label class="manager-field manager-picker-row__field">
            <span>skills 目录</span>
            <small class="manager-field__hint">skill 会软链接到这个目录。</small>
            <input v-model="form.skillDir" type="text" />
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
              target 的 MCP 配置文件路径。不主动支持 MCP 的 target（如 pi）可留空，留空时不写入任何
              MCP 配置。
            </small>
            <input v-model="form.configPath" type="text" />
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
          <input v-model="form.mcpConfigPrefix" type="text" />
        </label>

        <div class="manager-stack">
          <span class="manager-field__label">configType</span>
          <small class="manager-field__hint">
            Common：command + args + env；OpenCode：command（数组，含参数）+ environment
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
          </div>
        </div>
      </div>
    </div>
  </DialogShell>
</template>
