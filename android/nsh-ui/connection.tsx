import { computed, host, Scroll, signal, Text, View } from "nsh";
import { embedded, layoutWidth, root, tab } from "./store";
import { t } from "./copy";
import { desktopColors } from "./theme";
import { Action, Card, Chip, Info, RouteChainCard } from "./ui";

type Data = {
  active: boolean;
  node: string;
  mode: string;
  summary: string;
  error: string;
  unreachable: boolean;
  warnings: string[];
  warningCount: number;
  hasSources: boolean;
};

const empty: Data = {
  active: false,
  node: "",
  mode: "rule",
  summary: "",
  error: "",
  unreachable: false,
  warnings: [],
  warningCount: 0,
  hasSources: false,
};
const page = computed<Data>(() => (root().connection ?? empty) as Data);
const message = signal("");
const busy = signal(false);

async function invoke(command: string, payload: Record<string, unknown> = {}) {
  if (!host.available()) {
    message.set("独立运行模式：没有宿主可执行");
    return null;
  }
  busy.set(true);
  try {
    const reply = (await host.call(command, payload)) as
      | { data?: Data; message?: string }
      | null;
    if (reply?.data) root.set(reply.data as any);
    message.set(reply?.message ?? "");
    return reply;
  } catch (error) {
    message.set(error instanceof Error ? error.message : "操作失败");
    return null;
  } finally {
    busy.set(false);
  }
}

const modes = [
  ["rule", "conn.mode.rule"],
  ["global", "conn.mode.global"],
  ["direct", "conn.mode.direct"],
] as const;

function MobileConnectionPage() {
  return (
    <View class={embedded ? "flex-1 bg-background" : "flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"}>
      <Scroll style={{ flex: 1 }}>
        <View style={{ paddingHorizontal: 20, paddingTop: 18, paddingBottom: 24, gap: 14 }}>
          <Text style={{ fontSize: 28, fontWeight: "600" }}>{t("conn.title")}</Text>
          <Text style={{ fontSize: 13, color: "#6b7280" }}>
            {t("conn.hint")}
          </Text>

          <View
            style={{
              borderWidth: 1,
              borderColor: "#e5e7eb",
              borderRadius: 18,
              padding: 18,
              gap: 10,
              backgroundColor: page().active ? "#f5f3ff" : "#ffffff",
            }}
          >
            <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
              <View style={{ flex: 1, gap: 4 }}>
                <Text style={{ fontSize: 22, fontWeight: "600" }}>
                  {page().active ? t("conn.connected") : t("conn.disconnected")}
                </Text>
                <Text style={{ fontSize: 13, color: "#6b7280" }}>
                  {page().active
                    ? page().node || t("conn.loadingNode")
                    : t("conn.openVpn")}
                </Text>
              </View>
              <Action
                label={page().active ? t("chrome.disconnect") : t("chrome.connect")}
                primary={!page().active}
                onPress={() =>
                  void invoke("connection.toggle", { enabled: !page().active })
                }
              />
            </View>
            {page().active ? (
              <View
                style={{
                  borderTopWidth: 1,
                  borderTopColor: "#e9d5ff",
                  paddingTop: 10,
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 8,
                }}
              >
                <Text style={{ fontSize: 13, color: "#6b7280" }}>{t("conn.exit")}</Text>
                <Text style={{ fontSize: 14, flex: 1 }}>{page().node || "—"}</Text>
                <Action label={t("conn.switch")} onPress={() => tab.set("nodes")} />
              </View>
            ) : null}
          </View>

          {message() ? (
            <Text style={{ fontSize: 12, color: "#6b7280" }}>{message()}</Text>
          ) : null}

          <RouteChainCard node={page().node} mode={page().mode} />

          <Text style={{ fontSize: 16, fontWeight: "600" }}>{t("conn.routeMode")}</Text>
          <View style={{ flexDirection: "row", gap: 8 }}>
            {modes.map(([value, label]) => (
              <View
                onPress={() => void invoke("connection.mode", { mode: value })}
                role="button"
                accessible
                accessibilityState={{ selected: page().mode === value }}
                style={{
                  borderRadius: 10,
                  paddingHorizontal: 14,
                  paddingVertical: 8,
                  backgroundColor: page().mode === value ? "#7c3aed" : "#f3f4f6",
                }}
              >
                <Text style={{ fontSize: 13, color: page().mode === value ? "#ffffff" : "#374151" }}>
                  {t(label)}
                </Text>
              </View>
            ))}
          </View>

          {page().error ? (
            <View
              style={{
                borderWidth: 1,
                borderColor: "#fecaca",
                borderRadius: 12,
                padding: 12,
                backgroundColor: "#fef2f2",
              }}
            >
              <Text style={{ fontSize: 13, color: "#b91c1c" }}>{page().error}</Text>
            </View>
          ) : null}

          {page().summary ? (
            <Text style={{ fontSize: 12, color: "#6b7280" }}>{page().summary}</Text>
          ) : null}

          {page().unreachable ? (
            <View
              style={{
                borderWidth: 1,
                borderColor: "#fecaca",
                borderRadius: 12,
                padding: 12,
                backgroundColor: "#fef2f2",
                gap: 6,
              }}
            >
              <Text style={{ fontSize: 15, fontWeight: "600", color: "#b91c1c" }}>
                所有节点都连不上
              </Text>
              <Text style={{ fontSize: 12, color: "#6b7280" }}>
                {t("conn.unreachableHint")}
              </Text>
            </View>
          ) : null}

          {page().warnings.length > 0 ? (
            <View
              style={{
                borderWidth: 1,
                borderColor: "#e5e7eb",
                borderRadius: 12,
                padding: 12,
                gap: 6,
              }}
            >
              <Text style={{ fontSize: 15, fontWeight: "600" }}>{t("conn.warnings")}</Text>
              {page().warnings.map((warning, index) => (
                <Text style={{ fontSize: 12, color: "#6b7280" }}>
                  {warning}
                </Text>
              ))}
              {page().warningCount > page().warnings.length ? (
                <Text style={{ fontSize: 12, color: "#6b7280" }}>
                  {t("conn.moreWarnings", { count: page().warningCount - page().warnings.length })}
                </Text>
              ) : null}
            </View>
          ) : null}

          <View style={{ flexDirection: "row", gap: 8 }}>
            <Action label={t("conn.reload")} onPress={() => void invoke("connection.reload")} />
            {page().active ? (
              <Action label="断开" onPress={() => void invoke("connection.toggle", { enabled: false })} />
            ) : null}
          </View>

          {!page().hasSources ? (
            <Text style={{ fontSize: 12, color: "#b45309" }}>
              {t("conn.noSources")}
            </Text>
          ) : null}
        </View>
      </Scroll>
    </View>
  );
}

