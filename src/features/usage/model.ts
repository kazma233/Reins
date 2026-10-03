import type {
  SessionTokenUsage,
  SourceApp,
  UsageStats,
  UsageSourceStats,
} from "./types";

// 每来源固定颜色:曲线、峰值圆点、筛选 chip 共用,同一 agent 视觉一致。
// 色板取浅色主题下可区分的六个色相,避开 danger/success 等语义色。
export const SOURCE_COLORS: Record<SourceApp, string> = {
  codex: "#0052ff",
  claude_code: "#d97706",
  opencode: "#16a34a",
  pi: "#9333ea",
  grokbuild: "#dc2626",
  zcode: "#0e7490",
};

// days=1 是"今日"窗口:轴与序列切换为小时粒度;0 表示全部。
export const USAGE_WINDOWS = [
  { days: 1, label: "今日" },
  { days: 7, label: "7 天" },
  { days: 30, label: "30 天" },
  { days: 90, label: "90 天" },
  { days: 0, label: "全部" },
] as const;

export type UsageWindowDays = (typeof USAGE_WINDOWS)[number]["days"];

export function isUsageWindowDays(value: number): value is UsageWindowDays {
  return USAGE_WINDOWS.some((option) => option.days === value);
}

// 总消耗 = 四项之和:经过模型的全部 token,与会话列表的 chip 同口径。
export function totalTokens(usage: SessionTokenUsage): number {
  return (
    usage.inputTokens +
    usage.outputTokens +
    usage.cacheReadTokens +
    usage.cacheWriteTokens
  );
}

export function sourceColor(sourceApp: SourceApp): string {
  return SOURCE_COLORS[sourceApp] ?? "#64748b";
}

// 统一的桶序列:窗口模式下 bucket 是 "YYYY-MM-DD",今日模式下是 "HH"。
// 指标、占比与曲线都消费这份归一形态,避免两套平行逻辑。
export type UsageBucketPoint = { bucket: string; usage: SessionTokenUsage };

export type NormalizedUsageSource = {
  sourceApp: SourceApp;
  // 窗口模式为全周期会话数,今日模式为今日产生消耗的会话数。
  sessionCount: number;
  points: UsageBucketPoint[];
};

// 窗口模式:把日序列裁剪到轴内(轴外消耗不计入指标与占比)。
export function windowSources(
  sources: UsageSourceStats[],
  axis: string[]
): NormalizedUsageSource[] {
  const inWindow = new Set(axis);
  return sources.map((source) => ({
    sourceApp: source.sourceApp,
    sessionCount: source.sessionCount,
    points: source.days
      .filter((point) => inWindow.has(point.day))
      .map((point) => ({ bucket: point.day, usage: point.usage })),
  }));
}

// 今日模式:todayHours 转成两位小时串桶,会话数换成今日口径。
export function todaySources(sources: UsageSourceStats[]): NormalizedUsageSource[] {
  return sources.map((source) => ({
    sourceApp: source.sourceApp,
    sessionCount: source.todaySessionCount,
    points: source.todayHours.map((point) => ({
      bucket: String(point.hour).padStart(2, "0"),
      usage: point.usage,
    })),
  }));
}

// 今日窗口的日期轴:固定 0..23 点,无消耗的小时补零。
export function buildHourAxis(): string[] {
  return Array.from({ length: 24 }, (_, hour) => String(hour).padStart(2, "0"));
}

// 日期轴:窗口裁剪后补齐中间缺失的天,曲线在无消耗日回落到 0 而不是被压缩。
// day 是 YYYY-MM-DD(本地时区),字典序即时间序;today 注入以便测试。
export function buildDayAxis(
  days: Iterable<string>,
  windowDays: UsageWindowDays,
  today: string
): string[] {
  const present = [...new Set(days)].filter((day) => day <= today).sort();
  if (present.length === 0) {
    return [];
  }

  const windowStart =
    windowDays > 1 ? shiftDay(today, -(windowDays - 1)) : present[0];
  const start = windowStart > present[0] ? windowStart : present[0];

  const axis: string[] = [];
  for (let day = start; day <= today; day = shiftDay(day, 1)) {
    axis.push(day);
  }
  return axis;
}

