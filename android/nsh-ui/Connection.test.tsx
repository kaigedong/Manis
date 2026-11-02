import { expect, open, test } from "nsh/test";

test("renders connection summary", async () => {
  const data = {
    active: true,
    node: "香港 01",
    mode: "rule",
    summary: "1 订阅 · 1 节点",
    error: "",
    unreachable: false,
    warnings: [],
    warningCount: 0,
    hasSources: true,
  };
  const app = await open("./App.tsx", { host: { params: { tab: "connection", data: { connection: data } } } });
  expect(app.texts()).toContain("已连接");
  expect(app.texts()).toContain("香港 01");
  expect(app.texts()).toContain("规则");
});
