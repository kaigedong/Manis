import { computed, host, Scroll, signal, Text, View } from "nsh";
import { Input } from "nsh/base";
import { embedded, layoutWidth, root } from "./store";
import { t } from "./copy";
import { desktopColors } from "./theme";
import { Action, Chip, RuleRow } from "./ui";

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

type Reply = { data?: unknown; message?: string };

type Editor = {
  index: number | null;
  kind: string;
  value: string;
  policy: string;
  enabled: boolean;
};

const empty: Data = {
  rules: [],
  kinds: ["DOMAIN-SUFFIX", "DOMAIN", "DOMAIN-KEYWORD", "IP-CIDR", "IP6-CIDR", "GEOIP", "IP-ASN", "DST-PORT", "FINAL"],
  policies: ["DIRECT", "REJECT", "__MANIS_GLOBAL__"],
  warnings: [],
  summary: "",
};
const page = computed<Data>(() => (root().rules ?? empty) as Data);
const message = signal("");
const busy = signal(false);
const editor = signal<Editor | null>(null);

async function invoke(command: string, payload: Record<string, unknown> = {}) {
  if (!host.available()) {
    message.set(t("subs.noHost"));
    return null;
  }
  busy.set(true);
  try {
    const reply = (await host.call(command, payload)) as Reply | null;
    if (reply?.data) root.set(reply.data as any);
    message.set(reply?.message ?? "");
    return reply;
  } catch (error) {
    message.set(error instanceof Error ? error.message : t("subs.failed"));
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

function RuleForm() {
  const current = () => editor();
  return (
    <View
      style={
        embedded
          ? {
              borderWidth: 1,
              borderColor: desktopColors.primary,
              borderRadius: 10,
              padding: 12,
              gap: 8,
              backgroundColor: "#F7FBFA",
            }
          : {
              marginHorizontal: 16,
              marginTop: 14,
              borderWidth: 1,
              borderColor: "#e5e7eb",
              borderRadius: 16,
              padding: 14,
              gap: 10,
              backgroundColor: "#ffffff",
            }
      }
    >
      <Text
        role="header"
        style={{
          fontSize: embedded ? 13 : 18,
          fontWeight: "600",
          color: embedded ? desktopColors.text : undefined,
        }}
      >
        {current()?.index === null ? t("rules.addRule") : t("rules.editRule")}
      </Text>
      <Text style={{ fontSize: embedded ? 11 : 12, color: embedded ? desktopColors.tertiary : "#6b7280" }}>{t("rules.kindLabel")}</Text>
      <View style={{ flexDirection: "row", flexWrap: "wrap", gap: embedded ? 4 : 6 }}>
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
          style={
            embedded
              ? {
                  borderWidth: 1,
                  borderColor: desktopColors.outlineStrong,
                  borderRadius: 8,
                  paddingHorizontal: 10,
                  paddingVertical: 7,
                  fontSize: 12,
                }
              : { borderWidth: 1, borderColor: "#d1d5db", borderRadius: 10, padding: 10 }
          }
        />
      ) : null}
      <Text style={{ fontSize: embedded ? 11 : 12, color: embedded ? desktopColors.tertiary : "#6b7280" }}>{t("rules.policyLabel")}</Text>
      <View style={{ flexDirection: "row", flexWrap: "wrap", gap: embedded ? 4 : 6 }}>
        {page().policies.map((policy) => (
          <Chip
            label={policy === "__MANIS_GLOBAL__" ? "全局" : policy}
            active={current()?.policy === policy}
            onPress={() => updateEditor({ policy })}
          />
        ))}
      </View>
      <Text style={{ fontSize: embedded ? 11 : 12, color: embedded ? desktopColors.secondary : "#6b7280" }}>
        {kindHelp(current()?.kind ?? "")}
      </Text>
      <View style={{ flexDirection: "row", gap: embedded ? 6 : 8 }}>
        <Action label={t("rules.save")} primary onPress={() => void saveRule()} />
        <Action label={t("rules.cancel")} onPress={() => editor.set(null)} />
      </View>
    </View>
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

function MobileRulesPage() {
  return (
    <View class={embedded ? "flex-1 bg-background" : "flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"}>
      <Scroll style={{ flex: 1 }}>
        <View style={{ paddingHorizontal: 20, paddingTop: 18, paddingBottom: 24, gap: 12 }}>
          <Text style={{ fontSize: 28, fontWeight: "600" }}>{t("rules.title")}</Text>
          <Text style={{ fontSize: 13, color: "#6b7280" }}>
            {t("rules.hint")}
          </Text>
          <View style={{ flexDirection: "row", gap: 8, marginTop: 4 }}>
            <Action label={busy() ? t("nodes.refreshing") : t("rules.apply")} primary onPress={() => void invoke("rules.apply")} />
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
              <Action label={t("rules.add")} onPress={openNew} />
            </View>
            {page().rules.length === 0 ? (
              <Text style={{ fontSize: 12, color: "#6b7280" }}>
                {t("rules.empty")}
              </Text>
            ) : null}
            {page().rules.map((rule) => (
              <RuleRow
                index={rule.index}
                kind={rule.kind}
                value={rule.value}
                policyLabel={rule.policyLabel}
                enabled={rule.enabled}
                onToggle={() => void invoke("rules.toggle", { index: rule.index, enabled: !rule.enabled })}
                onEdit={() => openEdit(rule)}
                onDelete={() => void invoke("rules.delete", { index: rule.index })}
              />
            ))}
            <Text style={{ fontSize: 12, color: "#6b7280" }}>
              {t("rules.noteBody")}
            </Text>
          </Card>

          {page().warnings.length > 0 ? (
            <Card>
              <Text role="header" style={{ fontSize: 16, fontWeight: "600" }}>{t("rules.warnings")}</Text>
              {page().warnings.map((warning, index) => (
                <Text style={{ fontSize: 12, color: "#6b7280" }}>{warning}</Text>
              ))}
            </Card>
          ) : null}

          <Text style={{ fontSize: 12, color: "#6b7280" }}>
            {page().summary || t("rules.noSummary")}
          </Text>
        </View>
      </Scroll>
    </View>
  );
}

function DesktopRulesPage() {
  return (
    <View style={{ flex: 1, flexDirection: "row", backgroundColor: desktopColors.high }}>
      <View style={{ flex: 1, minWidth: 0, flexDirection: "column" }}>
        <View
          style={{
            flexDirection: "row",
            alignItems: "center",
            gap: 8,
            paddingHorizontal: 16,
            paddingVertical: 10,
            borderBottomWidth: 1,
            borderBottomColor: desktopColors.outline,
          }}
        >
          <Text style={{ fontSize: 12, color: desktopColors.secondary, flex: 1 }}>
            {message() || page().summary || t("rules.hint")}
          </Text>
          <Action label={t("rules.add")} onPress={openNew} />
          <Action label={busy() ? t("nodes.refreshing") : t("rules.apply")} primary onPress={() => void invoke("rules.apply")} />
        </View>
        <View
          style={{
            flexDirection: "row",
            gap: 10,
            paddingHorizontal: 12,
            paddingVertical: 6,
            backgroundColor: desktopColors.low,
            borderBottomWidth: 1,
            borderBottomColor: desktopColors.outline,
          }}
        >
          <Text style={{ fontSize: 10, color: desktopColors.tertiary, width: 34 }}>{t("rules.col.order")}</Text>
          <Text style={{ fontSize: 10, color: desktopColors.tertiary, width: 120 }}>{t("rules.kindLabel")}</Text>
          <Text style={{ fontSize: 10, color: desktopColors.tertiary, flex: 1 }}>{t("rules.col.match")}</Text>
          <Text style={{ fontSize: 10, color: desktopColors.tertiary, width: 110 }}>{t("rules.policyLabel")}</Text>
          <Text style={{ fontSize: 10, color: desktopColors.tertiary, width: 90 }}>{t("rules.col.state")}</Text>
          <Text style={{ fontSize: 10, color: desktopColors.tertiary, width: 64 }}>{t("rules.col.actions")}</Text>
        </View>
        <Scroll style={{ flex: 1 }}>
          {page().rules.map((rule) => (
            <RuleRow
              index={rule.index}
              kind={rule.kind}
              value={rule.value}
              policyLabel={rule.policyLabel}
              enabled={rule.enabled}
              onToggle={() => void invoke("rules.toggle", { index: rule.index, enabled: !rule.enabled })}
              onEdit={() => openEdit(rule)}
              onDelete={() => void invoke("rules.delete", { index: rule.index })}
            />
          ))}
          {page().rules.length === 0 ? (
            <Text style={{ padding: 16, fontSize: 12, color: desktopColors.tertiary }}>
              {t("rules.empty")}
            </Text>
          ) : null}
        </Scroll>
      </View>
      {layoutWidth() === 0 || layoutWidth() >= 1100 ? (
      <View style={{ width: 320, borderLeftWidth: 1, borderLeftColor: desktopColors.outline, backgroundColor: desktopColors.low }}>
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
          {editor() ? <RuleForm /> : null}
          <View
            style={{
              borderWidth: 1,
              borderColor: desktopColors.outline,
              borderRadius: 10,
              backgroundColor: desktopColors.high,
              padding: 12,
              gap: 6,
            }}
          >
            <Text style={{ fontSize: 11, fontWeight: "600", color: desktopColors.tertiary }}>{t("rules.summary")}</Text>
            <Text style={{ fontSize: 12, color: desktopColors.text }}>{page().summary || t("rules.noSummary")}</Text>
            <Text style={{ fontSize: 11, color: desktopColors.secondary }}>
              {t("rules.count", { total: page().rules.length, enabled: page().rules.filter((rule) => rule.enabled).length })}
            </Text>
          </View>
          {page().warnings.length > 0 ? (
            <View
              style={{
                borderWidth: 1,
                borderColor: desktopColors.outline,
                borderRadius: 10,
                backgroundColor: desktopColors.high,
                padding: 12,
                gap: 4,
              }}
            >
              <Text style={{ fontSize: 11, fontWeight: "600", color: desktopColors.warning }}>{t("rules.warnings")}</Text>
              {page().warnings.map((warning) => (
                <Text style={{ fontSize: 11, color: desktopColors.secondary }}>{warning}</Text>
              ))}
            </View>
          ) : null}
          <View
            style={{
              borderWidth: 1,
              borderColor: desktopColors.outline,
              borderRadius: 10,
              backgroundColor: desktopColors.high,
              padding: 12,
              gap: 4,
            }}
          >
            <Text style={{ fontSize: 11, fontWeight: "600", color: desktopColors.tertiary }}>{t("rules.note")}</Text>
            <Text style={{ fontSize: 11, color: desktopColors.secondary }}>
              {t("rules.noteBody")}
            </Text>
          </View>
        </Scroll>
      </View>
      ) : null}
    </View>
  );
}

export function RulesPage() {
  return embedded ? <DesktopRulesPage /> : <MobileRulesPage />;
}
