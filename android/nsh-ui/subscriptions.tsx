import { computed, host, Scroll, signal, Text, View } from "nsh";
import { Input } from "nsh/base";
import { embedded, root } from "./store";
import { desktopColors } from "./theme";
import { SubscriptionRow } from "./ui";

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
    message.set("独立运行模式：没有宿主可执行");
    return null;
  }
  busy.set(true);
  try {
    const reply = (await host.call(command, payload)) as Reply | null;
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
    message.set("订阅地址不能为空");
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
    message.set("请输入 vless:// 分享链接");
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
    message.set("策略组需要名称和至少一个成员");
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
    <Section title="添加订阅" hint="粘贴 Clash 格式订阅地址；添加后会立即下载。">
      <Input
        value={subName()}
        onChange={(value) => subName.set(value)}
        placeholder="名称（可选）"
        style={{ borderWidth: 1, borderColor: "#d1d5db", borderRadius: 10, padding: 10 }}
      />
      <Input
        value={subUrl()}
        onChange={(value) => subUrl.set(value)}
        placeholder="https://…"
        style={{ borderWidth: 1, borderColor: "#d1d5db", borderRadius: 10, padding: 10 }}
      />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action label="添加" primary onPress={() => void addSubscription()} />
        <Action label="取消" onPress={() => adding.set(null)} />
      </View>
    </Section>
  );
}

function NodeForm() {
  return (
    <Section title="添加 VLESS 节点" hint="支持 TLS / WebSocket / Reality 分享链接。">
      <Input
        value={nodeLink()}
        onChange={(value) => nodeLink.set(value)}
        placeholder="vless://…"
        multiline
        style={{ borderWidth: 1, borderColor: "#d1d5db", borderRadius: 10, padding: 10, minHeight: 88 }}
      />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action label="添加" primary onPress={() => void addNode()} />
        <Action label="取消" onPress={() => adding.set(null)} />
      </View>
    </Section>
  );
}

function GroupForm() {
  return (
    <Section title="添加策略组" hint="选择成员；自动测速组会按延迟选最快节点。">
      <Input
        value={groupName()}
        onChange={(value) => groupName.set(value)}
        placeholder="组名"
        style={{ borderWidth: 1, borderColor: "#d1d5db", borderRadius: 10, padding: 10 }}
      />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action
          label="手动选择"
          primary={groupStrategy() === "select"}
          onPress={() => groupStrategy.set("select")}
        />
        <Action
          label="自动测速"
          primary={groupStrategy() === "urltest"}
          onPress={() => groupStrategy.set("urltest")}
        />
      </View>
      <Text style={{ fontSize: 13, fontWeight: "600", marginTop: 4 }}>订阅成员</Text>
      {page().subscriptions.map((entry) => (
        <ChoiceRow
          label={entry.name}
          checked={selectedSubscriptions().includes(entry.index)}
          onPress={() =>
            selectedSubscriptions.set(
              toggleIndex(selectedSubscriptions(), entry.index),
            )
          }
        />
      ))}
      <Text style={{ fontSize: 13, fontWeight: "600", marginTop: 4 }}>手动节点成员</Text>
      {page().nodes.map((entry) => (
        <ChoiceRow
          label={entry.name}
          checked={selectedNodes().includes(entry.index)}
          onPress={() => selectedNodes.set(toggleIndex(selectedNodes(), entry.index))}
        />
      ))}
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action label="添加" primary onPress={() => void addGroup()} />
        <Action label="取消" onPress={() => adding.set(null)} />
      </View>
    </Section>
  );
}

