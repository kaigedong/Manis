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

test("desktop shell renders the three-column policy workspace", async () => {
  const app = await open("./App.tsx", {
    width: 1400,
    height: 900,
    platform: "macos",
    host: {
      params: {
        embedded: true,
        tab: "nodes",
        data: {
          nodes: {
            ...nodesPage,
            policyGroups: [
              {
                id: "policy-0",
                label: "香港",
                kind: "手动选择",
                current: "香港 01",
                members: [
                  { name: "香港 01", displayName: "香港 01", delayMs: 42, current: true },
                  { name: "香港 02", displayName: "香港 02", delayMs: 51, current: false },
                ],
              },
            ],
            global: { label: "全局", current: "香港 01", total: 3 },
          },
          connection: { active: true, mode: "rule", summary: "规则模式 · 香港 01" },
          rules: { summary: "12 条规则", warnings: [] },
        },
      },
    },
  });
  expect(app.texts()).toContain("策略组");
  expect(app.texts()).toContain("香港");
  expect(app.texts()).toContain("路由链");
  expect(app.texts()).toContain("当前节点：香港 01");
});

test("desktop sidebar collapses to an icon rail", async () => {
  const app = await open("./App.tsx", {
    width: 1400,
    height: 900,
    platform: "macos",
    host: { params: { embedded: true, tab: "nodes", data: { nodes: nodesPage } } },
  });
  expect(app.texts()).toContain("策略路由工作台");
  await app.press("收起侧栏");
  expect(app.texts()).not.toContain("策略路由工作台");
  expect(app.texts()).toContain("»");
});
