import { describe, expect, it } from "vitest";
import { isNearBottom } from "./infinite-scroll";

// node 环境没有布局,用属性桩构造距离场景;只测纯判定函数。
function stubElement(distances: {
  scrollHeight: number;
  scrollTop: number;
  clientHeight: number;
}): HTMLElement {
  const element = {} as HTMLElement;
  Object.assign(element, distances);
  return element;
}

describe("isNearBottom", () => {
  it("剩余距离不超过阈值即视为接近底部", () => {
    const element = stubElement({ scrollHeight: 1000, scrollTop: 700, clientHeight: 300 });
    // remaining = 0
    expect(isNearBottom(element)).toBe(true);
    // remaining = 240,恰好触界
    expect(isNearBottom(stubElement({ scrollHeight: 1240, scrollTop: 700, clientHeight: 300 }))).toBe(true);
    // remaining = 241
    expect(isNearBottom(stubElement({ scrollHeight: 1241, scrollTop: 700, clientHeight: 300 }))).toBe(false);
  });

  it("阈值可自定义", () => {
    const element = stubElement({ scrollHeight: 1100, scrollTop: 700, clientHeight: 300 });
    // remaining = 100
    expect(isNearBottom(element, 100)).toBe(true);
    expect(isNearBottom(element, 99)).toBe(false);
  });
});
