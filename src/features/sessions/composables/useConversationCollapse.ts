import { ref, watch, type Ref } from "vue";

// 过程行(思考/工具/错误)的展开状态:默认收起,点一下展开。
// 「只看对话」切到开启时强制收起当前已展开的行;开关之后用户单独展开的行
// 保持展开,直到下次切换——这正是"收起而不是隐藏"的行为边界。
export function useConversationCollapse(
  conversationOnly: () => boolean
): { expanded: Ref<boolean>; toggle: () => void } {
  const expanded = ref(false);

  watch(conversationOnly, (on) => {
    if (on) {
      expanded.value = false;
    }
  });

  function toggle() {
    expanded.value = !expanded.value;
  }

  return { expanded, toggle };
}
