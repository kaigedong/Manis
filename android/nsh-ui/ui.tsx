import { Text, View } from "nsh";
import { embedded } from "./store";
import { desktopColors } from "./theme";

/** A button that follows the mobile or the desktop shell. */
export function Action(props: {
  label: string;
  onPress: () => void;
  primary?: boolean;
  disabled?: boolean;
}) {
  if (embedded) {
    return (
      <View
        onPress={props.disabled ? undefined : props.onPress}
        role="button"
        accessible
        disabled={props.disabled}
        style={{
          opacity: props.disabled ? 0.45 : 1,
          borderRadius: 8,
          borderWidth: props.primary ? 0 : 1,
          borderColor: desktopColors.outline,
          paddingHorizontal: 12,
          paddingVertical: 6,
          backgroundColor: props.primary ? desktopColors.primary : desktopColors.high,
        }}
      >
        <Text
          style={{
            fontSize: 12,
            color: props.primary ? desktopColors.onPrimary : desktopColors.text,
          }}
        >
          {props.label}
        </Text>
      </View>
    );
  }
  return (
    <View
      onPress={props.disabled ? undefined : props.onPress}
      role="button"
      accessible
      disabled={props.disabled}
      style={{
        opacity: props.disabled ? 0.45 : 1,
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

/** A titled card that follows the mobile or the desktop shell. */
export function Card(props: { title: string; action?: any; children: any }) {
  if (embedded) {
    return (
      <View
        style={{
          borderWidth: 1,
          borderColor: desktopColors.outline,
          borderRadius: 10,
          backgroundColor: desktopColors.high,
        }}
      >
        <View
          style={{
            flexDirection: "row",
            alignItems: "center",
            gap: 8,
            paddingHorizontal: 12,
            paddingVertical: 8,
            borderBottomWidth: 1,
            borderBottomColor: desktopColors.outline,
            backgroundColor: desktopColors.low,
          }}
        >
          <Text style={{ fontSize: 13, fontWeight: "600", flex: 1, color: desktopColors.text }}>
            {props.title}
          </Text>
          {props.action}
        </View>
        <View style={{ padding: 12, gap: 8 }}>{props.children}</View>
      </View>
    );
  }
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
      <Text role="header" style={{ fontSize: 18, fontWeight: "600" }}>
        {props.title}
      </Text>
      {props.children}
    </View>
  );
}

/** A label/value line that follows the mobile or the desktop shell. */
export function Info(props: { label: string; value: string }) {
  return (
    <View style={{ flexDirection: "row", alignItems: "flex-start", gap: 12, paddingVertical: embedded ? 2 : 4 }}>
      <Text
        style={{
          fontSize: embedded ? 12 : 13,
          color: embedded ? desktopColors.secondary : "#6b7280",
          width: 96,
        }}
      >
        {props.label}
      </Text>
      <Text style={{ fontSize: embedded ? 12 : 13, flex: 1, color: embedded ? desktopColors.text : undefined }}>
        {props.value}
      </Text>
    </View>
  );
}

/** A selectable chip that follows the mobile or the desktop shell. */
export function Chip(props: { label: string; active: boolean; onPress: () => void }) {
  if (embedded) {
    return (
      <View
        onPress={props.onPress}
        role="button"
        accessible
        accessibilityState={{ selected: props.active }}
        style={{
          borderRadius: 6,
          borderWidth: 1,
          borderColor: props.active ? desktopColors.primary : desktopColors.outline,
          paddingHorizontal: 10,
          paddingVertical: 5,
          backgroundColor: props.active ? "#E6F1EE" : desktopColors.high,
        }}
      >
        <Text style={{ fontSize: 12, color: props.active ? desktopColors.primary : desktopColors.text }}>
          {props.label}
        </Text>
      </View>
    );
  }
  return (
    <View
      onPress={props.onPress}
      role="button"
      accessible
      accessibilityState={{ selected: props.active }}
      style={{
        borderRadius: 10,
        paddingHorizontal: 14,
        paddingVertical: 8,
        backgroundColor: props.active ? "#7c3aed" : "#f3f4f6",
      }}
    >
      <Text style={{ fontSize: 13, color: props.active ? "#ffffff" : "#374151" }}>
        {props.label}
      </Text>
    </View>
  );
}

/** The route chain, shared by the mobile connection page and the desktop inspector. */
export function RouteChainCard(props: { node: string; mode: string }) {
  return (
    <Card title="路由链">
      <Text style={{ fontSize: embedded ? 12 : 13, color: embedded ? desktopColors.secondary : "#6b7280" }}>
        规则 → 策略组 → 节点
      </Text>
      <Info label="当前节点" value={props.node || "未选择"} />
      <Info label="模式" value={props.mode || "—"} />
    </Card>
  );
}

/** A node row shared by the mobile list and the desktop workspace. */
export function NodeRow(props: {
  name: string;
  current: boolean;
  delayMs: number | null;
  onPress: () => void;
}) {
  const delay = props.delayMs === null ? "未测速" : `${props.delayMs} 毫秒`;
  if (embedded) {
    return (
      <View
        onPress={props.onPress}
        role="button"
        accessible
        accessibilityLabel={`${props.name}${props.current ? "，当前" : ""}，${delay}`}
        style={{
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
          paddingHorizontal: 20,
          paddingVertical: 10,
          borderBottomWidth: 1,
          borderBottomColor: "#EDF2EF",
          backgroundColor: props.current ? "#E6F1EE" : "transparent",
        }}
      >
        <Text style={{ fontSize: 13, flex: 1, color: desktopColors.text }}>{props.name}</Text>
        {props.current ? (
          <Text style={{ fontSize: 11, color: desktopColors.primary }}>当前</Text>
        ) : null}
        <Text
          style={{
            fontSize: 11,
            color: desktopColors.tertiary,
            width: 60,
            textAlign: "right",
          }}
        >
          {props.delayMs === null ? "—" : `${props.delayMs} ms`}
        </Text>
      </View>
    );
  }
  return (
    <View
      onPress={props.onPress}
      role="button"
      accessible
      accessibilityLabel={`${props.name}${props.current ? "，当前" : ""}，${delay}`}
      style={{
        marginHorizontal: 16,
        paddingHorizontal: 14,
        paddingVertical: 14,
        backgroundColor: "#f3f4f6",
        borderBottomWidth: 1,
        borderBottomColor: "#e5e7eb",
        borderBottomLeftRadius: 14,
        borderBottomRightRadius: 14,
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
      }}
    >
      <Text style={{ fontSize: 15, flex: 1 }}>{props.name}</Text>
      {props.current ? <Text style={{ fontSize: 11, color: "#7c3aed" }}>当前</Text> : null}
      <Text style={{ fontSize: 12, color: "#6b7280" }}>
        {props.delayMs === null ? "—" : `${props.delayMs} ms`}
      </Text>
    </View>
  );
}

/** An enabled/disabled badge shared by the mobile rows and the desktop tables. */
export function ToggleBadge(props: {
  enabled: boolean;
  onOn: string;
  onOff: string;
  onPress: () => void;
}) {
  if (embedded) {
    return (
      <View
        onPress={props.onPress}
        role="button"
        accessible
        accessibilityLabel={props.enabled ? props.onOff : props.onOn}
        style={{
          borderRadius: 6,
          paddingHorizontal: 8,
          paddingVertical: 3,
          backgroundColor: props.enabled ? "#E6F1EE" : desktopColors.low,
        }}
      >
        <Text
          style={{
            fontSize: 11,
            color: props.enabled ? desktopColors.success : desktopColors.secondary,
          }}
        >
          {props.enabled ? props.onOn : props.onOff}
        </Text>
      </View>
    );
  }
  return (
    <View
      onPress={props.onPress}
      role="button"
      accessible
      accessibilityLabel={props.enabled ? props.onOff : props.onOn}
      style={{
        borderRadius: 8,
        paddingHorizontal: 8,
        paddingVertical: 5,
        backgroundColor: props.enabled ? "#dcfce7" : "#f3f4f6",
      }}
    >
      <Text style={{ fontSize: 12, color: props.enabled ? "#166534" : "#6b7280" }}>
        {props.enabled ? props.onOn : props.onOff}
      </Text>
    </View>
  );
}
