import { describe, expect, it } from "vitest";
import { formatSourceAppName } from "./source-app";

// 删除入口的可见性断言已随 canDeleteSession 一起迁移到后端 plan 测试
// （src-tauri/src/tests/session_delete_plan.rs）；这里只钉来源产品名。
describe("Grok Build session contract", () => {
  it("labels the Grok Build source", () => {
    expect(formatSourceAppName("grokbuild")).toBe("Grok Build");
  });
});

describe("Pi session contract", () => {
  it("labels the Pi source", () => {
    expect(formatSourceAppName("pi")).toBe("Pi");
  });
});

describe("DSH session contract", () => {
  it("labels the DSH source", () => {
    expect(formatSourceAppName("dsh")).toBe("DeepSeek Harness");
  });
});
