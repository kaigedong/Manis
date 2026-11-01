import { host, Scroll, signal, Text, View } from "nsh";
import { Input } from "nsh/base";

type Rule = {
  index: number;
  kind: string;
  value: string;
  policy: string;
  policyLabel: string;
  enabled: boolean;
};

type Data = {
  rules: Rule[];
  kinds: string[];
  policies: string[];
  warnings: string[];
  summary: string;
};

type Reply = { data?: Data; message?: string };

type Editor = {
  index: number | null;
  kind: string;
  value: string;
  policy: string;
  enabled: boolean;
};

const params = (host.params ?? {}) as { data?: Data; embedded?: boolean };
const empty: Data = {
  rules: [],
  kinds: ["DOMAIN-SUFFIX", "DOMAIN", "DOMAIN-KEYWORD", "IP-CIDR", "IP6-CIDR", "GEOIP", "IP-ASN", "DST-PORT", "FINAL"],
  policies: ["DIRECT", "REJECT", "__MANIS_GLOBAL__"],
  warnings: [],
  summary: "",
};
const page = signal<Data>(params.data ?? empty);
const message = signal("");
const busy = signal(false);
const editor = signal<Editor | null>(null);

host.on("rules.updated", (payload) => {
  const next = (payload as { data?: Data } | undefined)?.data;
  if (next) page.set(next);
});

async function invoke(command: string, payload: Record<string, unknown> = {}) {
  if (!host.available()) {
    message.set("独立运行模式：没有宿主可执行");
    return null;
  }
  busy.set(true);
  try {
    const reply = (await host.call(command, payload)) as Reply | null;
    if (reply?.data) page.set(reply.data);
    message.set(reply?.message ?? "");
    return reply;
  } catch (error) {
    message.set(error instanceof Error ? error.message : "操作失败");
    return null;
  } finally {
    busy.set(false);
  }
}

function openNew() {
  editor.set({
    index: null,
    kind: "DOMAIN-SUFFIX",
    value: "",
    policy: "__MANIS_GLOBAL__",
    enabled: true,
  });
}

function openEdit(rule: Rule) {
  editor.set({
    index: rule.index,
    kind: rule.kind,
    value: rule.value,
    policy: rule.policy,
    enabled: rule.enabled,
  });
}

function updateEditor(patch: Partial<Editor>) {
  const current = editor();
  if (current) editor.set({ ...current, ...patch });
}

async function saveRule() {
  const current = editor();
  if (!current) return;
  const reply = await invoke("rules.save", current);
  if (reply?.message?.startsWith("规则已保存")) editor.set(null);
}

function Action(props: {
  label: string;
  onPress: () => void;
  primary?: boolean;
  disabled?: boolean;
}) {
  const disabled = () => props.disabled || busy();
  return (
    <View
      onPress={disabled() ? undefined : props.onPress}
      role="button"
      accessible
      disabled={disabled()}
      style={{
        opacity: disabled() ? 0.45 : 1,
        borderRadius: 10,
        paddingHorizontal: 14,
        paddingVertical: 8,
        backgroundColor: props.primary ? "#7c3aed" : "#ede9fe",
      }}
    >
      <Text style={{ fontSize: 14, color: props.primary ? "#ffffff" : "#5b21b6" }}>
        {props.label}
      </Text>
    </View>
  );
}

function Card(props: { children: any }) {
  return (
    <View
      style={{
        marginHorizontal: 16,
        marginTop: 14,
        borderWidth: 1,
        borderColor: "#e5e7eb",
        borderRadius: 16,
        padding: 14,
        gap: 10,
        backgroundColor: "#ffffff",
      }}
    >
      {props.children}
    </View>
  );
}

