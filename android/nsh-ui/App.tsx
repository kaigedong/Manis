import { host, mount, signal, Text, View } from "nsh";
import { ConnectionPage } from "./connection";
import { NodesPage } from "./nodes";
import { RulesPage } from "./rules";
import { SettingsPage } from "./settings";
import { SubscriptionsPage } from "./subscriptions";
import { desktopColors } from "./theme";
import {
  bottomInset,
  embedded,
  root,
  tab,
  toast,
  topInset,
  type Tab,
} from "./store";

const tabs: Array<{ id: Tab; label: string; icon: string; hint: string }> = [
  { id: "connection", label: "连接", icon: "⌂", hint: "运行状态与出口" },
  { id: "nodes", label: "节点", icon: "☷", hint: "策略组与节点选择" },
  { id: "subscriptions", label: "订阅", icon: "↻", hint: "订阅、手动节点与策略组" },
  { id: "rules", label: "规则", icon: "✎", hint: "有序分流规则" },
  { id: "settings", label: "设置", icon: "⚙", hint: "外观、更新与内核" },
];

function Action(props: {
  label: string;
  onPress: () => void;
  primary?: boolean;
}) {
  return (
    <View
      onPress={props.onPress}
      role="button"
      accessible
      style={{
        borderRadius: 10,
        paddingHorizontal: 14,
        paddingVertical: 8,
        backgroundColor: props.primary ? "#7c3aed" : "#ede9fe",
      }}
    >
      <Text style={{ fontSize: 13, color: props.primary ? "#ffffff" : "#5b21b6" }}>
        {props.label}
      </Text>
    </View>
  );
}

