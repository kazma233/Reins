import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope, nextTick, ref } from "vue";
import { useLoadElapsed } from "./use-load-elapsed";

// 计时行为:起表、按 100ms 刷新、停止定格、再次触发重新起表、scope 退出清理。
// active 是 watch 驱动的,翻转后都要等一次 tick 再断言。
describe("useLoadElapsed", () => {
  let now: number;

  beforeEach(() => {
    now = 1_000;
    vi.spyOn(performance, "now").mockImplementation(() => now);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  function advance(timeMs: number) {
    now += timeMs;
    vi.advanceTimersByTime(timeMs);
  }

  it("starts at zero and refreshes while active", async () => {
    vi.useFakeTimers();
    const scope = effectScope();
    const state = scope.run(() => {
      const active = ref(false);
      const timer = useLoadElapsed(active);
      active.value = true;
      return timer;
    })!;
    await nextTick();

    advance(250);
    expect(state.text.value).toBe("0.2s");
    advance(400);
    expect(state.text.value).toBe("0.6s");

    scope.stop();
    vi.useRealTimers();
  });

  it("freezes at the final readout when inactive and does not tick further", async () => {
    vi.useFakeTimers();
    const scope = effectScope();
    const state = scope.run(() => {
      const active = ref(false);
      const timer = useLoadElapsed(active);
      active.value = true;
      return { timer, active };
    })!;
    await nextTick();

    advance(300);
    state.active.value = false;
    await nextTick();
    expect(state.timer.text.value).toBe("0.3s");
    advance(5_000);
    expect(state.timer.text.value).toBe("0.3s");

    scope.stop();
    vi.useRealTimers();
  });

  it("restarts from zero when reactivated", async () => {
    vi.useFakeTimers();
    const scope = effectScope();
    const state = scope.run(() => {
      const active = ref(false);
      const timer = useLoadElapsed(active);
      active.value = true;
      return { timer, active };
    })!;
    await nextTick();

    advance(500);
    state.active.value = false;
    await nextTick();
    state.active.value = true;
    await nextTick();
    expect(state.timer.text.value).toBe("0.0s");
    advance(150);
    expect(state.timer.text.value).toBe("0.1s");

    scope.stop();
    vi.useRealTimers();
  });
});