function MobileSubscriptionsPage() {
  return (
    <View class={embedded ? "flex-1 bg-background" : "flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"}>
      <Scroll style={{ flex: 1 }}>
        <View style={{ paddingHorizontal: 20, paddingTop: 18, paddingBottom: 24, gap: 12 }}>
          <Text style={{ fontSize: 28, fontWeight: "600" }}>订阅</Text>
          <Text style={{ fontSize: 13, color: "#6b7280" }}>
            订阅、手动节点和策略组都会进入同一份内核配置。
          </Text>
          <View style={{ flexDirection: "row", gap: 8, marginTop: 4 }}>
            <Action label={busy() ? "刷新中…" : "刷新订阅"} onPress={() => void invoke("subscriptions.refresh")} />
            <Action label="应用" primary onPress={() => void invoke("subscriptions.apply")} />
          </View>
          {message() ? (
            <Text style={{ fontSize: 12, color: "#6b7280" }}>{message()}</Text>
          ) : null}

          {adding() === "subscription" ? <SubscriptionForm /> : null}
          {adding() === "node" ? <NodeForm /> : null}
          {adding() === "group" ? <GroupForm /> : null}

          <Section
            title="订阅"
            hint={page().subscriptions.length === 0 ? "还没有订阅。" : undefined}
            action={
              <Action
                label="添加"
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
            title="手动节点"
            hint={page().nodes.length === 0 ? "还没有手动节点。" : undefined}
            action={
              <Action
                label="添加"
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
            title="策略组"
            hint={
              page().groups.length === 0
                ? "全局组始终存在；再加一个自动测速组即可按延迟选最快节点。"
                : undefined
            }
            action={
              <Action
                label="添加"
                onPress={() => {
                  adding.set(adding() === "group" ? null : "group");
                }}
              />
            }
          >
            {page().groups.map((group) => (
              <SubscriptionRow
                title={group.name}
                detail={`${group.strategy === "urltest" ? "自动测速" : "手动选择"} · ${group.subscriptionIndexes.length} 个订阅 / ${group.nodeIndexes.length} 个节点`}
                onDelete={() => remove("group", group.index)}
              />
            ))}
          </Section>

          <Text style={{ fontSize: 12, color: "#6b7280", marginHorizontal: 2 }}>
            修改会先保存；点「应用」后内核重启并使用已下载的缓存，VPN 接口不会断开。
          </Text>
        </View>
      </Scroll>
    </View>
  );
}

function DButton(props: {
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
      style={{
        borderWidth: 1,
        borderColor: desktopColors.outlineStrong,
        borderRadius: 8,
        paddingHorizontal: 10,
        paddingVertical: 7,
        fontSize: 13,
        minHeight: props.multiline ? 72 : undefined,
      }}
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
      style={{
        borderWidth: 1,
        borderColor: desktopColors.primary,
        borderRadius: 10,
        padding: 12,
        gap: 8,
        backgroundColor: "#F7FBFA",
      }}
    >
      <Text style={{ fontSize: 13, fontWeight: "600", color: desktopColors.text }}>{props.title}</Text>
      <Text style={{ fontSize: 11, color: desktopColors.secondary }}>{props.hint}</Text>
      {props.children}
    </View>
  );
}

function DSubscriptionForm() {
  return (
    <DFormFrame title="添加订阅" hint="粘贴 Clash 格式订阅地址；添加后会立即下载。">
      <DInput value={subName()} onChange={(value) => subName.set(value)} placeholder="名称（可选）" />
      <DInput value={subUrl()} onChange={(value) => subUrl.set(value)} placeholder="https://…" />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <DButton label="添加" primary onPress={() => void addSubscription()} />
        <DButton label="取消" onPress={() => adding.set(null)} />
      </View>
    </DFormFrame>
  );
}

function DNodeForm() {
  return (
    <DFormFrame title="添加 VLESS 节点" hint="支持 TLS / WebSocket / Reality 分享链接。">
      <DInput value={nodeLink()} onChange={(value) => nodeLink.set(value)} placeholder="vless://…" multiline />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <DButton label="添加" primary onPress={() => void addNode()} />
        <DButton label="取消" onPress={() => adding.set(null)} />
      </View>
    </DFormFrame>
  );
}

function DGroupForm() {
  return (
    <DFormFrame title="添加策略组" hint="选择成员；自动测速组会按延迟选最快节点。">
      <DInput value={groupName()} onChange={(value) => groupName.set(value)} placeholder="组名" />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <DButton
          label="手动选择"
          primary={groupStrategy() === "select"}
          onPress={() => groupStrategy.set("select")}
        />
        <DButton
          label="自动测速"
          primary={groupStrategy() === "urltest"}
          onPress={() => groupStrategy.set("urltest")}
        />
      </View>
      <Text style={{ fontSize: 12, fontWeight: "600", color: desktopColors.text }}>订阅成员</Text>
      {page().subscriptions.map((entry) => (
        <DCheck
          label={entry.name}
          checked={selectedSubscriptions().includes(entry.index)}
          onPress={() => selectedSubscriptions.set(toggleIndex(selectedSubscriptions(), entry.index))}
        />
      ))}
      <Text style={{ fontSize: 12, fontWeight: "600", color: desktopColors.text }}>手动节点成员</Text>
      {page().nodes.map((entry) => (
        <DCheck
          label={entry.name}
          checked={selectedNodes().includes(entry.index)}
          onPress={() => selectedNodes.set(toggleIndex(selectedNodes(), entry.index))}
        />
      ))}
      <View style={{ flexDirection: "row", gap: 8 }}>
        <DButton label="添加" primary onPress={() => void addGroup()} />
        <DButton label="取消" onPress={() => adding.set(null)} />
      </View>
    </DFormFrame>
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
            {message() || "订阅、手动节点和策略组都会进入同一份内核配置。"}
          </Text>
          <DButton label={busy() ? "刷新中…" : "刷新订阅"} onPress={() => void invoke("subscriptions.refresh")} />
          <DButton label="应用" primary onPress={() => void invoke("subscriptions.apply")} />
        </View>
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
          {adding() === "subscription" ? <DSubscriptionForm /> : null}
          {adding() === "node" ? <DNodeForm /> : null}
          {adding() === "group" ? <DGroupForm /> : null}
          <DCard
            title="订阅"
            count={page().subscriptions.length}
            action={
              <DButton
                label="添加"
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
              <Text style={{ fontSize: 12, color: desktopColors.tertiary, padding: 12 }}>还没有订阅。</Text>
            ) : null}
          </DCard>
          <DCard
            title="手动节点"
            count={page().nodes.length}
            action={
              <DButton
                label="添加"
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
              <Text style={{ fontSize: 12, color: desktopColors.tertiary, padding: 12 }}>还没有手动节点。</Text>
            ) : null}
          </DCard>
          <DCard
            title="策略组"
            count={page().groups.length}
            action={
              <DButton
                label="添加"
                onPress={() => adding.set(adding() === "group" ? null : "group")}
              />
            }
          >
            {page().groups.map((group) => (
              <SubscriptionRow
                title={group.name}
                detail={`${group.strategy === "urltest" ? "自动测速" : "手动选择"} · ${group.subscriptionIndexes.length} 个订阅 / ${group.nodeIndexes.length} 个节点`}
                onDelete={() => remove("group", group.index)}
              />
            ))}
            {page().groups.length === 0 ? (
              <Text style={{ fontSize: 12, color: desktopColors.tertiary, padding: 12 }}>
                全局组始终存在；再加一个自动测速组即可按延迟选最快节点。
              </Text>
            ) : null}
          </DCard>
        </Scroll>
      </View>
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
            <Text style={{ fontSize: 11, fontWeight: "600", color: desktopColors.tertiary }}>汇总</Text>
            <Text style={{ fontSize: 12, color: desktopColors.text }}>
              订阅 {page().subscriptions.length}（已启用 {enabledSubscriptions}）
            </Text>
            <Text style={{ fontSize: 12, color: desktopColors.text }}>
              手动节点 {page().nodes.length}（已启用 {enabledNodes}）
            </Text>
            <Text style={{ fontSize: 12, color: desktopColors.text }}>策略组 {page().groups.length}</Text>
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
            <Text style={{ fontSize: 11, fontWeight: "600", color: desktopColors.tertiary }}>应用</Text>
            <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
              修改会先保存；点「应用」后内核重启并使用已下载的缓存，VPN 接口不会断开。
            </Text>
          </View>
        </Scroll>
      </View>
    </View>
  );
}

export function SubscriptionsPage() {
  return embedded ? <DesktopSubscriptionsPage /> : <MobileSubscriptionsPage />;
}
