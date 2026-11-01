import { expect, open, test } from "nsh/test";

test("renders rules and policies", async () => {
  const data = {
    rules: [
      {
        index: 0,
        kind: "DOMAIN-SUFFIX",
        value: "example.com",
        policy: "__MANIS_GLOBAL__",
        policyLabel: "全局",
        enabled: true,
      },
    ],
    kinds: ["DOMAIN-SUFFIX", "FINAL"],
    policies: ["DIRECT", "__MANIS_GLOBAL__"],
    warnings: [],
    summary: "1 条规则",
  };
  const app = await open("./App.tsx", { host: { params: { tab: "rules", data: { rules: data } } } });
  expect(app.texts()).toContain("规则");
  expect(app.texts()).toContain("example.com");
  expect(app.texts().some((text) => text.includes("全局"))).toBe(true);
});
