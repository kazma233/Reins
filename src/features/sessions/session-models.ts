import type { TimelineItem } from "./timeline-group";
import type { SessionModel } from "./types";

// 模型展示的三处口径(列表角标 / 详情整列 / 消息行)共用这一份:
// provider 是各工具自己的 provider id,有则拼成 "provider/model",
// Claude Code 与 Grok Build 没有该字段,只显示模型名。

export function formatSessionModel(model: SessionModel): string {
  return model.provider ? `${model.provider}/${model.model}` : model.model;
}

// 会话级展示:全部用过模型的长文案,列表角标的 title 与详情页共用。
export function formatSessionModels(models: SessionModel[]): string {
  return models.map(formatSessionModel).join("、");
}

// 列表角标:主模型(后端按用量排序后的首项)加其余模型数量,太长会挤坏卡片。
export function sessionModelsBadge(models: SessionModel[]): string | null {
  const primary = models[0];
  if (!primary) {
    return null;
  }
  return models.length > 1
    ? `${primary.model} +${models.length - 1}`
    : primary.model;
}

// 消息级标签挂在哪几条:只有输入与输出(带模型的文本单元),一条消息被过程行
// 切成多段文本时只挂首段。返回需要挂标签的单元 key。
export function messageModelLabelKeys(items: TimelineItem[]): Set<string> {
  const keys = new Set<string>();
  const seenMessages = new Set<string>();
  for (const item of items) {
    if (item.kind !== "text" || seenMessages.has(item.message.id)) {
      continue;
    }
    seenMessages.add(item.message.id);
    if (item.message.model) {
      keys.add(item.key);
    }
  }
  return keys;
}
