<script setup lang="ts">
import { computed } from "vue";
import AppCheckbox from "@shared/ui/AppCheckbox.vue";
import AppFieldError from "@shared/ui/AppFieldError.vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import type { McpFormState } from "../../model";

type McpCreateDialogProps = {
  form: McpFormState;
  open: boolean;
  loading: boolean;
  error: string | null;
};

const props = defineProps<McpCreateDialogProps>();

const emit = defineEmits<{
  close: [];
  confirm: [];
  clearFieldError: [field: string];
}>();

const dialogTitle = computed(() => (props.form.originalName ? "修改 mcp" : "新增 mcp"));
const confirmLabel = computed(() => (props.form.originalName ? "保存" : "添加"));
</script>

<template>
  <DialogShell
    :open="open"
    dialog-class-name="manager-import-dialog"
    eyebrow="MCP"
    :title="dialogTitle"
    title-id="mcp-create-dialog-title"
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
      <AppFieldError :message="form.errors.form ?? null" />

      <label class="manager-field">
        <span>
          名称
          <span class="manager-required-mark"> *</span>
        </span>
        <small class="manager-field__hint">唯一标识。会作为配置里的 mcp 名称。</small>
        <input
          v-model="form.name"
          :aria-describedby="form.errors.name ? 'mcp-name-error' : undefined"
          :aria-invalid="Boolean(form.errors.name)"
          type="text"
          @input="$emit('clearFieldError', 'name')"
        />
        <AppFieldError id="mcp-name-error" :message="form.errors.name ?? null" />
      </label>

      <AppCheckbox v-model="form.enabled">启用</AppCheckbox>

      <label class="manager-field">
        <span>主页</span>
        <small class="manager-field__hint">选填。用于记录作者主页、项目地址或文档地址，不参与连接。</small>
        <input
          v-model="form.homepage"
          type="text"
          @input="$emit('clearFieldError', 'homepage')"
        />
      </label>

      <div class="manager-stack">
        <span class="manager-field__label">传输方式</span>
        <p class="manager-field__hint">`stdio` 使用本地命令启动，`http` 和 `sse` 使用远程连接。</p>
        <div class="manager-segmented manager-segmented--triple">
          <button
            :class="`manager-segmented__button${form.transport === 'stdio' ? ' is-active' : ''}`"
            type="button"
            @click="form.transport = 'stdio'"
          >
            stdio
          </button>
          <button
            :class="`manager-segmented__button${form.transport === 'http' ? ' is-active' : ''}`"
            type="button"
            @click="form.transport = 'http'"
          >
            http
          </button>
          <button
            :class="`manager-segmented__button${form.transport === 'sse' ? ' is-active' : ''}`"
            type="button"
            @click="form.transport = 'sse'"
          >
            sse
          </button>
        </div>
      </div>

      <template v-if="form.transport === 'stdio'">
        <div class="manager-import-grid">
          <label class="manager-field">
            <span>
              Command
              <span class="manager-required-mark"> *</span>
            </span>
            <small class="manager-field__hint">本地启动命令，比如 `npx`、`uvx`、`docker`。</small>
            <input
              v-model="form.command"
              :aria-describedby="form.errors.command ? 'mcp-command-error' : undefined"
              :aria-invalid="Boolean(form.errors.command)"
              type="text"
              @input="$emit('clearFieldError', 'command')"
            />
            <AppFieldError id="mcp-command-error" :message="form.errors.command ?? null" />
          </label>
          <label class="manager-field">
            <span>Args</span>
            <small class="manager-field__hint">每行一个参数，按顺序写入数组，空行会忽略。</small>
            <textarea v-model="form.args" class="manager-editor manager-textarea" rows="4" />
          </label>
          <label class="manager-field">
            <span>Env</span>
            <small class="manager-field__hint">每行一条 `KEY=VALUE`。只有符合格式的内容会进入预览。</small>
            <textarea
              v-model="form.env"
              :aria-describedby="form.errors.env ? 'mcp-env-error' : undefined"
              :aria-invalid="Boolean(form.errors.env)"
              class="manager-editor manager-textarea"
              rows="5"
              @input="$emit('clearFieldError', 'env')"
            />
            <AppFieldError id="mcp-env-error" :message="form.errors.env ?? null" />
          </label>
        </div>
      </template>

      <template v-else>
        <div class="manager-import-grid">
          <label class="manager-field">
            <span>
              连接地址
              <span class="manager-required-mark"> *</span>
            </span>
            <small class="manager-field__hint">远程 mcp 服务地址，`http` 和 `sse` 都填这里。</small>
            <input
              v-model="form.url"
              :aria-describedby="form.errors.url ? 'mcp-url-error' : undefined"
              :aria-invalid="Boolean(form.errors.url)"
              type="text"
              @input="$emit('clearFieldError', 'url')"
            />
            <AppFieldError id="mcp-url-error" :message="form.errors.url ?? null" />
          </label>
          <label class="manager-field">
            <span>Headers</span>
            <small class="manager-field__hint">每行一条 `KEY=VALUE`，会写入远程请求头。</small>
            <textarea
              v-model="form.headers"
              :aria-describedby="form.errors.headers ? 'mcp-headers-error' : undefined"
              :aria-invalid="Boolean(form.errors.headers)"
              class="manager-editor manager-textarea"
              rows="5"
              @input="$emit('clearFieldError', 'headers')"
            />
            <AppFieldError id="mcp-headers-error" :message="form.errors.headers ?? null" />
          </label>
        </div>
      </template>

      <label class="manager-field">
        <span>Timeout</span>
        <small class="manager-field__hint">选填，单位毫秒。留空则不写入。</small>
        <input
          v-model="form.timeout"
          :aria-describedby="form.errors.timeout ? 'mcp-timeout-error' : undefined"
          :aria-invalid="Boolean(form.errors.timeout)"
          type="text"
          @input="$emit('clearFieldError', 'timeout')"
        />
        <AppFieldError id="mcp-timeout-error" :message="form.errors.timeout ?? null" />
      </label>
    </div>
  </DialogShell>
</template>