function DesktopConnectionPage() {
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
            {message() || page().summary || t("conn.hint")}
          </Text>
          <Action label={t("conn.reload")} onPress={() => void invoke("connection.reload")} />
          <Action
            label={page().active ? t("chrome.disconnect") : t("chrome.connect")}
            primary={!page().active}
            onPress={() => void invoke("connection.toggle", { enabled: !page().active })}
          />
        </View>
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
          <View
            style={{
              borderWidth: 1,
              borderColor: page().active ? desktopColors.primary : desktopColors.outline,
              borderRadius: 10,
              padding: 16,
              gap: 10,
              backgroundColor: page().active ? "#F7FBFA" : desktopColors.high,
            }}
          >
            <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
              <View style={{ flex: 1, gap: 4 }}>
                <Text style={{ fontSize: 20, fontWeight: "600", color: desktopColors.text }}>
                  {page().active ? t("conn.connected") : t("conn.disconnected")}
                </Text>
                <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
                  {page().active ? page().node || t("conn.loadingNode") : t("conn.openVpn")}
                </Text>
              </View>
              <View
                style={{
                  width: 10,
                  height: 10,
                  borderRadius: 5,
                  backgroundColor: page().active ? desktopColors.success : desktopColors.warning,
                }}
              />
            </View>
            {page().active ? (
              <View
                style={{
                  borderTopWidth: 1,
                  borderTopColor: desktopColors.outline,
                  paddingTop: 10,
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 8,
                }}
              >
                <Text style={{ fontSize: 12, color: desktopColors.secondary }}>{t("conn.exit")}</Text>
                <Text style={{ fontSize: 13, flex: 1, color: desktopColors.text }}>
                  {page().node || "—"}
                </Text>
                <Action label={t("conn.switch")} onPress={() => tab.set("nodes")} />
              </View>
            ) : null}
          </View>

          <Card title={t("conn.routeMode")}>
            <View style={{ flexDirection: "row", gap: 8 }}>
              {modes.map(([value, label]) => (
                <Chip
                  label={t(label)}
                  active={page().mode === value}
                  onPress={() => void invoke("connection.mode", { mode: value })}
                />
              ))}
            </View>
          </Card>

          {page().error ? (
            <View
              style={{
                borderWidth: 1,
                borderColor: desktopColors.error,
                borderRadius: 10,
                padding: 12,
                backgroundColor: "#FDF2F1",
              }}
            >
              <Text style={{ fontSize: 12, color: desktopColors.error }}>{page().error}</Text>
            </View>
          ) : null}

          {page().unreachable ? (
            <Card title={t("conn.unreachable")}>
              <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
                {t("conn.unreachableHint")}
              </Text>
            </Card>
          ) : null}

          {page().warnings.length > 0 ? (
            <Card title={t("conn.warnings")}>
              {page().warnings.map((warning) => (
                <Text style={{ fontSize: 12, color: desktopColors.secondary }}>{warning}</Text>
              ))}
              {page().warningCount > page().warnings.length ? (
                <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
                  {t("conn.moreWarnings", { count: page().warningCount - page().warnings.length })}
                </Text>
              ) : null}
            </Card>
          ) : null}

          {!page().hasSources ? (
            <Text style={{ fontSize: 12, color: desktopColors.warning }}>
              {t("conn.noSources")}
            </Text>
          ) : null}
        </Scroll>
      </View>
      {layoutWidth() === 0 || layoutWidth() >= 1100 ? (
      <View
        style={{
          width: 300,
          borderLeftWidth: 1,
          borderLeftColor: desktopColors.outline,
          backgroundColor: desktopColors.low,
        }}
      >
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
          <Card title={t("conn.overview")}>
            <Info label="状态" value={page().active ? t("conn.connected") : t("conn.disconnected")} />
            <Info label={t("conn.summary")} value={page().summary || "—"} />
          </Card>
          <RouteChainCard node={page().node} mode={page().mode} />
          <Card title={t("conn.tips")}>
            <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
              {t("conn.tipsBody")}
            </Text>
          </Card>
        </Scroll>
      </View>
      ) : null}
    </View>
  );
}

export function ConnectionPage() {
  return embedded ? <DesktopConnectionPage /> : <MobileConnectionPage />;
}
