<script setup lang="ts">
import { computed } from "vue";
import AppCheckbox from "@shared/ui/AppCheckbox.vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import type { McpFormState } from "../../model";

type McpCreateDialogProps = {
  form: McpFormState;
  open: boolean;
  loading: boolean;
};

const props = defineProps<McpCreateDialogProps>();

defineEmits<{
  close: [];
  confirm: [];
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
      <label class="manager-field">
        <span>
          名称
          <span class="manager-required-mark"> *</span>
        </span>
        <small class="manager-field__hint">唯一标识。会作为配置里的 mcp 名称。</small>
        <input v-model="form.name" type="text" />
      </label>

      <AppCheckbox v-model="form.enabled">启用</AppCheckbox>

      <label class="manager-field">
        <span>主页</span>
        <small class="manager-field__hint">选填。用于记录作者主页、项目地址或文档地址，不参与连接。</small>
        <input v-model="form.homepage" type="text" />
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
            <input v-model="form.command" type="text" />
          </label>
          <label class="manager-field">
            <span>Args</span>
            <small class="manager-field__hint">每行一个参数，按顺序写入数组，空行会忽略。</small>
            <textarea v-model="form.args" class="manager-editor manager-textarea" rows="4" />
          </label>
          <label class="manager-field">
            <span>Env</span>
            <small class="manager-field__hint">每行一条 `KEY=VALUE`。只有符合格式的内容会进入预览。</small>
            <textarea v-model="form.env" class="manager-editor manager-textarea" rows="5" />
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
            <input v-model="form.url" type="text" />
          </label>
          <label class="manager-field">
            <span>Headers</span>
            <small class="manager-field__hint">每行一条 `KEY=VALUE`，会写入远程请求头。</small>
            <textarea v-model="form.headers" class="manager-editor manager-textarea" rows="5" />
          </label>
        </div>
      </template>

      <label class="manager-field">
        <span>Timeout</span>
        <small class="manager-field__hint">选填，单位毫秒。留空则不写入。</small>
        <input v-model="form.timeout" type="text" />
      </label>
    </div>
  </DialogShell>
</template>