function Chip(props: {
  label: string;
  active: boolean;
  onPress: () => void;
}) {
  return (
    <View
      onPress={props.onPress}
      role="button"
      accessible
      accessibilityState={{ selected: props.active }}
      style={{
        borderRadius: 8,
        paddingHorizontal: 10,
        paddingVertical: 6,
        backgroundColor: props.active ? "#7c3aed" : "#f3f4f6",
      }}
    >
      <Text style={{ fontSize: 12, color: props.active ? "#ffffff" : "#374151" }}>
        {props.label}
      </Text>
    </View>
  );
}

function RuleForm() {
  const current = () => editor();
  return (
    <Card>
      <Text role="header" style={{ fontSize: 18, fontWeight: "600" }}>
        {current()?.index === null ? "添加规则" : "编辑规则"}
      </Text>
      <Text style={{ fontSize: 12, color: "#6b7280" }}>类型</Text>
      <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
        {page().kinds.map((kind) => (
          <Chip
            label={kind}
            active={current()?.kind === kind}
            onPress={() => updateEditor({ kind, value: kind === "FINAL" ? "" : current()?.value ?? "" })}
          />
        ))}
      </View>
      {current()?.kind !== "FINAL" ? (
        <Input
          value={current()?.value ?? ""}
          onChange={(value) => updateEditor({ value })}
          placeholder={kindPlaceholder(current()?.kind ?? "")}
          style={{ borderWidth: 1, borderColor: "#d1d5db", borderRadius: 10, padding: 10 }}
        />
      ) : null}
      <Text style={{ fontSize: 12, color: "#6b7280" }}>策略</Text>
      <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
        {page().policies.map((policy) => (
          <Chip
            label={policy === "__MANIS_GLOBAL__" ? "全局" : policy}
            active={current()?.policy === policy}
            onPress={() => updateEditor({ policy })}
          />
        ))}
      </View>
      <Text style={{ fontSize: 12, color: "#6b7280" }}>{kindHelp(current()?.kind ?? "")}</Text>
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action label="保存" primary onPress={() => void saveRule()} />
        <Action label="取消" onPress={() => editor.set(null)} />
      </View>
    </Card>
  );
}

function kindPlaceholder(kind: string): string {
  switch (kind) {
    case "DOMAIN":
    case "DOMAIN-SUFFIX":
      return "example.com";
    case "DOMAIN-KEYWORD":
      return "google";
    case "DOMAIN-WILDCARD":
      return "*.example.com";
    case "IP-CIDR":
      return "10.0.0.0/8";
    case "IP6-CIDR":
      return "2001:db8::/32";
    case "GEOIP":
      return "CN";
    case "IP-ASN":
      return "13335";
    case "DST-PORT":
      return "443";
    default:
      return "";
  }
}

function kindHelp(kind: string): string {
  switch (kind) {
    case "DOMAIN":
      return "只匹配完整域名。";
    case "DOMAIN-SUFFIX":
      return "匹配该域名及其所有子域名。";
    case "DOMAIN-KEYWORD":
      return "域名中包含该关键字即匹配。";
    case "DOMAIN-WILDCARD":
      return "支持 * 和 ? 的域名通配符。";
    case "IP-CIDR":
    case "IP6-CIDR":
      return "按目标 IP 网段匹配，不触发 DNS 解析。";
    case "GEOIP":
      return "按目标 IP 的国家/地区代码匹配。";
    case "IP-ASN":
      return "按目标 IP 所属 ASN 匹配。";
    case "DST-PORT":
      return "按目标端口匹配。";
    case "FINAL":
      return "兜底规则，匹配所有剩余流量；只能启用一条。";
    default:
      return "";
  }
}

