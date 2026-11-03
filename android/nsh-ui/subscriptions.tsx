import { computed, host, Scroll, signal, Text, View } from "nsh";
import { Input } from "nsh/base";
import { embedded, layoutWidth, root } from "./store";
import { t } from "./copy";
import { desktopColors } from "./theme";
import { Action, SubscriptionRow } from "./ui";

type SubscriptionItem = {
  index: number;
  name: string;
  displayUrl: string;
  enabled: boolean;
};

type ManualNode = {
  index: number;
  name: string;
  enabled: boolean;
};

type GroupItem = {
  index: number;
  name: string;
  strategy: string;
  subscriptionIndexes: number[];
  nodeIndexes: number[];
};

type Data = {
  subscriptions: SubscriptionItem[];
  nodes: ManualNode[];
  groups: GroupItem[];
};

type Reply = { data?: unknown; message?: string };

const empty: Data = { subscriptions: [], nodes: [], groups: [] };
const page = computed<Data>(() => (root().subscriptions ?? empty) as Data);
const message = signal("");
const busy = signal(false);
const adding = signal<"subscription" | "node" | "group" | null>(null);

const subName = signal("");
const subUrl = signal("");
const nodeLink = signal("");
const groupName = signal("");
const groupStrategy = signal("urltest");
const selectedSubscriptions = signal<number[]>([]);
const selectedNodes = signal<number[]>([]);

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

function toggleSubscription(entry: SubscriptionItem) {
  void invoke("subscriptions.toggle", {
    kind: "subscription",
    index: entry.index,
    enabled: !entry.enabled,
  });
}

function toggleNode(entry: ManualNode) {
  void invoke("subscriptions.toggle", {
    kind: "node",
    index: entry.index,
    enabled: !entry.enabled,
  });
}

function remove(kind: "subscription" | "node" | "group", index: number) {
  void invoke("subscriptions.delete", { kind, index });
}

function toggleIndex(list: number[], value: number): number[] {
  return list.includes(value) ? list.filter((item) => item !== value) : [...list, value].sort((a, b) => a - b);
}

async function addSubscription() {
  if (!subUrl().trim()) {
    message.set(t("subs.nameRequired"));
    return;
  }
  const reply = await invoke("subscriptions.add", {
    name: subName(),
    url: subUrl().trim(),
  });
  if (reply) {
    subName.set("");
    subUrl.set("");
    adding.set(null);
  }
}

async function addNode() {
  if (!nodeLink().trim().startsWith("vless://")) {
    message.set(t("subs.vlessRequired"));
    return;
  }
  const reply = await invoke("subscriptions.addNode", { link: nodeLink().trim() });
  if (reply) {
    nodeLink.set("");
    adding.set(null);
  }
}

async function addGroup() {
  if (!groupName().trim() || (selectedSubscriptions().length === 0 && selectedNodes().length === 0)) {
    message.set(t("subs.groupRequired"));
    return;
  }
  const reply = await invoke("subscriptions.addGroup", {
    name: groupName().trim(),
    strategy: groupStrategy(),
    subscriptionIndexes: selectedSubscriptions(),
    nodeIndexes: selectedNodes(),
  });
  if (reply) {
    groupName.set("");
    selectedSubscriptions.set([]);
    selectedNodes.set([]);
    adding.set(null);
  }
}

function Section(props: {
  title: string;
  hint?: string;
  action?: any;
  children: any;
}) {
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
      <View style={{ flexDirection: "row", alignItems: "center", gap: 10 }}>
        <Text role="header" style={{ fontSize: 18, fontWeight: "600", flex: 1 }}>
          {props.title}
        </Text>
        {props.action}
      </View>
      {props.hint ? (
        <Text style={{ fontSize: 12, color: "#6b7280" }}>{props.hint}</Text>
      ) : null}
      {props.children}
    </View>
  );
}

