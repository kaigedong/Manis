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
  expect(app.texts()).toContain("路由链");
});

test("desktop connection renders the status and overview", async () => {
  const app = await open("./App.tsx", {
    width: 1400,
    height: 900,
    platform: "macos",
    host: {
      params: {
        embedded: true,
        tab: "connection",
        data: {
          connection: {
            active: true,
            node: "香港 01",
            mode: "rule",
            summary: "规则模式 · 香港 01",
            error: "",
            unreachable: false,
            warnings: [],
            warningCount: 0,
            hasSources: true,
          },
        },
      },
    },
  });
  expect(app.texts()).toContain("已连接");
  expect(app.texts()).toContain("概览");
  expect(app.texts()).toContain("路由模式");
});
