import { describe, expect, it } from "vitest";
import { formatTokenCount } from "./format";

describe("formatTokenCount", () => {
  it("abbreviates token counts for compact chips", () => {
    expect(formatTokenCount(636783)).toBe("636.8k");
    expect(formatTokenCount(2916352)).toBe("2.9M");
    expect(formatTokenCount(18)).toBe("18");
  });
});
