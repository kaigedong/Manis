import { expect, open, test } from "nsh/test";

test("subscriptions render", async () => {
  const data = {
    subscriptions: [{ index: 0, name: "A", displayUrl: "https://x/…", enabled: true }],
    nodes: [],
    groups: [],
  };
  const app = await open("./App.tsx", { host: { params: { page: "subscriptions", data } } });
  expect(app.texts()).toContain("订阅");
  expect(app.texts()).toContain("A");
});
