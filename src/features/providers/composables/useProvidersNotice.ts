import { createNoticeSingleton } from "@shared/lib/notice";

// 模块级单例：所有调用 useProvidersNotice 的 composable 共享同一个
// notice ref，由 ProvidersWorkspace.vue 渲染唯一的 toast。
const singleton = createNoticeSingleton();

export function useProvidersNotice() {
  return singleton;
}
