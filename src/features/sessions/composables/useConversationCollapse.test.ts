import { describe, expect, it } from "vitest";
import { nextTick, ref } from "vue";
import { useConversationCollapse } from "./useConversationCollapse";

describe("useConversationCollapse", () => {
  it("默认收起,点击后展开", () => {
    const conversationOnly = ref(false);
    const { expanded, toggle } = useConversationCollapse(() => conversationOnly.value);

    expect(expanded.value).toBe(false);
    toggle();
    expect(expanded.value).toBe(true);
    toggle();
    expect(expanded.value).toBe(false);
  });

  it("只看对话开启时收起已展开的行", async () => {
    const conversationOnly = ref(false);
    const { expanded, toggle } = useConversationCollapse(() => conversationOnly.value);

    toggle();
    expect(expanded.value).toBe(true);

    conversationOnly.value = true;
    await nextTick();
    expect(expanded.value).toBe(false);
  });

  it("开关保持开启时仍可单独展开这一行", async () => {
    const conversationOnly = ref(true);
    const { expanded, toggle } = useConversationCollapse(() => conversationOnly.value);
    await nextTick();

    toggle();
    expect(expanded.value).toBe(true);

    // 开关值没变,不能再把这一行收回去。
    await nextTick();
    expect(expanded.value).toBe(true);
  });

  it("取消后再次开启会重新收起", async () => {
    const conversationOnly = ref(true);
    const { expanded, toggle } = useConversationCollapse(() => conversationOnly.value);

    toggle();
    expect(expanded.value).toBe(true);

    conversationOnly.value = false;
    await nextTick();
    expect(expanded.value).toBe(true);

    conversationOnly.value = true;
    await nextTick();
    expect(expanded.value).toBe(false);
  });
});
