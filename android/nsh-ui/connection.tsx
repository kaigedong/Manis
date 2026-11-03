import { computed, host, Scroll, signal, Text, View } from "nsh";
import { embedded, root, tab } from "./store";
import { desktopColors } from "./theme";
import { Action, Card, Chip, Info } from "./ui";

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
  ["rule", "规则"],
  ["global", "全局"],
  ["direct", "直连"],
] as const;

function MobileConnectionPage() {
  return (
    <View class={embedded ? "flex-1 bg-background" : "flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"}>
      <Scroll style={{ flex: 1 }}>
        <View style={{ paddingHorizontal: 20, paddingTop: 18, paddingBottom: 24, gap: 14 }}>
          <Text style={{ fontSize: 28, fontWeight: "600" }}>连接</Text>
          <Text style={{ fontSize: 13, color: "#6b7280" }}>
            手机与桌面共用同一份规则和渲染器。
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
                  {page().active ? "已连接" : "未连接"}
                </Text>
                <Text style={{ fontSize: 13, color: "#6b7280" }}>
                  {page().active
                    ? page().node || "正在加载节点"
                    : "打开开关建立 VPN"}
                </Text>
              </View>
              <Action
                label={page().active ? "断开" : "连接"}
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
                <Text style={{ fontSize: 13, color: "#6b7280" }}>出口</Text>
                <Text style={{ fontSize: 14, flex: 1 }}>{page().node || "未选择"}</Text>
                <Action label="切换" onPress={() => tab.set("nodes")} />
              </View>
            ) : null}
          </View>

          {message() ? (
            <Text style={{ fontSize: 12, color: "#6b7280" }}>{message()}</Text>
          ) : null}

          <Text style={{ fontSize: 16, fontWeight: "600" }}>路由模式</Text>
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
                  {label}
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
                手机网络到订阅节点的链路超时，流量现在无法出去。先换一个 Wi-Fi 或移动数据，或到节点页点「选最快」重试；仍然失败时，去设置里切换「节点域名解析」。
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
              <Text style={{ fontSize: 15, fontWeight: "600" }}>配置提示</Text>
              {page().warnings.map((warning, index) => (
                <Text style={{ fontSize: 12, color: "#6b7280" }}>
                  {warning}
                </Text>
              ))}
              {page().warningCount > page().warnings.length ? (
                <Text style={{ fontSize: 12, color: "#6b7280" }}>
                  以及另外 {page().warningCount - page().warnings.length} 条
                </Text>
              ) : null}
            </View>
          ) : null}

          <View style={{ flexDirection: "row", gap: 8 }}>
            <Action label="重新载入配置" onPress={() => void invoke("connection.reload")} />
            {page().active ? (
              <Action label="断开" onPress={() => void invoke("connection.toggle", { enabled: false })} />
            ) : null}
          </View>

          {!page().hasSources ? (
            <Text style={{ fontSize: 12, color: "#b45309" }}>
              还没有启用的订阅或节点；先在「订阅」里添加一个。
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
            {message() || page().summary || "手机与桌面共用同一份规则和渲染器。"}
          </Text>
          <Action label="重新载入配置" onPress={() => void invoke("connection.reload")} />
          <Action
            label={page().active ? "断开" : "连接"}
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
                  {page().active ? "已连接" : "未连接"}
                </Text>
                <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
                  {page().active ? page().node || "正在加载节点" : "打开开关建立 VPN"}
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
                <Text style={{ fontSize: 12, color: desktopColors.secondary }}>出口</Text>
                <Text style={{ fontSize: 13, flex: 1, color: desktopColors.text }}>
                  {page().node || "未选择"}
                </Text>
                <Action label="切换" onPress={() => tab.set("nodes")} />
              </View>
            ) : null}
          </View>

          <Card title="路由模式">
            <View style={{ flexDirection: "row", gap: 8 }}>
              {modes.map(([value, label]) => (
                <Chip
                  label={label}
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
            <Card title="所有节点都连不上">
              <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
                到订阅节点的链路超时，流量现在无法出去。先换一个网络，或到节点页点「刷新」重试；仍然失败时，去设置里切换「节点域名解析」。
              </Text>
            </Card>
          ) : null}

          {page().warnings.length > 0 ? (
            <Card title="配置提示">
              {page().warnings.map((warning) => (
                <Text style={{ fontSize: 12, color: desktopColors.secondary }}>{warning}</Text>
              ))}
              {page().warningCount > page().warnings.length ? (
                <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
                  以及另外 {page().warningCount - page().warnings.length} 条
                </Text>
              ) : null}
            </Card>
          ) : null}

          {!page().hasSources ? (
            <Text style={{ fontSize: 12, color: desktopColors.warning }}>
              还没有启用的订阅或节点；先在「订阅」里添加一个。
            </Text>
          ) : null}
        </Scroll>
      </View>
      <View
        style={{
          width: 300,
          borderLeftWidth: 1,
          borderLeftColor: desktopColors.outline,
          backgroundColor: desktopColors.low,
        }}
      >
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
          <Card title="概览">
            <Info label="状态" value={page().active ? "已连接" : "未连接"} />
            <Info label="出口" value={page().node || "未选择"} />
            <Info label="路由模式" value={page().mode} />
            <Info label="摘要" value={page().summary || "—"} />
          </Card>
          <Card title="提示">
            <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
              规则、策略组与节点来自同一份已保存工作区；切换节点后立即生效。
            </Text>
          </Card>
        </Scroll>
      </View>
    </View>
  );
}

export function ConnectionPage() {
  return embedded ? <DesktopConnectionPage /> : <MobileConnectionPage />;
}
