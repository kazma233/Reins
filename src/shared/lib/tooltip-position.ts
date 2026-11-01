// 浮层定位：气泡挂在 body 上用 fixed 定位，位置由触发元素与气泡尺寸算出。
// 抽成纯函数是为了能直接测边界收敛（视口与裁剪都不会裁掉它）。
export type TooltipPlacement = "bottom" | "top";
export type TooltipAlign = "start" | "end";

type Rect = {
  top: number;
  left: number;
  right: number;
  bottom: number;
};

export type TooltipOffsetInput = {
  trigger: Rect;
  bubble: { width: number; height: number };
  viewport: { width: number; height: number };
  placement: TooltipPlacement;
  align: TooltipAlign;
  // 与触发元素的间距
  gap?: number;
  // 距视口边缘至少保留的距离
  margin?: number;
};

export function computeTooltipOffset(input: TooltipOffsetInput): { top: number; left: number } {
  const { trigger, bubble, viewport, placement, align } = input;
  const gap = input.gap ?? 6;
  const margin = input.margin ?? 8;

  const preferredLeft = align === "end" ? trigger.right - bubble.width : trigger.left;

  const belowTop = trigger.bottom + gap;
  const aboveTop = trigger.top - bubble.height - gap;
  const fitsBelow = belowTop + bubble.height <= viewport.height - margin;
  const fitsAbove = aboveTop >= margin;
  // 贴到视口边缘时先换到另一侧，换不了才夹取位置，避免压在触发元素上
  const preferredTop =
    placement === "top"
      ? fitsAbove || !fitsBelow
        ? aboveTop
        : belowTop
      : fitsBelow || !fitsAbove
        ? belowTop
        : aboveTop;

  // 视口内收；气泡比视口还大时退化成贴左 / 贴上，不再做无意义的 clamp
  const maxLeft = Math.max(margin, viewport.width - bubble.width - margin);
  const maxTop = Math.max(margin, viewport.height - bubble.height - margin);

  return {
    left: Math.min(Math.max(margin, preferredLeft), maxLeft),
    top: Math.min(Math.max(margin, preferredTop), maxTop),
  };
}

export type CenteredTooltipInput = {
  center: number;
  bubbleWidth: number;
  containerWidth: number;
  margin?: number;
};

// 以容器内某个点为圆心居中的浮层（如跟随光标的图表气泡）：
// 中心点夹取到容器范围内，两端贴近边缘时整体移进来，不会一半探出被裁掉。
export function clampCenteredOffset(input: CenteredTooltipInput): number {
  const { center, bubbleWidth, containerWidth } = input;
  const margin = input.margin ?? 0;
  const half = bubbleWidth / 2;
  const min = half + margin;
  const max = Math.max(min, containerWidth - half - margin);
  return Math.min(Math.max(center, min), max);
}
