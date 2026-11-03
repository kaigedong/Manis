import { expect, open, test } from "nsh/test";

test("logs page shows the kernel log lines", async () => {
  const app = await open("./App.tsx", {
    host: {
      params: { tab: "logs" },
      handlers: { "settings.logs": { logs: ["INFO started", "WARN retry"] } },
    },
  });
  await app.setViewport(390, 844);
  expect(app.texts()).toContain("INFO started");
});

test("desktop logs renders the log panel", async () => {
  const app = await open("./App.tsx", {
    width: 1400,
    height: 900,
    platform: "macos",
    host: {
      params: { embedded: true, tab: "logs" },
      handlers: { "settings.logs": { logs: ["INFO started"] } },
    },
  });
  await app.setViewport(1400, 900);
  expect(app.texts()).toContain("说明");
  expect(app.texts()).toContain("INFO started");
});
