<script setup lang="ts">
import { computed } from "vue";
import AppFieldError from "@shared/ui/AppFieldError.vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import { AVAILABLE_PROJECT_AGENTS, formatTargetName, type ProjectFormState } from "../../model";

type ProjectCreateDialogProps = {
  form: ProjectFormState;
  open: boolean;
  loading: boolean;
  error: string | null;
};

const props = defineProps<ProjectCreateDialogProps>();

defineEmits<{
  close: [];
  confirm: [];
  clearFieldError: [field: string];
  pickProjectPath: [];
}>();

const mode = computed(() => (props.form.originalProjectId === null ? "create" : "edit"));
const title = computed(() =>
  mode.value === "create" ? "新增 project" : "修改 project",
);
const confirmLabel = computed(() =>
  props.loading ? "保存中..." : mode.value === "create" ? "新增" : "保存",
);

function isAgentActive(agentId: string): boolean {
  return props.form.agents.includes(agentId);
}

function toggleAgent(agentId: string) {
  if (props.form.agents.includes(agentId)) {
    props.form.agents = props.form.agents.filter((id) => id !== agentId);
  } else {
    props.form.agents = [...props.form.agents, agentId];
  }
}
</script>

<template>
  <DialogShell
    :open="open"
    :title="title"
    title-id="project-dialog-title"
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

    <div class="manager-stack">
      <AppFieldError :message="error" />

      <label class="manager-field">
        <span class="manager-field-label">project ID</span>
        <input
          v-model="form.projectId"
          :aria-describedby="form.errors.projectId ? 'project-id-error' : undefined"
          :aria-invalid="Boolean(form.errors.projectId)"
          class="manager-input"
          :disabled="mode === 'edit'"
          placeholder="my-app"
          type="text"
          @input="$emit('clearFieldError', 'projectId')"
        />
        <AppFieldError id="project-id-error" :message="form.errors.projectId ?? null" />
      </label>

      <div class="manager-field">
        <span class="manager-field-label">项目路径</span>
        <div class="manager-field-row">
          <input
            v-model="form.path"
            :aria-describedby="form.errors.path ? 'project-path-error' : undefined"
            :aria-invalid="Boolean(form.errors.path)"
            class="manager-input"
            placeholder="~/code/my-app"
            type="text"
            @input="$emit('clearFieldError', 'path')"
          />
          <button
            class="secondary-button"
            :disabled="loading"
            type="button"
            @click="$emit('pickProjectPath')"
          >
            选择
          </button>
        </div>
        <AppFieldError id="project-path-error" :message="form.errors.path ?? null" />
      </div>

      <div class="manager-field">
        <span class="manager-field-label">启用的 agents</span>
        <div class="manager-target-buttons">
          <button
            v-for="agentId in AVAILABLE_PROJECT_AGENTS"
            :key="agentId"
            :class="`secondary-button manager-target-button${isAgentActive(agentId) ? ' is-active' : ''}`"
            :disabled="loading"
            type="button"
            @click="toggleAgent(agentId)"
          >
            {{ formatTargetName(agentId) }}
          </button>
        </div>
      </div>
    </div>
  </DialogShell>
</template>
