import { createNoticeSingleton } from "@shared/lib/notice";

// 模块级单例：所有调用 useWorkspaceNotice 的 composable 共享同一个
// notice ref，由 Workspace.vue 渲染唯一的 toast。
const singleton = createNoticeSingleton();

export function useWorkspaceNotice() {
  return singleton;
}
