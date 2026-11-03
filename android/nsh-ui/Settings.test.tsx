import { expect, open, test } from "nsh/test";

test("renders settings summary", async () => {
  const data = {
    status: "未连接",
    mode: "rule",
    proxyDns: { mode: "doh", custom: [], discovered: [], label: "加密 DNS（DoH）" },
    kernelVersion: "v1.19.33",
    summary: "1 订阅 · 2 节点",
    subscriptions: 1,
    nodes: 2,
    groups: 0,
    abi: "arm64-v8a",
    dataDir: "files/mihomo",
    installedVersion: "0.1.110",
    update: { state: "idle" },
  };
  const app = await open("./App.tsx", { host: { params: { tab: "settings", data: { settings: data } } } });
  expect(app.texts()).toContain("设置");
  expect(app.texts()).toContain("v1.19.33");
  expect(app.texts()).toContain("1 订阅 · 2 节点");
});

test("desktop settings renders the sections and status panel", async () => {
  const app = await open("./App.tsx", {
    width: 1400,
    height: 900,
    platform: "macos",
    host: {
      params: {
        embedded: true,
        tab: "settings",
        data: {
          settings: {
            status: "已连接",
            mode: "rule",
            proxyDns: { mode: "doh", custom: [], discovered: [], label: "加密 DNS（DoH）" },
            kernelVersion: "v1.19.33",
            summary: "12 条规则",
            subscriptions: 1,
            nodes: 2,
            groups: 1,
            abi: "arm64-v8a",
            dataDir: "files/mihomo",
            installedVersion: "0.1.123",
            update: { state: "idle" },
          },
        },
      },
    },
  });
  expect(app.texts()).toContain("配置摘要");
  expect(app.texts()).toContain("状态");
  expect(app.texts()).toContain("工作区");
});
