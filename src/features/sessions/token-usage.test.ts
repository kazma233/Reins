import { describe, expect, it } from "vitest";
import {
  cacheHitRate,
  formatCacheHitRate,
  formatSessionUsage,
  totalConsumedTokens
} from "./token-usage";
import type { SessionTokenUsage } from "./types";

function usage(
  inputTokens: number,
  outputTokens: number,
  cacheReadTokens: number,
  cacheWriteTokens: number
): SessionTokenUsage {
  return { inputTokens, outputTokens, cacheReadTokens, cacheWriteTokens };
}

describe("totalConsumedTokens", () => {
  it("总消耗是四项之和", () => {
    expect(totalConsumedTokens(usage(100, 20, 800, 40))).toBe(960);
    expect(totalConsumedTokens(usage(0, 0, 0, 0))).toBe(0);
  });
});

describe("cacheHitRate", () => {
  it("缓存读 / (输入 + 缓存读)", () => {
    expect(cacheHitRate(usage(110, 0, 1200, 0))).toBeCloseTo(1200 / 1310);
  });

  it("prompt 为 0 时无意义,返回 null", () => {
    expect(cacheHitRate(usage(0, 5, 0, 3))).toBeNull();
  });
});

describe("formatCacheHitRate", () => {
  it("99.6% 不因四舍五入显示成假 100%", () => {
    expect(formatCacheHitRate(0.996)).toBe("99.6%");
    expect(formatCacheHitRate(1)).toBe("100%");
    expect(formatCacheHitRate(null)).toBe("—");
  });
});

describe("formatSessionUsage", () => {
  it("总消耗后面跟缓存命中率", () => {
    // 命中率 = 缓存读 /(输入 + 缓存读) = 1200 / 1310
    expect(formatSessionUsage(usage(110, 20, 1200, 40))).toBe("⛁ 1.4k (91.6%)");
  });

  it("完全没有缓存读时命中率是 0%", () => {
    expect(formatSessionUsage(usage(100, 20, 0, 0))).toBe("⛁ 120 (0%)");
  });

  it("prompt 为 0 时命中率无意义,显示占位", () => {
    expect(formatSessionUsage(usage(0, 5, 0, 0))).toBe("⛁ 5 (—)");
  });
});
