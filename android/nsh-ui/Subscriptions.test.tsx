import { expect, open, test } from "nsh/test";

test("subscriptions render", async () => {
  const data = {
    subscriptions: [{ index: 0, name: "A", displayUrl: "https://x/…", enabled: true }],
    nodes: [],
    groups: [],
  };
  const app = await open("./App.tsx", { host: { params: { tab: "subscriptions", data: { subscriptions: data } } } });
  expect(app.texts()).toContain("订阅");
  expect(app.texts()).toContain("A");
});

test("desktop subscriptions renders the tables and summary", async () => {
  const app = await open("./App.tsx", {
    width: 1400,
    height: 900,
    platform: "macos",
    host: {
      params: {
        embedded: true,
        tab: "subscriptions",
        data: {
          subscriptions: {
            subscriptions: [
              { index: 0, name: "我的订阅", displayUrl: "https://example.com/sub", enabled: true },
            ],
            nodes: [{ index: 0, name: "手动节点 01", enabled: false }],
            groups: [
              { index: 0, name: "自动测速", strategy: "urltest", subscriptionIndexes: [0], nodeIndexes: [] },
            ],
          },
        },
      },
    },
  });
  expect(app.texts()).toContain("我的订阅");
  expect(app.texts()).toContain("手动节点 01");
  expect(app.texts()).toContain("自动测速");
  expect(app.texts()).toContain("汇总");
});
