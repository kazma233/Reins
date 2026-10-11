import { describe, expect, it } from "vitest";
import { cacheHitRate, formatCacheHitRate, totalConsumedTokens } from "./token-usage";
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
