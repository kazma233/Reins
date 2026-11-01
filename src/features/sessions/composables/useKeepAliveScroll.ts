import { onActivated, onBeforeUnmount, onMounted } from "vue";

// KeepAlive 缓存的会话视图在切走时,浏览器会把内部滚动位置丢掉(子树被移进隐藏
// 容器,没有布局盒);失活钩子里再读已经是 0,滚动事件在部分 webview 里对程序化
// 滚动也不派发。所以由各视图在挂载时登记"取当前偏移"的回调,模式切换前主动抓
// 一次(app store 的 $onAction 在 action 执行前回调,此时 DOM 还没被移走),
// 视图激活时再写回。
type ContainerGetter = () => HTMLElement | null;

type KeepAliveScrollOptions = {
  /* v-if 重建的容器(会话列表从详情返回时整体重建)不走 onActivated,挂载时
     就要回放最近一次抓拍的偏移。内容随选择变化的容器(会话详情)不能开,
     否则会把上一个会话的滚动位置套到下一个会话上。 */
  restoreOnMount?: boolean;
};

const containers = new Map<string, ContainerGetter>();
const offsets = new Map<string, number>();

/// 切走会话视图前调用一次,记下所有已登记容器的滚动位置。
export function captureKeepAliveScroll() {
  for (const [key, getContainer] of containers) {
    const element = getContainer();
    if (element) {
      offsets.set(key, element.scrollTop);
    }
  }
}

function restoreOffset(key: string, getContainer: ContainerGetter) {
  const element = getContainer();
  const offset = offsets.get(key) ?? 0;
  if (element && offset > 0) {
    element.scrollTop = offset;
  }
}

export function useKeepAliveScroll(
  key: string,
  getContainer: ContainerGetter,
  options: KeepAliveScrollOptions = {},
) {
  onMounted(() => {
    containers.set(key, getContainer);
    if (options.restoreOnMount) {
      restoreOffset(key, getContainer);
    }
  });

  onBeforeUnmount(() => {
    containers.delete(key);
  });

  onActivated(() => {
    restoreOffset(key, getContainer);
  });
}
