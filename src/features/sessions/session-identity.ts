import type { SourceApp } from "./types";

// 会话身份的唯一前端主键:(来源, source_sessionId) 复合键。transcript_path
// 只是展示字段,不参与去重、选中或导航。
export function sessionIdentityKey(
  sourceApp: SourceApp,
  sourceSessionId: string
): string {
  return `${sourceApp}:${sourceSessionId}`;
}
