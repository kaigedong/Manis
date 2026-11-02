import { expect, open, test } from "nsh/test";

const nodesPage = {
  global: { label: "全局", current: "香港 01", total: 2 },
  sources: [
    {
      id: "subscription-0",
      name: "我的订阅",
      detail: "订阅 · example.com",
      kind: "subscription",
      enabled: true,
      nodes: [
        { name: "香港 01", displayName: "香港 01", delayMs: 42, current: true },
        { name: "日本 02", displayName: "日本 02", delayMs: 78, current: false },
      ],
    },
  ],
  policyGroups: [],
};

test("renders source groups and current node", async () => {
  const app = await open("./App.tsx", { host: { params: { tab: "nodes", data: { nodes: nodesPage } } } });
  expect(app.texts()).toContain("我的订阅");
  expect(app.texts()).toContain("香港 01");
  expect(app.texts()).toContain("当前");
});
