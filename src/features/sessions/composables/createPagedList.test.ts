import { describe, expect, it, vi } from "vitest";
import { nextTick } from "vue";
import { createPagedList, type PagedResult } from "./createPagedList";

// 分页状态机的行为测试:门控、首屏替换 / 追加、竞态作废、错误分区。
// 这些行为收编前散在会话列表与时间线的多条手写流里,零测试覆盖。

type Item = { id: number };

function page(
  items: Item[],
  nextOffset: number | null,
  totalCount = items.length
): PagedResult<Item> {
  return { items, nextOffset, hasMore: nextOffset !== null, totalCount };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function createMachine(overrides?: {
  requestKey?: () => string | null;
  isEmpty?: () => boolean;
}) {
  const applied: Array<{ ids: number[]; mode: string }> = [];
  const errors: Array<{ message: string; mode: string }> = [];
  const fetchPage = vi.fn();
  const list = createPagedList<Item>({
    requestKey: overrides?.requestKey ?? (() => "source-a"),
    fetchPage: (offset, mode) => fetchPage(offset, mode),
    apply: (result, mode) => applied.push({ ids: result.items.map((i) => i.id), mode }),
    onError: (error, mode) =>
      errors.push({ message: String((error as Error).message), mode }),
    isEmpty: overrides?.isEmpty ?? (() => applied.length === 0),
  });
  return { list, applied, errors, fetchPage };
}

describe("createPagedList 门控", () => {
  it("未就绪(nextOffset null)与无更多时 loadMore 不发请求", async () => {
    const { list, fetchPage } = createMachine();
    await list.loadMore();
    expect(fetchPage).not.toHaveBeenCalled();
    expect(list.canLoadMore.value).toBe(false);
  });

  it("加载中不放行追加", async () => {
    const { list, fetchPage } = createMachine();
    const gate = deferred<PagedResult<Item>>();
    fetchPage.mockReturnValue(gate.promise);
    list.prime();
    void list.loadMore();
    expect(list.loading.value).toBe(true);
    expect(list.canLoadMore.value).toBe(false);

    await list.loadMore();
    expect(fetchPage).toHaveBeenCalledTimes(1);
    gate.resolve(page([{ id: 1 }], 20));
    await nextTick();
  });

  it("requestKey 为 null 时不可加载", async () => {
    let key: string | null = "source-a";
    const { list, fetchPage } = createMachine({ requestKey: () => key });
    list.prime();
    key = null;
    await list.loadMore();
    expect(fetchPage).not.toHaveBeenCalled();
  });
});

describe("createPagedList 首屏与追加", () => {
  it("prime 后首次 loadMore 按 replace 处理空数据", async () => {
    const { list, applied, fetchPage } = createMachine();
    fetchPage.mockResolvedValue(page([{ id: 1 }], 1));
    list.prime();
    await list.loadMore();
    expect(fetchPage).toHaveBeenCalledWith(0, "replace");
    expect(applied).toEqual([{ ids: [1], mode: "replace" }]);
    expect(list.loading.value).toBe(false);
    expect(list.hasMore.value).toBe(true);
    expect(list.canLoadMore.value).toBe(true);
  });

  it("已有数据后 loadMore 按 append 追加并推进 offset", async () => {
    const { list, applied, fetchPage } = createMachine();
    fetchPage.mockResolvedValueOnce(page([{ id: 1 }], 1));
    fetchPage.mockResolvedValueOnce(page([{ id: 2 }, { id: 3 }], null, 3));
    list.prime();
    await list.loadMore();
    await list.loadMore();
    expect(fetchPage).toHaveBeenNthCalledWith(2, 1, "append");
    expect(applied[1]).toEqual({ ids: [2, 3], mode: "append" });
    expect(list.hasMore.value).toBe(false);
    expect(list.canLoadMore.value).toBe(false);
  });

  it("replace 强制替换模式并更新 resultKey", async () => {
    const { list, applied, fetchPage } = createMachine();
    fetchPage.mockResolvedValue(page([{ id: 9 }], null));
    await list.replace();
    expect(fetchPage).toHaveBeenCalledWith(0, "replace");
    expect(applied).toEqual([{ ids: [9], mode: "replace" }]);
    expect(list.resultKey.value).toContain("source-a");
  });

  it("不提供 isEmpty 时 loadMore 恒为 append", async () => {
    const applied: Array<{ mode: string }> = [];
    const list = createPagedList<Item>({
      requestKey: () => "k",
      fetchPage: async () => page([{ id: 1 }], null),
      apply: (_r, mode) => applied.push({ mode }),
      onError: () => {},
    });
    list.prime();
    await list.loadMore();
    expect(applied).toEqual([{ mode: "append" }]);
  });
});

describe("createPagedList 竞态", () => {
  it("replace 作废在途追加:旧结果不落盘、不复位加载位", async () => {
    const { list, applied, fetchPage } = createMachine();
    const appendGate = deferred<PagedResult<Item>>();
    const replaceGate = deferred<PagedResult<Item>>();
    fetchPage.mockImplementation((_offset, mode) =>
      mode === "append" ? appendGate.promise : replaceGate.promise
    );

    fetchPage.mockResolvedValueOnce(page([{ id: 1 }], 1));
    list.prime();
    await list.loadMore(); // 首屏成功,nextOffset=1

    void list.loadMore(); // 追加在途
    expect(list.loadingMore.value).toBe(true);

    void list.replace(); // 接管
    expect(list.loading.value).toBe(true);
    expect(list.loadingMore.value).toBe(false);

    // 旧追加流晚归:key 已失配,不得写入也不得动加载位。
    appendGate.resolve(page([{ id: 2 }], 2));
    await nextTick();
    expect(applied.some((entry) => entry.ids.includes(2))).toBe(false);
    expect(list.loading.value).toBe(true);

    replaceGate.resolve(page([{ id: 9 }], null));
    await nextTick();
    await nextTick();
    expect(applied[applied.length - 1].ids).toEqual([9]);
    expect(list.loading.value).toBe(false);
  });

  it("reset 作废在途请求并回到未就绪态", async () => {
    const { list, applied, fetchPage } = createMachine();
    const gate = deferred<PagedResult<Item>>();
    fetchPage.mockReturnValue(gate.promise);
    list.prime();
    void list.loadMore();
    list.reset();
    expect(list.loading.value).toBe(false);
    expect(list.nextOffset.value).toBeNull();

    gate.resolve(page([{ id: 1 }], 1));
    await nextTick();
    expect(applied).toEqual([]);
  });

  it("invalidate 供接管式流程做 stale 检查", async () => {
    const { list, fetchPage } = createMachine();
    fetchPage.mockResolvedValue(page([{ id: 1 }], null));
    list.invalidate();
    const keyAtStart = list.currentRequestKey();
    expect(list.currentRequestKey()).toBe(keyAtStart);
    list.invalidate();
    expect(list.currentRequestKey()).not.toBe(keyAtStart);
  });
});

describe("createPagedList 错误分区", () => {
  it("错误按模式交给 onError,加载位复位", async () => {
    const { list, errors, fetchPage } = createMachine();
    fetchPage.mockRejectedValueOnce(new Error("boom-1"));
    list.prime();
    await list.loadMore();
    expect(errors).toEqual([{ message: "boom-1", mode: "replace" }]);
    expect(list.loading.value).toBe(false);

    fetchPage.mockResolvedValueOnce(page([{ id: 1 }], 1));
    await list.loadMore();

    fetchPage.mockRejectedValueOnce(new Error("boom-2"));
    await list.loadMore();
    expect(errors[1]).toEqual({ message: "boom-2", mode: "append" });
    expect(list.loadingMore.value).toBe(false);
  });

  it("被作废的失败不进 onError", async () => {
    const { list, errors, fetchPage } = createMachine();
    const gate = deferred<PagedResult<Item>>();
    fetchPage.mockReturnValueOnce(gate.promise);
    fetchPage.mockResolvedValueOnce(page([{ id: 9 }], null));
    list.prime();
    const first = list.loadMore();
    void list.replace();
    gate.reject(new Error("stale-failure"));
    await first;
    await nextTick();
    expect(errors).toEqual([]);
  });
});

describe("createPagedList 接管", () => {
  it("begin/commit 走机器记账:加载位、apply(replace) 与成功回写", async () => {
    const { list, applied } = createMachine();
    list.prime();
    await list.loadMore();

    const request = list.takeover();
    expect(request.key).toBe("source-a::1");
    request.begin();
    expect(list.loading.value).toBe(true);
    expect(list.loadingMore.value).toBe(false);

    request.commit(page([{ id: 9 }], null));
    expect(applied).toContainEqual({ ids: [9], mode: "replace" });
    expect(list.nextOffset.value).toBe(null);
    expect(list.hasMore.value).toBe(false);
    expect(list.resultKey.value).toBe("source-a::1");
    expect(list.canLoadMore.value).toBe(false);
  });

  it("stale 后 commit 是 no-op:旧接管不能覆盖新请求的记账", async () => {
    const { list, applied } = createMachine();
    const request = list.takeover();
    // 新请求推进版本,接管持有的 key 立即过期。
    await list.replace();

    request.begin();
    expect(list.loading.value).toBe(true);
    request.commit(page([{ id: 7 }], 5));
    expect(applied).not.toContainEqual({ ids: [7], mode: "replace" });
    expect(list.resultKey.value).not.toBe(request.key);
    // settle 在 stale 下跳过复位,与 run 的 finally 同一规则。
    request.settle();
    expect(list.loading.value).toBe(true);
  });

  it("settle 在未 stale 时复位加载位", async () => {
    const { list } = createMachine();
    const request = list.takeover();
    request.begin();
    expect(list.loading.value).toBe(true);
    request.settle();
    expect(list.loading.value).toBe(false);
  });

  it("requestKey 为 null(无身份)时 key 为 null,commit 不落账", async () => {
    const { list, applied } = createMachine({
      requestKey: () => null,
    });
    const request = list.takeover();
    expect(request.key).toBe(null);
    request.begin();
    request.commit(page([{ id: 1 }], null));
    expect(applied).toHaveLength(0);
    expect(list.resultKey.value).toBe(null);
  });
});
