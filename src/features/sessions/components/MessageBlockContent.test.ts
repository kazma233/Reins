import { describe, expect, it } from "vitest";
import { createSSRApp, h } from "vue";
import { renderToString } from "vue/server-renderer";
import MessageBlockContent from "./MessageBlockContent.vue";

async function renderBlock(kind: string, contentText: string) {
  const app = createSSRApp({
    render: () => h(MessageBlockContent, { kind, contentText })
  });
  return renderToString(app);
}

describe("MessageBlockContent", () => {
  it("文本块按 markdown 渲染 GFM：表格、删除线、任务列表", async () => {
    const html = await renderBlock(
      "text",
      "| a |\n| - |\n| 1 |\n\n~~s~~\n\n- [x] done"
    );
    expect(html).toContain("<table>");
    expect(html).toContain("<del>");
    expect(html).toContain('type="checkbox"');
  });

  it("原始 HTML 转义为可见文本，不生成元素", async () => {
    const html = await renderBlock(
      "text",
      "<b>x</b>\n\n<script>alert(1)</script>"
    );
    expect(html).not.toContain("<b>");
    expect(html).not.toContain("<script");
    expect(html).toContain("&lt;b&gt;");
  });

  it("段内单换行显示为换行", async () => {
    const html = await renderBlock("text", "一\n二");
    expect(html).toContain("<br");
  });

  it("非文本块按原文 pre 展示，不走 markdown", async () => {
    const html = await renderBlock("tool_result", "| not | a | table |");
    expect(html).not.toContain("<table>");
    expect(html).toContain("| not | a | table |");
  });
});