export function RulesPage() {
  return (
    <View class={params.embedded ? "flex-1 bg-background" : "flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"}>
      <Scroll style={{ flex: 1 }}>
        <View style={{ paddingHorizontal: 20, paddingTop: 18, paddingBottom: 24, gap: 12 }}>
          <Text style={{ fontSize: 28, fontWeight: "600" }}>规则</Text>
          <Text style={{ fontSize: 13, color: "#6b7280" }}>
            按顺序匹配；未命中的流量自动走全局组。
          </Text>
          <View style={{ flexDirection: "row", gap: 8, marginTop: 4 }}>
            <Action label={busy() ? "处理中…" : "应用"} primary onPress={() => void invoke("rules.apply")} />
          </View>
          {message() ? (
            <Text style={{ fontSize: 12, color: "#6b7280" }}>{message()}</Text>
          ) : null}

          {editor() ? <RuleForm /> : null}

          <Card>
            <View style={{ flexDirection: "row", alignItems: "center", gap: 10 }}>
              <Text role="header" style={{ fontSize: 18, fontWeight: "600", flex: 1 }}>
                规则
              </Text>
              <Action label="添加" onPress={openNew} />
            </View>
            {page().rules.length === 0 ? (
              <Text style={{ fontSize: 12, color: "#6b7280" }}>
                还没有规则。添加一条 DOMAIN-SUFFIX 或 IP-CIDR 规则即可开始。
              </Text>
            ) : null}
            {page().rules.map((rule) => (
              <View
                style={{
                  borderTopWidth: 1,
                  borderTopColor: "#f3f4f6",
                  paddingTop: 10,
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 8,
                }}
              >
                <View style={{ flex: 1, gap: 3 }}>
                  <Text style={{ fontSize: 12, color: "#7c3aed" }}>{rule.kind}</Text>
                  <Text style={{ fontSize: 15 }}>{rule.kind === "FINAL" ? "所有剩余流量" : rule.value}</Text>
                  <Text style={{ fontSize: 12, color: "#6b7280" }}>→ {rule.policyLabel}</Text>
                </View>
                <View
                  onPress={() => void invoke("rules.toggle", { index: rule.index, enabled: !rule.enabled })}
                  role="button"
                  accessible
                  accessibilityLabel={rule.enabled ? "停用规则" : "启用规则"}
                  style={{
                    borderRadius: 8,
                    paddingHorizontal: 8,
                    paddingVertical: 5,
                    backgroundColor: rule.enabled ? "#dcfce7" : "#f3f4f6",
                  }}
                >
                  <Text style={{ fontSize: 12, color: rule.enabled ? "#166534" : "#6b7280" }}>
                    {rule.enabled ? "已启用" : "已停用"}
                  </Text>
                </View>
                <View
                  onPress={() => openEdit(rule)}
                  role="button"
                  accessible
                  accessibilityLabel="编辑规则"
                  style={{ paddingHorizontal: 6, paddingVertical: 5 }}
                >
                  <Text style={{ fontSize: 13, color: "#5b21b6" }}>编辑</Text>
                </View>
                <View
                  onPress={() => void invoke("rules.delete", { index: rule.index })}
                  role="button"
                  accessible
                  accessibilityLabel="删除规则"
                  style={{ paddingHorizontal: 6, paddingVertical: 5 }}
                >
                  <Text style={{ fontSize: 13, color: "#b91c1c" }}>删除</Text>
                </View>
              </View>
            ))}
            <Text style={{ fontSize: 12, color: "#6b7280" }}>
              规则从上到下匹配；策略可以是 DIRECT、REJECT 或任意策略组。最终规则之后的规则不会再生效。
            </Text>
          </Card>

          {page().warnings.length > 0 ? (
            <Card>
              <Text role="header" style={{ fontSize: 16, fontWeight: "600" }}>解析提示</Text>
              {page().warnings.map((warning, index) => (
                <Text key={index} style={{ fontSize: 12, color: "#6b7280" }}>{warning}</Text>
              ))}
            </Card>
          ) : null}

          <Text style={{ fontSize: 12, color: "#6b7280" }}>
            {page().summary || "尚未连接，应用后可在「连接」页查看摘要"}
          </Text>
        </View>
      </Scroll>
    </View>
  );
}
