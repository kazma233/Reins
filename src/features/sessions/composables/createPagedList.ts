import { computed, ref, type ComputedRef, type Ref } from "vue";

// 分页列表状态机:首屏替换 / 追加两条流共用一套加载位、竞态作废与 offset
// 推进;数据落点(本地 ref / pinia store)由 apply 声明,错误分区(首屏 /
// 追加)由 onError 声明。收编前这台机器在会话时间线与会话列表各有一份
// 手写变体(成功 / catch / finally 三处 stale 检查 × 多条流,零测试覆盖)。

export type PagedResult<T> = {
  items: T[];
  nextOffset: number | null;
  hasMore: boolean;
  // 列表侧的总数展示;时间线等无此概念的调用方可省略。
  totalCount?: number;
};

export type PagedListMode = "replace" | "append";

export type PagedListOptions<T> = {
  // 调用方的请求身份(来源、筛选条件等);null 表示当前不可加载。
  requestKey: () => string | null;
  fetchPage: (offset: number, mode: PagedListMode) => Promise<PagedResult<T>>;
  // 数据落点:replace 全量替换,append 追加。
  apply: (result: PagedResult<T>, mode: PagedListMode) => void;
  // 错误交给调用方分区:首屏错误与追加错误显示在不同区域、文案不同。
  onError: (error: unknown, mode: PagedListMode) => void;
  // 当前身份下是否还没有任何数据;loadMore 据此把「offset 0 且无数据」
  // 识别为首屏替换(时间线懒加载页的入口形态)。列表侧不提供:重拉一律
  // 走显式 replace。
  isEmpty?: () => boolean;
};

export type PagedList<T> = {
  // 加载位与记账 ref 对调用方只读:写入只发生在机器内部(run 的两条流
  // 与 takeover 的 commit/settle),手写回写等于在调用方复刻记账规则。
  loading: Readonly<Ref<boolean>>;
  loadingMore: Readonly<Ref<boolean>>;
  nextOffset: Readonly<Ref<number | null>>;
  hasMore: Readonly<Ref<boolean>>;
  // 追加门:未就绪(nextOffset null)、无更多、任一加载位占用都不放行。
  canLoadMore: ComputedRef<boolean>;
  // 最近一次成功请求的 key;列表据此重置滚动位置。
  resultKey: Readonly<Ref<string | null>>;
  // 强制替换流:递增版本作废在途请求,从 offset 0 全量重拉(来源 / 筛选
  // 切换、重试)。接管在途的追加流,加载位随之复位。
  replace: () => Promise<void>;
  // 懒加载页的就绪标记:只置 offset 0,首次 loadMore 自动按首屏替换处理。
  prime: () => void;
  loadMore: () => Promise<void>;
  // 切换身份:作废在途请求并回到未就绪态。
  reset: () => void;
  // 仅作废在途请求,不动加载进度。
  invalidate: () => void;
  currentRequestKey: () => string | null;
  // 接管式操作(如列表的 refresh:响应里还有列表以外的数据):外部请求
  // 绕过 fetchPage,但加载位、数据落点与成功回写仍是机器的记账。调用方
  // 只保留自己的旁路副作用(如 setSources)与错误分区;key 失配即 stale,
  // commit/settle 在 stale 下与 run 的对应路径同一规则(no-op / 跳过复位)。
  takeover: () => {
    key: string | null;
    isStale: () => boolean;
    begin: () => void;
    commit: (result: PagedResult<T>) => void;
    settle: () => void;
  };
};

export function createPagedList<T>(options: PagedListOptions<T>): PagedList<T> {
  const loading = ref(false);
  const loadingMore = ref(false);
  const nextOffset = ref<number | null>(null);
  const hasMore = ref(false);
  const resultKey = ref<string | null>(null);
  let version = 0;

  const canLoadMore = computed(
    () =>
      nextOffset.value !== null &&
      hasMore.value &&
      !loading.value &&
      !loadingMore.value
  );

  function currentRequestKey(): string | null {
    const base = options.requestKey();
    return base === null ? null : `${base}::${version}`;
  }

  async function run(mode: PagedListMode, offset: number): Promise<void> {
    const requestKey = currentRequestKey();
    if (requestKey === null) {
      return;
    }

    if (mode === "replace") {
      loading.value = true;
      // 接管可能在途的追加流:其 finally 会因 key 失配跳过复位。
      loadingMore.value = false;
    } else {
      loadingMore.value = true;
    }

    try {
      const result = await options.fetchPage(offset, mode);
      if (currentRequestKey() !== requestKey) {
        return;
      }
      options.apply(result, mode);
      nextOffset.value = result.nextOffset;
      hasMore.value = result.hasMore;
      resultKey.value = requestKey;
    } catch (error) {
      if (currentRequestKey() !== requestKey) {
        return;
      }
      options.onError(error, mode);
    } finally {
      if (currentRequestKey() === requestKey) {
        loading.value = false;
        loadingMore.value = false;
      }
    }
  }

  async function replace(): Promise<void> {
    version += 1;
    await run("replace", 0);
  }

  function prime(): void {
    nextOffset.value = 0;
    hasMore.value = true;
  }

  async function loadMore(): Promise<void> {
    if (!canLoadMore.value) {
      return;
    }
    const offset = nextOffset.value ?? 0;
    // 「offset 0 且当前身份还没有数据」是首屏:整页替换而非追加。
    const initialLoad = offset === 0 && (options.isEmpty?.() ?? false);
    await run(initialLoad ? "replace" : "append", offset);
  }

  function reset(): void {
    version += 1;
    loading.value = false;
    loadingMore.value = false;
    nextOffset.value = null;
    hasMore.value = false;
  }

  function invalidate(): void {
    version += 1;
  }

  function takeover() {
    invalidate();
    const key = currentRequestKey();
    return {
      key,
      isStale: () => currentRequestKey() !== key,
      begin: () => {
        loading.value = true;
        // 接管可能在途的追加流:其 finally 会因 key 失配跳过复位。
        loadingMore.value = false;
      },
      commit: (result: PagedResult<T>) => {
        // 与 run 的入口守卫同一哲学:无身份不记账。
        if (key === null || currentRequestKey() !== key) {
          return;
        }
        options.apply(result, "replace");
        nextOffset.value = result.nextOffset;
        hasMore.value = result.hasMore;
        resultKey.value = key;
      },
      settle: () => {
        if (currentRequestKey() === key) {
          loading.value = false;
        }
      }
    };
  }

  return {
    loading,
    loadingMore,
    nextOffset,
    hasMore,
    canLoadMore,
    resultKey,
    replace,
    prime,
    loadMore,
    reset,
    invalidate,
    currentRequestKey,
    takeover,
  };
}