function ChoiceRow(props: {
  label: string;
  checked: boolean;
  onPress: () => void;
}) {
  return (
    <View
      onPress={props.onPress}
      role="button"
      accessible
      accessibilityLabel={`${props.label}${props.checked ? "，已选择" : "，未选择"}`}
      accessibilityState={{ checked: props.checked }}
      style={{ flexDirection: "row", alignItems: "center", gap: 10, paddingVertical: 6 }}
    >
      <View
        style={{
          width: 20,
          height: 20,
          borderRadius: 5,
          borderWidth: 1,
          borderColor: props.checked ? "#7c3aed" : "#9ca3af",
          backgroundColor: props.checked ? "#7c3aed" : "#ffffff",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        {props.checked ? (
          <Text style={{ fontSize: 13, color: "#ffffff" }}>✓</Text>
        ) : null}
      </View>
      <Text style={{ fontSize: 14 }}>{props.label}</Text>
    </View>
  );
}

function SubscriptionForm() {
  return (
    <DFormFrame title={t("subs.addSubscription")} hint={t("subs.addSubscriptionHint")}>
      <DInput value={subName()} onChange={(value) => subName.set(value)} placeholder={t("subs.nameOptional")} />
      <DInput value={subUrl()} onChange={(value) => subUrl.set(value)} placeholder="https://…" />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action label={t("subs.add")} primary onPress={() => void addSubscription()} />
        <Action label={t("subs.cancel")} onPress={() => adding.set(null)} />
      </View>
    </DFormFrame>
  );
}

function NodeForm() {
  return (
    <DFormFrame title={t("subs.addNode")} hint={t("subs.addNodeHint")}>
      <DInput value={nodeLink()} onChange={(value) => nodeLink.set(value)} placeholder="vless://…" multiline />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action label={t("subs.add")} primary onPress={() => void addNode()} />
        <Action label={t("subs.cancel")} onPress={() => adding.set(null)} />
      </View>
    </DFormFrame>
  );
}

function GroupForm() {
  return (
    <DFormFrame title={t("subs.addGroup")} hint={t("subs.addGroupHint")}>
      <DInput value={groupName()} onChange={(value) => groupName.set(value)} placeholder={t("subs.groupName")} />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action
          label={t("subs.manualSelect")}
          primary={groupStrategy() === "select"}
          onPress={() => groupStrategy.set("select")}
        />
        <Action
          label={t("subs.autoSelect")}
          primary={groupStrategy() === "urltest"}
          onPress={() => groupStrategy.set("urltest")}
        />
      </View>
      <Text style={{ fontSize: embedded ? 12 : 13, fontWeight: "600", color: embedded ? desktopColors.text : undefined }}>
        {t("subs.membersSubscriptions")}
      </Text>
      {page().subscriptions.map((entry) => (
        <DCheck
          label={entry.name}
          checked={selectedSubscriptions().includes(entry.index)}
          onPress={() => selectedSubscriptions.set(toggleIndex(selectedSubscriptions(), entry.index))}
        />
      ))}
      <Text style={{ fontSize: embedded ? 12 : 13, fontWeight: "600", color: embedded ? desktopColors.text : undefined }}>
        {t("subs.membersNodes")}
      </Text>
      {page().nodes.map((entry) => (
        <DCheck
          label={entry.name}
          checked={selectedNodes().includes(entry.index)}
          onPress={() => selectedNodes.set(toggleIndex(selectedNodes(), entry.index))}
        />
      ))}
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action label={t("subs.add")} primary onPress={() => void addGroup()} />
        <Action label={t("subs.cancel")} onPress={() => adding.set(null)} />
      </View>
    </DFormFrame>
  );
}

function MobileSubscriptionsPage() {
  return (
    <View class={embedded ? "flex-1 bg-background" : "flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"}>
      <Scroll style={{ flex: 1 }}>
        <View style={{ paddingHorizontal: 20, paddingTop: 18, paddingBottom: 24, gap: 12 }}>
          <Text style={{ fontSize: 28, fontWeight: "600" }}>{t("subs.title")}</Text>
          <Text style={{ fontSize: 13, color: "#6b7280" }}>
            {t("subs.hint")}
          </Text>
          <View style={{ flexDirection: "row", gap: 8, marginTop: 4 }}>
            <Action label={busy() ? t("nodes.refreshing") : t("subs.refresh")} onPress={() => void invoke("subscriptions.refresh")} />
            <Action label={t("subs.apply")} primary onPress={() => void invoke("subscriptions.apply")} />
          </View>
          {message() ? (
            <Text style={{ fontSize: 12, color: "#6b7280" }}>{message()}</Text>
          ) : null}

          {adding() === "subscription" ? <SubscriptionForm /> : null}
          {adding() === "node" ? <NodeForm /> : null}
          {adding() === "group" ? <GroupForm /> : null}

          <Section
            title={t("subs.subscriptions")}
            hint={page().subscriptions.length === 0 ? t("subs.noSubscriptions") : undefined}
            action={
              <Action
                label={t("subs.add")}
                onPress={() => {
                  adding.set(adding() === "subscription" ? null : "subscription");
                }}
              />
            }
          >
            {page().subscriptions.map((entry) => (
              <SubscriptionRow
                title={entry.name}
                detail={entry.displayUrl}
                enabled={entry.enabled}
                onToggle={() => toggleSubscription(entry)}
                onDelete={() => remove("subscription", entry.index)}
              />
            ))}
          </Section>

          <Section
            title={t("subs.nodes")}
            hint={page().nodes.length === 0 ? t("subs.noNodes") : undefined}
            action={
              <Action
                label={t("subs.add")}
                onPress={() => {
                  adding.set(adding() === "node" ? null : "node");
                }}
              />
            }
          >
            {page().nodes.map((entry) => (
              <SubscriptionRow
                title={entry.name}
                detail="VLESS"
                enabled={entry.enabled}
                onToggle={() => toggleNode(entry)}
                onDelete={() => remove("node", entry.index)}
              />
            ))}
          </Section>

          <Section
            title={t("subs.groups")}
            hint={
              page().groups.length === 0
                ? "全局组始终存在；再加一个自动测速组即可按延迟选最快节点。"
                : undefined
            }
            action={
              <Action
                label={t("subs.add")}
                onPress={() => {
                  adding.set(adding() === "group" ? null : "group");
                }}
              />
            }
          >
            {page().groups.map((group) => (
              <SubscriptionRow
                title={group.name}
                detail={`${group.strategy === "urltest" ? t("subs.autoSelect") : t("subs.manualSelect")} · ${group.subscriptionIndexes.length} 个订阅 / ${group.nodeIndexes.length} 个节点`}
                onDelete={() => remove("group", group.index)}
              />
            ))}
          </Section>

          <Text style={{ fontSize: 12, color: "#6b7280", marginHorizontal: 2 }}>
            {t("subs.applyNote")}
          </Text>
        </View>
      </Scroll>
    </View>
  );
}

function DCard(props: { title: string; count?: number; action?: any; children: any }) {
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
        {props.count === undefined ? null : (
          <Text style={{ fontSize: 11, color: desktopColors.tertiary }}>{props.count}</Text>
        )}
        {props.action}
      </View>
      <View>{props.children}</View>
    </View>
  );
}

function DInput(props: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  multiline?: boolean;
}) {
  return (
    <Input
      value={props.value}
      onChange={props.onChange}
      placeholder={props.placeholder}
      multiline={props.multiline}
      style={
        embedded
          ? {
              borderWidth: 1,
              borderColor: desktopColors.outlineStrong,
              borderRadius: 8,
              paddingHorizontal: 10,
              paddingVertical: 7,
              fontSize: 13,
              minHeight: props.multiline ? 72 : undefined,
            }
          : {
              borderWidth: 1,
              borderColor: "#d1d5db",
              borderRadius: 10,
              padding: 10,
              minHeight: props.multiline ? 88 : undefined,
            }
      }
    />
  );
}

