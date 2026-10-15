import { formatTokenCount } from "@shared/lib/format";
import type { SessionTokenUsage } from "./types";

// 会话 token 展示口径的唯一来源：会话列表 chip、会话详情、用量统计
// 共用；将来加 token 桶只改这里。

// 总消耗 = 四项之和：经过模型的全部 token，分项明细留在详情页。
export function totalConsumedTokens(usage: SessionTokenUsage): number {
  return (
    usage.inputTokens +
    usage.outputTokens +
    usage.cacheReadTokens +
    usage.cacheWriteTokens
  );
}

// 缓存命中率 = 缓存读 / (输入 + 缓存读)，与 pi 的 CH、magpie 的 hit rate
// 同口径；prompt 为 0 时无意义，返回 null。
export function cacheHitRate(usage: SessionTokenUsage): number | null {
  const prompt = usage.inputTokens + usage.cacheReadTokens;
  return prompt > 0 ? usage.cacheReadTokens / prompt : null;
}

// 整数百分比四舍五入会把 99.6% 显示成假 100%，除精确命中外按一位小数
// 向下取整。
export function formatCacheHitRate(rate: number | null): string {
  if (rate === null) {
    return "—";
  }
  if (rate === 1) {
    return "100%";
  }
  const percent = Math.floor(rate * 1000) / 10;
  return Number.isInteger(percent) ? `${percent}%` : `${percent.toFixed(1)}%`;
}

// 会话列表的用量文案：总消耗 + 缓存命中率（分项明细留在详情页）。
export function formatSessionUsage(usage: SessionTokenUsage): string {
  return `⛁ ${formatTokenCount(totalConsumedTokens(usage))} (${formatCacheHitRate(
    cacheHitRate(usage)
  )})`;
}