// 补洞/裁剪只做日期搬移,用 UTC 日运算避免本地时区的夏令时跳变。
export function shiftDay(day: string, offsetDays: number): string {
  const [year, month, date] = day.split("-").map(Number);
  const shifted = new Date(Date.UTC(year, month - 1, date) + offsetDays * 86_400_000);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${shifted.getUTCFullYear()}-${pad(shifted.getUTCMonth() + 1)}-${pad(shifted.getUTCDate())}`;
}

// 与后端 day_key 同为本地时区:窗口右端点锚定"今天"。
export function localTodayDay(): string {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

export function sourceSeries(points: UsageBucketPoint[], axis: string[]): number[] {
  const byBucket = new Map(points.map((point) => [point.bucket, totalTokens(point.usage)]));
  return axis.map((bucket) => byBucket.get(bucket) ?? 0);
}

export type UsageSourceSummary = {
  sourceApp: SourceApp;
  color: string;
  tokens: number;
  sessionCount: number;
  share: number;
};

// 当前轴内各来源的总消耗与占比,按消耗降序排列。
export function windowSourceSummaries(
  sources: NormalizedUsageSource[],
  axis: string[]
): UsageSourceSummary[] {
  const inAxis = new Set(axis);
  const summaries = sources.map((source) => {
    const tokens = source.points
      .filter((point) => inAxis.has(point.bucket))
      .reduce((sum, point) => sum + totalTokens(point.usage), 0);
    return {
      sourceApp: source.sourceApp,
      color: sourceColor(source.sourceApp),
      tokens,
      sessionCount: source.sessionCount,
      share: 0,
    };
  });

  const total = summaries.reduce((sum, item) => sum + item.tokens, 0);
  for (const item of summaries) {
    item.share = total > 0 ? item.tokens / total : 0;
  }

  return summaries
    .filter((item) => item.tokens > 0)
    .sort((left, right) => right.tokens - left.tokens);
}

export type UsageMetrics = {
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  dailyAverage: number;
  // 轴内合并消耗最大的桶;窗口模式是"天",今日模式是"小时"。
  peak: { bucket: string; tokens: number } | null;
  // 0..1;null 表示轴内没有输入与缓存读,无法计算。
  cacheHitRate: number | null;
  // 窗口模式为全周期会话数之和,今日模式为今日会话数之和。
  sessionCount: number;
};

export function computeMetrics(
  sources: NormalizedUsageSource[],
  axis: string[]
): UsageMetrics {
  let input = 0;
  let output = 0;
  let cacheRead = 0;
  let cacheWrite = 0;

  for (const source of sources) {
    for (const point of source.points) {
      input += point.usage.inputTokens;
      output += point.usage.outputTokens;
      cacheRead += point.usage.cacheReadTokens;
      cacheWrite += point.usage.cacheWriteTokens;
    }
  }

  // 按桶合并各来源消耗,峰值取合并后的最大值。
  const bucketTotals = new Map<string, number>();
  for (const source of sources) {
    for (const point of source.points) {
      bucketTotals.set(
        point.bucket,
        (bucketTotals.get(point.bucket) ?? 0) + totalTokens(point.usage)
      );
    }
  }
  let peak: UsageMetrics["peak"] = null;
  for (const bucket of axis) {
    const tokens = bucketTotals.get(bucket) ?? 0;
    if (!peak || tokens > peak.tokens) {
      peak = { bucket, tokens };
    }
  }
  if (peak && peak.tokens === 0) {
    peak = null;
  }

  const total = input + output + cacheRead + cacheWrite;
  const prompt = input + cacheRead;

  return {
    totalTokens: total,
    inputTokens: input,
    outputTokens: output,
    cacheReadTokens: cacheRead,
    cacheWriteTokens: cacheWrite,
    dailyAverage: axis.length > 0 ? total / axis.length : 0,
    peak,
    cacheHitRate: prompt > 0 ? cacheRead / prompt : null,
    sessionCount: sources.reduce((sum, source) => sum + source.sessionCount, 0),
  };
}

// 与会话详情页同口径:整数百分比四舍五入会把 99.6% 显示成假 100%,
// 除精确命中外按一位小数向下取整。
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

// y 轴量程:向上取整到 1/2/5 × 10^n 的"好读"上限,网格线落在整数刻度上。
export function niceCeil(value: number): number {
  if (value <= 0) {
    return 1;
  }
  const exponent = Math.floor(Math.log10(value));
  const base = 10 ** exponent;
  for (const multiplier of [1, 2, 5, 10]) {
    if (value <= multiplier * base) {
      return multiplier * base;
    }
  }
  return 10 * base;
}
