<script setup lang="ts">
import { computed } from "vue";
import DialogShell from "./DialogShell.vue";
import { joinClasses } from "../lib/join-classes";
import "./confirm-dialog.css";

type ConfirmDialogProps = {
  open: boolean;
  titleId: string;
  title: string;
  eyebrow?: string;
  description?: string;
  confirmLabel?: string;
  confirmButtonClassName?: string;
  cancelLabel?: string;
  dialogClassName?: string;
  loading?: boolean;
};

const props = withDefaults(defineProps<ConfirmDialogProps>(), {
  eyebrow: "删除确认",
  confirmLabel: "确认",
  confirmButtonClassName: "danger-button",
  loading: false,
});

defineEmits<{
  close: [];
  confirm: [];
}>();

const descriptionId = computed(() =>
  props.description ? `${props.titleId}-description` : undefined,
);
</script>

<template>
  <DialogShell
    :open="open"
    :title-id="titleId"
    :title="title"
    :described-by-id="descriptionId"
    :dialog-class-name="joinClasses('confirm-dialog', dialogClassName)"
    dialog-role="alertdialog"
    :close-disabled="loading"
    @close="$emit('close')"
  >
    <template #eyebrow>{{ eyebrow }}</template>

    <template #actions>
      <button
        v-if="cancelLabel"
        class="secondary-button"
        :disabled="loading"
        type="button"
        @click="$emit('close')"
      >
        {{ cancelLabel }}
      </button>
      <button
        :class="confirmButtonClassName"
        :disabled="loading"
        type="button"
        @click="$emit('confirm')"
      >
        {{ confirmLabel }}
      </button>
    </template>

    <p v-if="description" :id="descriptionId" class="confirm-dialog__description">
      {{ description }}
    </p>
    <div v-if="$slots.default" class="confirm-dialog__content">
      <slot />
    </div>
  </DialogShell>
</template>
