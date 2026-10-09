<script setup lang="ts">
import { computed } from "vue";
import { type TargetFormState } from "../../model";
import type { McpFormatExample, TargetPreset } from "../../types";
import AppFieldError from "@shared/ui/AppFieldError.vue";
import AppLoadError from "@shared/ui/AppLoadError.vue";
import DialogShell from "@shared/ui/DialogShell.vue";

type TargetCreateDialogProps = {
  form: TargetFormState;
  open: boolean;
  loading: boolean;
  error: string | null;
  // 创建模式：后端下发的七个内置工具预设。
  presets: TargetPreset[];
  presetsLoading: boolean;
  presetsError: string | null;
  // 编辑模式：后端下发的该 target 的 MCP 配置格式说明。
  mcpFormatDescription: string | null;
  // 编辑模式：后端下发的写入形态示例（按形态各一段）。
  mcpFormatExamples: McpFormatExample[];
};

const props = defineProps<TargetCreateDialogProps>();

defineEmits<{
  close: [];
  confirm: [];
  clearFieldError: [field: string];
  // presetId 来自下发的预设 targetId（封闭集合），用 string 传递。
  applyBuiltinPreset: [presetId: string];
  retryLoadPresets: [];
  pickMcpConfigFile: [];
  pickSkillDirectory: [];
}>();

const dialogTitle = computed(() => (props.form.originalTargetId ? "修改 target" : "新增 target"));
const confirmLabel = computed(() => (props.form.originalTargetId ? "保存" : "添加"));
// 创建模式只能从七个内置预设里选，编辑模式（含存量自定义 target）不走预设。
const isCreate = computed(() => props.form.originalTargetId === null);
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

      <div v-if="isCreate" class="manager-stack">
        <span class="manager-field__label">内置工具</span>
        <div v-if="presetsLoading" class="loading-pill">正在读取内置工具预设...</div>
        <AppLoadError
          v-else-if="presetsError"
          :message="presetsError"
          :retrying="presetsLoading"
          @retry="$emit('retryLoadPresets')"
        />
        <div v-else class="manager-card-actions manager-card-actions--start">
          <button
            v-for="preset in presets"
            :key="preset.targetId"
            class="secondary-button"
            :disabled="loading"
            type="button"
            @click="$emit('applyBuiltinPreset', preset.targetId)"
          >
            {{ preset.label }}
          </button>
        </div>
      </div>

      <div class="manager-stack manager-target-form-fields">
        <label class="manager-field">
          <span>
            ID
            <span class="manager-required-mark"> *</span>
            <AppFieldError
              class="manager-field__error"
              id="target-id-error"
              :message="form.errors.targetId ?? null"
            />
          </span>
          <small v-if="isCreate" class="manager-field__hint">从上方内置工具选择后自动填入，不可修改。</small>
          <small v-else class="manager-field__hint">唯一标识。只允许小写字母、数字和 `-`。</small>
          <input
            v-model="form.targetId"
            :readonly="isCreate"
            :aria-describedby="form.errors.targetId ? 'target-id-error' : undefined"
            :aria-invalid="Boolean(form.errors.targetId)"
            type="text"
            @input="$emit('clearFieldError', 'targetId')"
          />
        </label>

        <div class="manager-picker-row">
          <label class="manager-field manager-picker-row__field">
            <span>
              skills 目录
              <AppFieldError
                class="manager-field__error"
                id="target-skill-dir-error"
                :message="form.errors.skillDir ?? null"
              />
            </span>
            <small class="manager-field__hint">skill 会软链接到这个目录。</small>
            <input
              v-model="form.skillDir"
              :aria-describedby="form.errors.skillDir ? 'target-skill-dir-error' : undefined"
              :aria-invalid="Boolean(form.errors.skillDir)"
              type="text"
              @input="$emit('clearFieldError', 'skillDir')"
            />
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
            <span>
              MCP 配置文件（可选）
              <AppFieldError
                class="manager-field__error"
                id="target-config-path-error"
                :message="form.errors.configPath ?? null"
              />
            </span>
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

        <div class="manager-field">
          <span>configPrefix</span>
          <small class="manager-field__hint">
            该工具写入 MCP 配置文件的节点路径,由各工具定义好的固定值,展示只读。
          </small>
          <input
            v-model="form.mcpConfigPrefix"
            readonly
            tabindex="-1"
            type="text"
            class="is-readonly"
          />
        </div>

        <div v-if="mcpFormatDescription && !isCreate" class="manager-stack">
          <span class="manager-field__label">MCP 配置格式</span>
          <small class="manager-field__hint">{{ mcpFormatDescription }}</small>
          <div
            v-for="example in mcpFormatExamples"
            :key="example.label"
            class="manager-stack"
          >
            <small class="manager-field__hint">{{ example.label }}</small>
            <pre class="manager-pre">{{ example.body }}</pre>
          </div>
        </div>
      </div>
    </div>
  </DialogShell>
</template>