function DesktopAction(props: {
  label: string;
  onPress: () => void;
  primary?: boolean;
}) {
  return (
    <View
      onPress={props.onPress}
      role="button"
      accessible
      style={{
        borderRadius: 8,
        paddingHorizontal: 12,
        paddingVertical: 6,
        backgroundColor: props.primary ? desktopColors.primary : desktopColors.low,
      }}
    >
      <Text
        style={{
          fontSize: 13,
          color: props.primary ? desktopColors.onPrimary : desktopColors.text,
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

function ApplyBar() {
  if (!root().pending) return null;
  return (
    <View
      style={{
        marginHorizontal: 12,
        marginTop: 8,
        borderRadius: 12,
        paddingHorizontal: 12,
        paddingVertical: 8,
        backgroundColor: "#ede9fe",
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
      }}
    >
      <Text style={{ fontSize: 13, flex: 1 }}>有未应用的更改</Text>
      <Action
        label="放弃"
        onPress={() => {
          void host.call("workspace.discard").catch(() => undefined);
        }}
      />
      <Action
        label="应用"
        primary
        onPress={() => {
          void host.call("workspace.apply").catch(() => undefined);
        }}
      />
    </View>
  );
}

function DesktopApplyBar() {
  if (!root().pending) return null;
  return (
    <View
      style={{
        marginHorizontal: 12,
        marginTop: 8,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: desktopColors.outline,
        paddingHorizontal: 12,
        paddingVertical: 8,
        backgroundColor: desktopColors.low,
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
      }}
    >
      <Text style={{ fontSize: 13, flex: 1, color: desktopColors.text }}>
        有未应用的更改
      </Text>
      <DesktopAction
        label="放弃"
        onPress={() => {
          void host.call("workspace.discard").catch(() => undefined);
        }}
      />
      <DesktopAction
        label="应用"
        primary
        onPress={() => {
          void host.call("workspace.apply").catch(() => undefined);
        }}
      />
    </View>
  );
}

function TabBar() {
  return (
    <View
      style={{
        flexDirection: "row",
        borderTopWidth: 1,
        borderTopColor: "#e5e7eb",
        backgroundColor: "#faf5ff",
        paddingTop: 8,
        paddingBottom: 12,
      }}
    >
      {tabs.map((entry) => (
        <View
          onPress={() => tab.set(entry.id)}
          role="button"
          accessible
          accessibilityLabel={entry.label}
          accessibilityState={{ selected: tab() === entry.id }}
          style={{ flex: 1, alignItems: "center", gap: 3, paddingVertical: 4 }}
        >
          <Text style={{ fontSize: 21, color: tab() === entry.id ? "#5b21b6" : "#6b7280" }}>
            {entry.icon}
          </Text>
          <Text
            style={{
              fontSize: 11,
              color: tab() === entry.id ? "#5b21b6" : "#6b7280",
              fontWeight: tab() === entry.id ? "600" : "400",
            }}
          >
            {entry.label}
          </Text>
        </View>
      ))}
    </View>
  );
}

function CurrentPage() {
  switch (tab()) {
    case "nodes":
      return <NodesPage />;
    case "subscriptions":
      return <SubscriptionsPage />;
    case "rules":
      return <RulesPage />;
    case "settings":
      return <SettingsPage />;
    default:
      return <ConnectionPage />;
  }
}

function SectionLabel() {
  const current = tabs.find((entry) => entry.id === tab()) ?? tabs[0];
  return (
    <View style={{ flexDirection: "column", gap: 2 }}>
      <Text style={{ fontSize: 17, fontWeight: "600", color: desktopColors.text }}>
        {current.label}
      </Text>
      <Text style={{ fontSize: 12, color: desktopColors.secondary }}>{current.hint}</Text>
    </View>
  );
}

function RuntimeIndicator() {
  const connection = root().connection ?? {};
  const active = connection.active === true;
  const label = active ? "已连接" : "未连接";
  return (
    <View style={{ flexDirection: "row", alignItems: "center", gap: 6 }}>
      <View
        style={{
          width: 8,
          height: 8,
          borderRadius: 4,
          backgroundColor: active ? desktopColors.success : desktopColors.warning,
        }}
      />
      <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
        {label}
        {connection.mode ? ` · ${connection.mode}` : ""}
      </Text>
    </View>
  );
}

function DesktopChrome() {
  const connection = root().connection ?? {};
  const active = connection.active === true;
  return (
    <View
      style={{
        height: 56,
        flexDirection: "row",
        alignItems: "center",
        gap: 16,
        paddingHorizontal: 20,
        backgroundColor: desktopColors.chrome,
        borderBottomWidth: 1,
        borderBottomColor: desktopColors.outline,
      }}
    >
      <SectionLabel />
      <View style={{ flex: 1 }} />
      <RuntimeIndicator />
      <DesktopAction
        label={active ? "断开" : "连接"}
        primary={!active}
        onPress={() => {
          void host.call("connection.toggle").catch(() => undefined);
        }}
      />
    </View>
  );
}

const sidebarCollapsed = signal(false);

function DesktopSidebar() {
  return (
    <View
      style={{
        width: sidebarCollapsed() ? 56 : 220,
        backgroundColor: desktopColors.low,
        borderRightWidth: 1,
        borderRightColor: desktopColors.outline,
        paddingHorizontal: 12,
        paddingVertical: 16,
        gap: 4,
      }}
    >
      <View
        style={{
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          paddingHorizontal: sidebarCollapsed() ? 0 : 8,
          paddingBottom: 16,
        }}
      >
        {sidebarCollapsed() ? null : (
          <View style={{ flex: 1, gap: 2 }}>
            <Text style={{ fontSize: 17, fontWeight: "700", color: desktopColors.text }}>
              Manis
            </Text>
            <Text style={{ fontSize: 12, color: desktopColors.tertiary }}>
              策略路由工作台
            </Text>
          </View>
        )}
        <View
          onPress={() => sidebarCollapsed.set(!sidebarCollapsed())}
          role="button"
          accessible
          accessibilityLabel={sidebarCollapsed() ? "展开侧栏" : "收起侧栏"}
          style={{ paddingHorizontal: 6, paddingVertical: 4 }}
        >
          <Text style={{ fontSize: 14, color: desktopColors.secondary }}>
            {sidebarCollapsed() ? "»" : "«"}
          </Text>
        </View>
      </View>
      {tabs.map((entry) => {
        const selected = tab() === entry.id;
        return (
          <View
            onPress={() => tab.set(entry.id)}
            role="button"
            accessible
            accessibilityLabel={entry.label}
            accessibilityState={{ selected }}
            style={{
              flexDirection: "row",
              alignItems: "center",
              gap: 10,
              paddingHorizontal: 10,
              paddingVertical: 8,
              borderRadius: 8,
              backgroundColor: selected ? desktopColors.primary : "transparent",
            }}
          >
            <Text
              style={{
                fontSize: 16,
                width: 20,
                color: selected ? desktopColors.onPrimary : desktopColors.secondary,
              }}
            >
              {entry.icon}
            </Text>
            {sidebarCollapsed() ? null : (
              <Text
                style={{
                  fontSize: 13,
                  color: selected ? desktopColors.onPrimary : desktopColors.text,
                  fontWeight: selected ? "600" : "400",
                }}
              >
                {entry.label}
              </Text>
            )}
          </View>
        );
      })}
      <View style={{ flex: 1 }} />
      {sidebarCollapsed() ? null : (
        <View style={{ paddingHorizontal: 8, gap: 4 }}>
          <RuntimeIndicator />
          <Text style={{ fontSize: 11, color: desktopColors.tertiary }}>
            {root().connection?.node || "未选择节点"}
          </Text>
        </View>
      )}
    </View>
  );
}

function DesktopStatusBar() {
  const connection = root().connection ?? {};
  const total = root().nodes?.global?.total ?? 0;
  return (
    <View
      style={{
        height: 30,
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        paddingHorizontal: 20,
        backgroundColor: desktopColors.chrome,
        borderTopWidth: 1,
        borderTopColor: desktopColors.outline,
      }}
    >
      <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
        {connection.summary || connection.mode || "等待内核状态"}
      </Text>
      <View style={{ flex: 1 }} />
      <Text style={{ fontSize: 12, color: desktopColors.tertiary }}>
        节点 {total} · NSH
      </Text>
    </View>
  );
}

function DesktopApp() {
  return (
    <View style={{ flex: 1, flexDirection: "row", backgroundColor: desktopColors.base }}>
      <DesktopSidebar />
      <View style={{ flex: 1, flexDirection: "column" }}>
        <DesktopChrome />
        {toast() ? (
          <View style={{ paddingHorizontal: 20, paddingTop: 8 }}>
            <Text style={{ fontSize: 12, color: desktopColors.secondary }}>{toast()}</Text>
          </View>
        ) : null}
        <DesktopApplyBar />
        <View
          style={{
            flex: 1,
            margin: 12,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: desktopColors.outline,
            backgroundColor: desktopColors.high,
            overflow: "hidden",
          }}
        >
          <View style={{ flex: 1 }}>
            <CurrentPage />
          </View>
        </View>
        <DesktopStatusBar />
      </View>
    </View>
  );
}

function MobileApp() {
  return (
    <View class="flex-1 bg-background" style={{ paddingTop: topInset }}>
      {toast() ? (
        <View style={{ marginHorizontal: 16, marginTop: 8 }}>
          <Text style={{ fontSize: 12, color: "#6b7280" }}>{toast()}</Text>
        </View>
      ) : null}
      <View style={{ flex: 1 }}>
        <CurrentPage />
      </View>
      {root().pending ? (
        <View
          style={{
            marginHorizontal: 12,
            marginTop: 8,
            borderRadius: 12,
            paddingHorizontal: 12,
            paddingVertical: 8,
            backgroundColor: "#ede9fe",
            flexDirection: "row",
            alignItems: "center",
            gap: 8,
          }}
        >
          <Text style={{ fontSize: 13, flex: 1 }}>有未应用的更改</Text>
          <View
            onPress={() => {
              void host.call("workspace.discard").catch(() => undefined);
            }}
            role="button"
            accessible
            style={{ borderRadius: 10, paddingHorizontal: 14, paddingVertical: 8, backgroundColor: "#ede9fe" }}
          >
            <Text style={{ fontSize: 13, color: "#5b21b6" }}>放弃</Text>
          </View>
          <View
            onPress={() => {
              void host.call("workspace.apply").catch(() => undefined);
            }}
            role="button"
            accessible
            style={{ borderRadius: 10, paddingHorizontal: 14, paddingVertical: 8, backgroundColor: "#7c3aed" }}
          >
            <Text style={{ fontSize: 13, color: "#ffffff" }}>应用</Text>
          </View>
        </View>
      ) : null}
      <View style={{ paddingBottom: bottomInset }}>
        <TabBar />
      </View>
    </View>
  );
}

function App() {
  return embedded ? <DesktopApp /> : <MobileApp />;
}

export function Root() {
  return <App />;
}

mount(Root);
