import { onMounted, watch, type WatchSource } from "vue";

// 无业务语义的滚动加载助手:统一「接近底部判定 + 事件挂载 + 条件门」。
// 收编前这套判定在会话列表(滚动 + 视口补拉)与详情时间线(滚动)各有一份
// 手写变体,四条件门逐字重复。

// remaining = scrollHeight - scrollTop - clientHeight ≤ threshold。
export function isNearBottom(element: HTMLElement, threshold = 240): boolean {
  return (
    element.scrollHeight - element.scrollTop - element.clientHeight <= threshold
  );
}

export type InfiniteScrollOptions = {
  // 滚动容器;元素替换(路由切换、条件渲染)时自动迁移监听。
  getElement: () => HTMLElement | null;
  // 加载门(未在加载、还有更多等);不满足时不触发。
  canLoadMore: () => boolean;
  onLoadMore: () => void;
  // 提供时启用「内容短于视口自动补拉」:挂载时与 deps 变化时重查
  // (列表长度、加载位等),不提供则只在滚动时判定。
  autoFillDeps?: WatchSource[];
  threshold?: number;
};

export function useInfiniteScroll(options: InfiniteScrollOptions): void {
  const { getElement, canLoadMore, onLoadMore, autoFillDeps, threshold = 240 } =
    options;

  function check(): void {
    const element = getElement();
    if (!element || !canLoadMore() || !isNearBottom(element, threshold)) {
      return;
    }
    onLoadMore();
  }

  // 元素就位即挂 passive 监听,替换时迁移、卸载时清理。
  watch(
    getElement,
    (element, _oldElement, onCleanup) => {
      if (!element) {
        return;
      }
      element.addEventListener("scroll", check, { passive: true });
      onCleanup(() => element.removeEventListener("scroll", check));
    },
    { immediate: true }
  );

  if (autoFillDeps) {
    watch(autoFillDeps, () => check());
    onMounted(() => check());
  }
}
