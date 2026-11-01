const ZH_TIMESTAMP_FORMATTER = new Intl.DateTimeFormat("zh-CN", {
  dateStyle: "medium",
  timeStyle: "short"
});

export function formatTimestamp(timestamp: number | null): string {
  if (!timestamp) {
    return "未知";
  }

  return ZH_TIMESTAMP_FORMATTER.format(timestamp);
}

// token 用量展示用 k/M 缩写,精确数字对阅读无用且挤占布局。
export function formatTokenCount(count: number): string {
  if (Math.abs(count) >= 1_000_000) {
    return `${(count / 1_000_000).toFixed(1)}M`;
  }
  if (Math.abs(count) >= 1_000) {
    return `${(count / 1_000).toFixed(1)}k`;
  }
  return `${count}`;
}
