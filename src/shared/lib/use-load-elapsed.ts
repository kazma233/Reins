import { computed, onScopeDispose, ref, watch, type Ref } from "vue";

// 加载中界面的耗时指示:active 翻真时从 0 起表、每 100ms 刷新已用时,翻假时
// 定格在最终读数(不清零),加载完成的一瞬仍能看到这次等了多久。
export function useLoadElapsed(active: Ref<boolean>) {
  const elapsedSeconds = ref(0);
  const text = computed(() => `${elapsedSeconds.value.toFixed(1)}s`);

  let startedAt = 0;
  let timer: ReturnType<typeof setInterval> | undefined;

  function stop() {
    if (timer !== undefined) {
      clearInterval(timer);
      timer = undefined;
    }
    if (startedAt > 0) {
      elapsedSeconds.value = (performance.now() - startedAt) / 1000;
      startedAt = 0;
    }
  }

  watch(
    active,
    (on) => {
      if (on) {
        stop();
        startedAt = performance.now();
        elapsedSeconds.value = 0;
        timer = setInterval(() => {
          elapsedSeconds.value = (performance.now() - startedAt) / 1000;
        }, 100);
      } else {
        stop();
      }
    },
    { immediate: true }
  );

  onScopeDispose(stop);

  return { elapsedSeconds, text };
}
