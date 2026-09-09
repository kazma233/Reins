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
