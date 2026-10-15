import { describe, expect, it } from "vitest";
import {
  formatSessionModel,
  formatSessionModels,
  messageModelLabelKeys,
  sessionModelsBadge
} from "./session-models";
import type { TimelineItem } from "./timeline-group";
import type { SessionMessage } from "./types";

describe("formatSessionModel", () => {
  it("有 provider 时拼成 provider/model", () => {
    expect(formatSessionModel({ model: "deepseek-flash", provider: "new-provider" })).toBe(
      "new-provider/deepseek-flash"
    );
  });

  it("没有 provider 的来源只显示模型名", () => {
    expect(formatSessionModel({ model: "glm-5.3" })).toBe("glm-5.3");
  });
});

describe("sessionModelsBadge", () => {
  it("没有模型时不给角标", () => {
    expect(sessionModelsBadge([])).toBeNull();
  });

  it("单模型直接显示模型名", () => {
    expect(sessionModelsBadge([{ model: "gpt-5.6-terra" }])).toBe("gpt-5.6-terra");
  });

  it("多模型显示主模型与其余数量", () => {
    expect(
      sessionModelsBadge([
        { model: "gpt-5.6-terra", provider: "custom" },
        { model: "gpt-5.5", provider: "custom" }
      ])
    ).toBe("gpt-5.6-terra +1");
  });
});

describe("formatSessionModels", () => {
  it("按顺序拼接全部模型", () => {
    expect(
      formatSessionModels([
        { model: "deepseek-flash", provider: "reins-fanggeek" },
        { model: "GLM-5.3", provider: "account:bigmodel" },
        { model: "grok-4.5" }
      ])
    ).toBe("reins-fanggeek/deepseek-flash、account:bigmodel/GLM-5.3、grok-4.5");
  });
});

function textItem(
  key: string,
  messageId: string,
  role: "user" | "assistant",
  model?: SessionMessage["model"]
): TimelineItem {
  return {
    kind: "text",
    key,
    role,
    message: {
      id: messageId,
      role,
      timestamp: null,
      blocks: [{ kind: "text", text: "内容", toolName: null, toolCallId: null, payload: null }],
      model
    },
    blocks: []
  };
}

describe("messageModelLabelKeys", () => {
  it("输入与输出各挂一次标签", () => {
    const keys = messageModelLabelKeys([
      textItem("m1:a", "m1", "user", { model: "glm-5.3" }),
      textItem("m2:a", "m2", "assistant", { model: "glm-5.3" })
    ]);
    expect([...keys]).toEqual(["m1:a", "m2:a"]);
  });

  it("同一条消息被切成多段文本时只挂首段", () => {
    const keys = messageModelLabelKeys([
      textItem("m1:text-0", "m1", "assistant", { model: "glm-5.3" }),
      textItem("m1:text-2", "m1", "assistant", { model: "glm-5.3" })
    ]);
    expect([...keys]).toEqual(["m1:text-0"]);
  });

  it("没有模型信息的消息不挂标签", () => {
    const keys = messageModelLabelKeys([
      textItem("m1:a", "m1", "assistant"),
      textItem("m2:a", "m2", "assistant")
    ]);
    expect(keys.size).toBe(0);
  });
});
