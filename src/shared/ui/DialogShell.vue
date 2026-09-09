<script setup lang="ts">
import { ref, watch } from "vue";
import { joinClasses } from "../lib/join-classes";
import "./dialog-shell.css";

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

let openDialogCount = 0;
let originalBodyOverflow = "";
const dialogStack: HTMLElement[] = [];

function isHTMLElement(value: Element | null): value is HTMLElement {
  return value instanceof HTMLElement;
}

function focusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (element) => element.offsetParent !== null || element === document.activeElement,
  );
}

function focusInitialElement(dialog: HTMLElement) {
  const autofocusElement = dialog.querySelector<HTMLElement>("[data-autofocus]");
  (autofocusElement ?? dialog).focus({ preventScroll: true });
}

function isTopDialog(dialog: HTMLElement): boolean {
  return dialogStack[dialogStack.length - 1] === dialog;
}

type DialogShellProps = {
  open: boolean;
  titleId: string;
  title: string;
  eyebrow?: string;
  centerTitle?: boolean;
  closeDisabled?: boolean;
  closeLabel?: string;
  describedById?: string;
  dialogClassName?: string;
  dialogRole?: "dialog" | "alertdialog";
  dismissible?: boolean;
  actionsClassName?: string;
};

const props = withDefaults(defineProps<DialogShellProps>(), {
  centerTitle: false,
  closeDisabled: false,
  closeLabel: "关闭弹窗",
  dialogRole: "dialog",
  dismissible: true,
});

const emit = defineEmits<{ close: [] }>();

const dialogRef = ref<HTMLElement | null>(null);

// flush: 'post' ensures DOM is mounted before we read dialogRef.
// Capture the dialog element so cleanup still works after v-if unmounts it.
watch(
  () => props.open,
  (open, _oldOpen, onCleanup) => {
    if (!open) return;

    const mountedDialog = dialogRef.value;
    const previousFocus = isHTMLElement(document.activeElement) ? document.activeElement : null;

    if (openDialogCount === 0) {
      originalBodyOverflow = document.body.style.overflow;
      document.body.style.overflow = "hidden";
    }
    openDialogCount += 1;

    if (mountedDialog) {
      dialogStack.push(mountedDialog);
    }

    function handleFocusIn(event: FocusEvent) {
      const dialog = dialogRef.value;
      if (!dialog || !isTopDialog(dialog) || dialog.contains(event.target as Node | null)) {
        return;
      }
      focusInitialElement(dialog);
    }

    document.addEventListener("focusin", handleFocusIn);

    const animationFrameId = window.requestAnimationFrame(() => {
      const dialog = dialogRef.value;
      if (!dialog) return;
      focusInitialElement(dialog);
    });

    onCleanup(() => {
      window.cancelAnimationFrame(animationFrameId);
      document.removeEventListener("focusin", handleFocusIn);
      if (mountedDialog) {
        const dialogIndex = dialogStack.indexOf(mountedDialog);
        if (dialogIndex !== -1) {
          dialogStack.splice(dialogIndex, 1);
        }
      }
      openDialogCount = Math.max(0, openDialogCount - 1);
      if (openDialogCount === 0) {
        document.body.style.overflow = originalBodyOverflow;
      }
      previousFocus?.focus({ preventScroll: true });
    });
  },
  { flush: "post", immediate: true },
);

function closeDialog() {
  if (props.closeDisabled) return;
  emit("close");
}

function handleBackdropClick() {
  if (!props.dismissible) return;
  closeDialog();
}

function handleKeyDown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    if (!props.dismissible) return;
    event.preventDefault();
    closeDialog();
    return;
  }

  if (event.key !== "Tab") return;

  const dialog = dialogRef.value;
  if (!dialog) return;

  const elements = focusableElements(dialog);
  if (elements.length === 0) {
    event.preventDefault();
    dialog.focus({ preventScroll: true });
    return;
  }

  const first = elements[0];
  const last = elements[elements.length - 1];

  if (event.shiftKey) {
    if (document.activeElement === first || !dialog.contains(document.activeElement)) {
      event.preventDefault();
      last.focus();
    }
    return;
  }

  if (document.activeElement === last || !dialog.contains(document.activeElement)) {
    event.preventDefault();
    first.focus();
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="dialog-backdrop"
      role="presentation"
      @click="handleBackdropClick"
    >
      <section
        ref="dialogRef"
        :aria-describedby="describedById"
        :aria-labelledby="titleId"
        aria-modal="true"
        :class="joinClasses('dialog-card', 'dialog-panel', dialogClassName)"
        :role="dialogRole"
        tabindex="-1"
        @click.stop
        @keydown="handleKeyDown"
      >
        <div class="dialog-header">
          <div :class="joinClasses('dialog-hero', centerTitle && 'dialog-hero-centered')">
            <span v-if="centerTitle" class="dialog-hero-spacer" aria-hidden="true" />
            <div
              :class="joinClasses('dialog-title-group', centerTitle && 'dialog-title-group-centered')"
            >
              <p v-if="eyebrow || $slots.eyebrow" class="dialog-eyebrow">
                <slot name="eyebrow">{{ eyebrow }}</slot>
              </p>
              <h3 :id="titleId">{{ title }}</h3>
            </div>
            <button
              :aria-label="closeLabel"
              class="dialog-close-button"
              :disabled="closeDisabled"
              type="button"
              @click="closeDialog"
            >
              <svg aria-hidden="true" focusable="false" viewBox="0 0 24 24">
                <path
                  d="M6.7 5.3 12 10.6l5.3-5.3 1.4 1.4-5.3 5.3 5.3 5.3-1.4 1.4-5.3-5.3-5.3 5.3-1.4-1.4 5.3-5.3-5.3-5.3z"
                />
              </svg>
            </button>
          </div>
        </div>

        <div class="dialog-body">
          <div class="dialog-scroll">
            <div class="dialog-scroll-content">
              <slot />
            </div>
          </div>
        </div>

        <div v-if="$slots.actions" :class="joinClasses('dialog-actions', actionsClassName)">
          <slot name="actions" />
        </div>
      </section>
    </div>
  </Teleport>
</template>
