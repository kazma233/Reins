import { describe, expect, it } from "vitest";
import type { SourceApp, UsageSourceStats } from "./types";
import {
  buildDayAxis,
  buildHourAxis,
  computeMetrics,
  formatCacheHitRate,
  niceCeil,
  shiftDay,
  sourceSeries,
  todaySources,
  windowSourceSummaries,
  windowSources
} from "./model";

type DayFixture = Array<[string, [number, number, number, number]]>;

function rawSourceStats(
  sourceApp: SourceApp,
  days: DayFixture,
  todayHours: Array<[number, [number, number, number, number]]> = []
): UsageSourceStats {
  const point = ([input, output, cacheRead, cacheWrite]: [number, number, number, number]) => ({
    inputTokens: input,
    outputTokens: output,
    cacheReadTokens: cacheRead,
    cacheWriteTokens: cacheWrite
  });
  return {
    sourceApp,
    sessionCount: days.length + todayHours.length,
    todaySessionCount: todayHours.length,
    days: days.map(([day, usage]) => ({ day, usage: point(usage) })),
    todayHours: todayHours.map(([hour, usage]) => ({ hour, usage: point(usage) }))
  };
}

describe("buildDayAxis", () => {
  it("补齐中间缺失的天并裁剪到窗口", () => {
    const axis = buildDayAxis(["2026-09-01", "2026-09-05"], 7, "2026-09-07");
    expect(axis).toEqual([
      "2026-09-01",
      "2026-09-02",
      "2026-09-03",
      "2026-09-04",
      "2026-09-05",
      "2026-09-06",
      "2026-09-07"
    ]);
  });

  it("窗口外的早期日期被裁掉", () => {
    const axis = buildDayAxis(["2026-01-01", "2026-09-06"], 7, "2026-09-07");
    expect(axis[0]).toBe("2026-09-01");
    expect(axis).toHaveLength(7);
    expect(axis[axis.length - 1]).toBe("2026-09-07");
  });

  it("全部窗口从最早记录起到今天", () => {
    const axis = buildDayAxis(["2026-09-06", "2026-09-07"], 0, "2026-09-07");
    expect(axis).toEqual(["2026-09-06", "2026-09-07"]);
  });

  it("忽略晚于今天的日期,无有效日期时返回空轴", () => {
    expect(buildDayAxis(["2026-09-09"], 7, "2026-09-07")).toEqual([]);
    expect(buildDayAxis([], 7, "2026-09-07")).toEqual([]);
  });
});

describe("buildHourAxis", () => {
  it("固定 24 个两位小时串", () => {
    const axis = buildHourAxis();
    expect(axis).toHaveLength(24);
    expect(axis[0]).toBe("00");
    expect(axis[9]).toBe("09");
    expect(axis[23]).toBe("23");
  });
});

describe("shiftDay", () => {
  it("跨月与跨年边界", () => {
    expect(shiftDay("2026-09-01", -1)).toBe("2026-08-31");
    expect(shiftDay("2026-12-31", 1)).toBe("2027-01-01");
    expect(shiftDay("2024-02-28", 1)).toBe("2024-02-29");
  });
});

describe("windowSources / todaySources", () => {
  it("窗口模式裁剪轴外的天,会话数保持全周期口径", () => {
    const raw = rawSourceStats("codex", [
      ["2026-09-01", [30, 0, 0, 0]],
      ["2026-08-01", [100, 0, 0, 0]]
    ]);
    const normalized = windowSources([raw], ["2026-09-01"]);
    expect(normalized[0].points).toEqual([{ bucket: "2026-09-01", usage: expect.objectContaining({ inputTokens: 30 }) }]);
    expect(normalized[0].sessionCount).toBe(2);
  });

  it("今日模式转成两位小时桶,会话数换今日口径", () => {
    const raw = rawSourceStats("codex", [["2026-09-30", [9, 9, 9, 9]]], [
      [9, [30, 3, 0, 0]],
      [14, [5, 7, 0, 0]]
    ]);
    const normalized = todaySources([raw]);
    expect(normalized[0].points).toEqual([
      { bucket: "09", usage: expect.objectContaining({ inputTokens: 30 }) },
      { bucket: "14", usage: expect.objectContaining({ inputTokens: 5 }) }
    ]);
    expect(normalized[0].sessionCount).toBe(2);
  });
});

describe("sourceSeries", () => {
  it("按轴补零", () => {
    const points = [
      { bucket: "2026-09-01", usage: { inputTokens: 10, outputTokens: 5, cacheReadTokens: 0, cacheWriteTokens: 0 } },
      { bucket: "2026-09-03", usage: { inputTokens: 1, outputTokens: 1, cacheReadTokens: 1, cacheWriteTokens: 1 } }
    ];
    expect(sourceSeries(points, ["2026-09-01", "2026-09-02", "2026-09-03"])).toEqual([15, 0, 4]);
  });
});