function DCheck(props: { label: string; checked: boolean; onPress: () => void }) {
  return (
    <View
      onPress={props.onPress}
      role="button"
      accessible
      accessibilityLabel={props.label}
      accessibilityState={{ checked: props.checked }}
      style={{ flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 4 }}
    >
      <View
        style={{
          width: 16,
          height: 16,
          borderRadius: 4,
          borderWidth: 1,
          borderColor: props.checked ? desktopColors.primary : desktopColors.outlineStrong,
          backgroundColor: props.checked ? desktopColors.primary : desktopColors.high,
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        {props.checked ? <Text style={{ fontSize: 11, color: desktopColors.onPrimary }}>✓</Text> : null}
      </View>
      <Text style={{ fontSize: 12, color: desktopColors.text }}>{props.label}</Text>
    </View>
  );
}

function DFormFrame(props: { title: string; hint: string; children: any }) {
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
              borderColor: "#7c3aed",
              borderRadius: 16,
              padding: 14,
              gap: 10,
              backgroundColor: "#ffffff",
            }
      }
    >
      <Text role="header" style={{ fontSize: embedded ? 13 : 18, fontWeight: "600", color: embedded ? desktopColors.text : undefined }}>
        {props.title}
      </Text>
      <Text style={{ fontSize: embedded ? 11 : 12, color: embedded ? desktopColors.secondary : "#6b7280" }}>
        {props.hint}
      </Text>
      {props.children}
    </View>
  );
}

