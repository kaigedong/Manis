import { host, mount, Text, View } from "nsh";
import { ConnectionPage } from "./connection";
import { NodesPage } from "./nodes";
import { RulesPage } from "./rules";
import { SettingsPage } from "./settings";
import { SubscriptionsPage } from "./subscriptions";
import { bottomInset, root, tab, toast, topInset, type Tab } from "./store";

const tabs: Array<{ id: Tab; label: string; icon: string }> = [
  { id: "connection", label: "连接", icon: "⌂" },
  { id: "nodes", label: "节点", icon: "☷" },
  { id: "subscriptions", label: "订阅", icon: "↻" },
  { id: "rules", label: "规则", icon: "✎" },
  { id: "settings", label: "设置", icon: "⚙" },
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
          key={entry.id}
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

function App() {
  return (
    <View class="flex-1 bg-background" style={{ paddingTop: topInset }}>
      {toast() ? (
        <View style={{ marginHorizontal: 16, marginTop: 8 }}>
          <Text style={{ fontSize: 12, color: "#6b7280" }}>{toast()}</Text>
        </View>
      ) : null}
      <View style={{ flex: 1 }}>
        {tab() === "nodes" ? (
          <NodesPage />
        ) : tab() === "subscriptions" ? (
          <SubscriptionsPage />
        ) : tab() === "rules" ? (
          <RulesPage />
        ) : tab() === "settings" ? (
          <SettingsPage />
        ) : (
          <ConnectionPage />
        )}
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

export function Root() {
  return <App />;
}

mount(Root);
