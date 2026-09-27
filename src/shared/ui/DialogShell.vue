<script lang="ts">
// 弹窗栈状态必须放在模块级供所有 DialogShell 实例共享：
// 放在 <script setup> 里每个实例各持一份，isTopDialog 对每个实例都为真，
// 按 Esc 会把所有层级的弹窗一次性全部关闭。
let openDialogCount = 0;
let originalBodyOverflow = "";
const dialogStack: HTMLElement[] = [];
</script>

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
    document.addEventListener("keydown", handleDocumentKeyDown);

    const animationFrameId = window.requestAnimationFrame(() => {
      const dialog = dialogRef.value;
      if (!dialog) return;
      focusInitialElement(dialog);
    });

    onCleanup(() => {
      window.cancelAnimationFrame(animationFrameId);
      document.removeEventListener("focusin", handleFocusIn);
      document.removeEventListener("keydown", handleDocumentKeyDown);
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

// 点击外部关闭要求按下与松开都发生在弹窗外：弹窗内按下拖到外面
// 松开（文本选择、滑块拖动等）不应触发关闭。
let backdropPointerDown = false;

function handleBackdropPointerDown(event: PointerEvent) {
  backdropPointerDown = event.target === event.currentTarget;
}

function handleBackdropPointerUp(event: PointerEvent) {
  const pressedOnBackdrop = backdropPointerDown;
  backdropPointerDown = false;
  if (!pressedOnBackdrop) return;
  if (!props.dismissible) return;
  if (event.target !== event.currentTarget) return;
  closeDialog();
}

function handleDocumentKeyDown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  if (!props.dismissible) return;
  // 多层弹窗时只关栈顶一层：焦点可能仍留在下层弹窗的元素上，
  // 此时按 Esc 不能把未聚焦的上层（及其内嵌子弹窗）一并关掉。
  const dialog = dialogRef.value;
  if (!dialog || !isTopDialog(dialog)) return;
  event.preventDefault();
  closeDialog();
}

function handleKeyDown(event: KeyboardEvent) {
  // Escape 在 document 级监听里统一处理（见 handleDocumentKeyDown），
  // 这里只负责焦点陷阱。
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
      @pointerdown="handleBackdropPointerDown"
      @pointerup="handleBackdropPointerUp"
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