describe("windowSourceSummaries", () => {
  it("按占比降序,无消耗的来源不出现", () => {
    const normalized = windowSources(
      [
        rawSourceStats("codex", [
          ["2026-09-01", [30, 0, 0, 0]],
          ["2026-08-01", [100, 0, 0, 0]]
        ]),
        rawSourceStats("pi", [["2026-09-02", [10, 10, 0, 0]]])
      ],
      ["2026-09-01", "2026-09-02"]
    );
    const summaries = windowSourceSummaries(normalized, ["2026-09-01", "2026-09-02"]);
    expect(summaries.map((item) => item.sourceApp)).toEqual(["codex", "pi"]);
    expect(summaries[0].tokens).toBe(30);
    expect(summaries[1].tokens).toBe(20);
    expect(summaries[0].share).toBeCloseTo(0.6);
  });

  it("轴内无消耗时为空", () => {
    const normalized = windowSources([rawSourceStats("codex", [["2026-08-01", [10, 0, 0, 0]]])], ["2026-09-01"]);
    expect(windowSourceSummaries(normalized, ["2026-09-01"])).toEqual([]);
  });
});

describe("computeMetrics", () => {
  it("汇总四项、日均、峰值桶与命中率", () => {
    const normalized = windowSources(
      [
        rawSourceStats("codex", [
          ["2026-09-01", [100, 20, 1000, 50]],
          ["2026-09-03", [10, 5, 200, 0]]
        ])
      ],
      ["2026-09-01", "2026-09-02", "2026-09-03"]
    );
    const metrics = computeMetrics(normalized, ["2026-09-01", "2026-09-02", "2026-09-03"]);
    expect(metrics.totalTokens).toBe(1385);
    expect(metrics.inputTokens).toBe(110);
    expect(metrics.outputTokens).toBe(25);
    expect(metrics.cacheReadTokens).toBe(1200);
    expect(metrics.cacheWriteTokens).toBe(50);
    expect(metrics.dailyAverage).toBeCloseTo(1385 / 3);
    expect(metrics.peak).toEqual({ bucket: "2026-09-01", tokens: 1170 });
    expect(metrics.cacheHitRate).toBeCloseTo(1200 / 1310);
    expect(metrics.sessionCount).toBe(2);
  });

  it("同一天多来源合并后取峰值", () => {
    const normalized = windowSources(
      [
        rawSourceStats("codex", [["2026-09-01", [60, 0, 0, 0]]]),
        rawSourceStats("pi", [
          ["2026-09-01", [55, 0, 0, 0]],
          ["2026-09-02", [100, 0, 0, 0]]
        ])
      ],
      ["2026-09-01", "2026-09-02"]
    );
    expect(computeMetrics(normalized, ["2026-09-01", "2026-09-02"]).peak).toEqual({
      bucket: "2026-09-01",
      tokens: 115
    });
  });

  it("今日模式:峰值桶是小时,会话数是今日口径", () => {
    const normalized = todaySources([
      rawSourceStats("codex", [["2026-09-30", [9, 9, 9, 9]]], [
        [9, [30, 3, 0, 0]],
        [14, [5, 7, 0, 0]]
      ])
    ]);
    const metrics = computeMetrics(normalized, buildHourAxis());
    expect(metrics.totalTokens).toBe(45);
    expect(metrics.peak).toEqual({ bucket: "09", tokens: 33 });
    expect(metrics.sessionCount).toBe(2);
    expect(metrics.dailyAverage).toBeCloseTo(45 / 24);
  });

  it("空轴无峰值且命中率不可计算", () => {
    const metrics = computeMetrics(
      [{ sourceApp: "codex", sessionCount: 0, points: [] }],
      []
    );
    expect(metrics.peak).toBeNull();
    expect(metrics.cacheHitRate).toBeNull();
    expect(metrics.totalTokens).toBe(0);
  });
});

describe("formatCacheHitRate", () => {
  it("99.6% 不因四舍五入显示成假 100%", () => {
    expect(formatCacheHitRate(0.996)).toBe("99.6%");
    expect(formatCacheHitRate(1)).toBe("100%");
    expect(formatCacheHitRate(null)).toBe("—");
  });
});

describe("niceCeil", () => {
  it("取整到 1/2/5 的倍数刻度", () => {
    expect(niceCeil(0)).toBe(1);
    expect(niceCeil(7)).toBe(10);
    expect(niceCeil(1_400_000)).toBe(2_000_000);
    expect(niceCeil(600_000)).toBe(1_000_000);
  });
});
