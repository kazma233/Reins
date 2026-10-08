import { describe, expect, it } from "vitest";
import { clampCenteredOffset, computeTooltipOffset } from "./tooltip-position";

const trigger = { top: 100, left: 300, right: 316, bottom: 116 };
const viewport = { width: 1200, height: 800 };

describe("computeTooltipOffset", () => {
  it("opens below the trigger by default", () => {
    const offset = computeTooltipOffset({
      trigger,
      bubble: { width: 200, height: 30 },
      viewport,
      placement: "bottom",
      align: "start",
    });

    expect(offset).toEqual({ top: 122, left: 300 });
  });

  it("opens above and right-aligns when asked", () => {
    const offset = computeTooltipOffset({
      trigger,
      bubble: { width: 200, height: 30 },
      viewport,
      placement: "top",
      align: "end",
    });

    expect(offset).toEqual({ top: 64, left: 116 });
  });

  it("keeps the bubble inside the viewport on both axes", () => {
    const offset = computeTooltipOffset({
      trigger: { top: 10, left: 1180, right: 1196, bottom: 26 },
      bubble: { width: 320, height: 60 },
      viewport,
      placement: "bottom",
      align: "start",
    });

    expect(offset.left).toBe(1200 - 320 - 8);
    expect(offset.top).toBe(32);
  });

  it("clamps to the near edge when neither side has room", () => {
    const offset = computeTooltipOffset({
      trigger: { top: 4, left: 2, right: 18, bottom: 20 },
      bubble: { width: 320, height: 60 },
      // 视口比气泡还矮：翻面也放不下，只能贴住上边缘
      viewport: { width: 1200, height: 40 },
      placement: "top",
      align: "start",
    });

    expect(offset).toEqual({ top: 8, left: 8 });
  });

  it("falls back to the near edge when the bubble is wider than the viewport", () => {
    const offset = computeTooltipOffset({
      trigger,
      bubble: { width: 2000, height: 30 },
      viewport,
      placement: "bottom",
      align: "end",
    });

    expect(offset.left).toBe(8);
  });
});

describe("computeTooltipOffset 贴边翻面", () => {
  it("flips above when there is no room below", () => {
    const offset = computeTooltipOffset({
      trigger: { top: 760, left: 300, right: 316, bottom: 776 },
      bubble: { width: 200, height: 30 },
      viewport,
      placement: "bottom",
      align: "start",
    });

    expect(offset.top).toBe(760 - 30 - 6);
  });

  it("flips below when there is no room above", () => {
    const offset = computeTooltipOffset({
      trigger: { top: 2, left: 300, right: 316, bottom: 18 },
      bubble: { width: 200, height: 30 },
      viewport,
      placement: "top",
      align: "start",
    });

    expect(offset.top).toBe(24);
  });
});

describe("clampCenteredOffset", () => {
  it("keeps the center when the bubble fits", () => {
    expect(
      clampCenteredOffset({ center: 400, bubbleWidth: 150, containerWidth: 900 }),
    ).toBe(400);
  });

  it("shifts the bubble inside when the cursor is near the right edge", () => {
    expect(
      clampCenteredOffset({ center: 890, bubbleWidth: 150, containerWidth: 900 }),
    ).toBe(900 - 75);
  });

  it("shifts the bubble inside when the cursor is near the left edge", () => {
    expect(
      clampCenteredOffset({ center: 4, bubbleWidth: 150, containerWidth: 900 }),
    ).toBe(75);
  });

  it("falls back to the near edge when the bubble is wider than the container", () => {
    expect(
      clampCenteredOffset({ center: 100, bubbleWidth: 1200, containerWidth: 900 }),
    ).toBe(600);
  });
});