function DesktopSubscriptionsPage() {
  const enabledSubscriptions = page().subscriptions.filter((entry) => entry.enabled).length;
  const enabledNodes = page().nodes.filter((entry) => entry.enabled).length;
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
            {message() || t("subs.hint")}
          </Text>
          <Action label={busy() ? t("nodes.refreshing") : t("subs.refresh")} onPress={() => void invoke("subscriptions.refresh")} />
          <Action label={t("subs.apply")} primary onPress={() => void invoke("subscriptions.apply")} />
        </View>
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
          {adding() === "subscription" ? <SubscriptionForm /> : null}
          {adding() === "node" ? <NodeForm /> : null}
          {adding() === "group" ? <GroupForm /> : null}
          <DCard
            title={t("subs.subscriptions")}
            count={page().subscriptions.length}
            action={
              <Action
                label={t("subs.add")}
                onPress={() => adding.set(adding() === "subscription" ? null : "subscription")}
              />
            }
          >
            {page().subscriptions.map((entry) => (
              <SubscriptionRow
                title={entry.name}
                detail={entry.displayUrl}
                enabled={entry.enabled}
                onToggle={() => toggleSubscription(entry)}
                onDelete={() => remove("subscription", entry.index)}
              />
            ))}
            {page().subscriptions.length === 0 ? (
              <Text style={{ fontSize: 12, color: desktopColors.tertiary, padding: 12 }}>{t("subs.noSubscriptions")}</Text>
            ) : null}
          </DCard>
          <DCard
            title={t("subs.nodes")}
            count={page().nodes.length}
            action={
              <Action
                label={t("subs.add")}
                onPress={() => adding.set(adding() === "node" ? null : "node")}
              />
            }
          >
            {page().nodes.map((entry) => (
              <SubscriptionRow
                title={entry.name}
                detail="VLESS"
                enabled={entry.enabled}
                onToggle={() => toggleNode(entry)}
                onDelete={() => remove("node", entry.index)}
              />
            ))}
            {page().nodes.length === 0 ? (
              <Text style={{ fontSize: 12, color: desktopColors.tertiary, padding: 12 }}>{t("subs.noNodes")}</Text>
            ) : null}
          </DCard>
          <DCard
            title={t("subs.groups")}
            count={page().groups.length}
            action={
              <Action
                label={t("subs.add")}
                onPress={() => adding.set(adding() === "group" ? null : "group")}
              />
            }
          >
            {page().groups.map((group) => (
              <SubscriptionRow
                title={group.name}
                detail={`${group.strategy === "urltest" ? t("subs.autoSelect") : t("subs.manualSelect")} · ${group.subscriptionIndexes.length} 个订阅 / ${group.nodeIndexes.length} 个节点`}
                onDelete={() => remove("group", group.index)}
              />
            ))}
            {page().groups.length === 0 ? (
              <Text style={{ fontSize: 12, color: desktopColors.tertiary, padding: 12 }}>
                {t("subs.noGroups")}
              </Text>
            ) : null}
          </DCard>
        </Scroll>
      </View>
      {layoutWidth() === 0 || layoutWidth() >= 1100 ? (
      <View style={{ width: 280, borderLeftWidth: 1, borderLeftColor: desktopColors.outline, backgroundColor: desktopColors.low }}>
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
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
            <Text style={{ fontSize: 11, fontWeight: "600", color: desktopColors.tertiary }}>{t("subs.summary")}</Text>
            <Text style={{ fontSize: 12, color: desktopColors.text }}>
              {t("subs.subscriptions")} {page().subscriptions.length} · {t("subs.enabledCount", { count: enabledSubscriptions })}
            </Text>
            <Text style={{ fontSize: 12, color: desktopColors.text }}>
              {t("subs.nodes")} {page().nodes.length} · {t("subs.enabledCount", { count: enabledNodes })}
            </Text>
            <Text style={{ fontSize: 12, color: desktopColors.text }}>{t("subs.groups")} {page().groups.length}</Text>
          </View>
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
            <Text style={{ fontSize: 11, fontWeight: "600", color: desktopColors.tertiary }}>{t("subs.applyTitle")}</Text>
            <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
              {t("subs.applyNote")}
            </Text>
          </View>
        </Scroll>
      </View>
      ) : null}
    </View>
  );
}

export function SubscriptionsPage() {
  return embedded ? <DesktopSubscriptionsPage /> : <MobileSubscriptionsPage />;
}
