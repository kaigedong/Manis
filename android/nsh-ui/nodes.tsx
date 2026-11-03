import { computed, host, List, Scroll, signal, Text, View } from "nsh";
import { Input } from "nsh/base";
import { embedded, layoutWidth, root } from "./store";
import { t } from "./copy";
import { modeLabel, sourceKindLabel, strategyLabel } from "./labels";
import { desktopColors } from "./theme";
import { NodeRow as SharedNodeRow } from "./ui";

type NodeItem = {
  name: string;
  displayName: string;
  delayMs: number | null;
  current: boolean;
};

type Source = {
  id: string;
  name: string;
  detail: string;
  kind: "subscription" | "manual" | "other";
  enabled: boolean;
  nodes: NodeItem[];
};

type PolicyGroup = {
  id: string;
  label: string;
  kind: string;
  current: string | null;
  members: NodeItem[];
};

type Page = {
  global: { label: string; current: string | null; total: number };
  sources: Source[];
  policyGroups: PolicyGroup[];
};

type Row =
  | { kind: "source"; source: Source }
  | { kind: "source-node"; source: Source; node: NodeItem }
  | { kind: "policy"; group: PolicyGroup }
  | { kind: "policy-node"; group: PolicyGroup; node: NodeItem };

type SourceRow = Extract<Row, { kind: "source" }>;
type SourceNodeRow = Extract<Row, { kind: "source-node" }>;
type PolicyRow = Extract<Row, { kind: "policy" }>;
type PolicyNodeRow = Extract<Row, { kind: "policy-node" }>;

const emptyPage: Page = {
  global: { label: "全局", current: null, total: 0 },
  sources: [],
  policyGroups: [],
};

const page = computed<Page>(() => (root().nodes ?? emptyPage) as Page);
const query = signal("");
const refreshing = signal(false);
const message = signal("");

const rows = computed<Row[]>(() => {
  const needle = query().trim().toLowerCase();
  const result: Row[] = [];
  for (const source of page().sources) {
    const nodes = needle
      ? source.nodes.filter((node) =>
          (node.displayName || node.name).toLowerCase().includes(needle),
        )
      : source.nodes;
    if (nodes.length === 0) continue;
    result.push({ kind: "source", source });
    for (const node of nodes) result.push({ kind: "source-node", source, node });
  }
  for (const group of page().policyGroups) {
    const members = needle
      ? group.members.filter((node) =>
          (node.displayName || node.name).toLowerCase().includes(needle),
        )
      : group.members;
    if (members.length === 0) continue;
    result.push({ kind: "policy", group });
    for (const node of members) result.push({ kind: "policy-node", group, node });
  }
  return result;
});

async function refresh() {
  if (!host.available()) {
    message.set(t("nodes.noHost"));
    return;
  }
  refreshing.set(true);
  message.set("");
  try {
    const reply = (await host.call("nodes.refresh")) as { data?: unknown } | null;
    if (reply?.data) root.set(reply.data as any);
    message.set(t("nodes.refreshed"));
  } catch (error) {
    message.set(error instanceof Error ? error.message : t("nodes.refreshFailed"));
  } finally {
    refreshing.set(false);
  }
}

function selectSourceNode(name: string) {
  root.update((current) => {
    const nodes = current.nodes as Page;
    return {
      ...current,
      nodes: {
        ...nodes,
        global: { ...nodes.global, current: name },
        sources: nodes.sources.map((source) => ({
          ...source,
          nodes: source.nodes.map((node) => ({
            ...node,
            current: node.name === name,
          })),
        })),
      },
    };
  });
  if (host.available()) {
    void host.call("nodes.select", { name }).catch(() => undefined);
  }
}

function selectPolicyNode(groupId: string, name: string) {
  root.update((current) => {
    const nodes = current.nodes as Page;
    return {
      ...current,
      nodes: {
        ...nodes,
        policyGroups: nodes.policyGroups.map((group) =>
          group.id === groupId
            ? {
                ...group,
                current: name,
                members: group.members.map((node) => ({
                  ...node,
                  current: node.name === name,
                })),
              }
            : group,
        ),
      },
    };
  });
  if (host.available()) {
    void host.call("nodes.selectGroup", { group: groupId, name }).catch(() => undefined);
  }
}

function SourceHeader(props: { row: () => Row }) {
  const source = () => (props.row() as SourceRow).source;
  return (
    <View
      style={{
        marginTop: 12,
        marginHorizontal: 16,
        paddingHorizontal: 14,
        paddingTop: 12,
        paddingBottom: 8,
        backgroundColor: "#f3f4f6",
        borderTopLeftRadius: 14,
        borderTopRightRadius: 14,
      }}
    >
      <View style={{ flexDirection: "row", alignItems: "baseline", gap: 8 }}>
        <Text role="header" style={{ fontSize: 17, fontWeight: "600", flex: 1 }}>
          {source().name}
        </Text>
        <Text style={{ fontSize: 12, color: "#6b7280" }}>
          {t("nodes.count", { count: source().nodes.length })}
        </Text>
      </View>
      <Text style={{ fontSize: 12, color: "#6b7280", marginTop: 4 }}>
        {source().detail}
      </Text>
    </View>
  );
}

function PolicyHeader(props: { row: () => Row }) {
  const group = () => (props.row() as PolicyRow).group;
  return (
    <View
      style={{
        marginTop: 12,
        marginHorizontal: 16,
        paddingHorizontal: 14,
        paddingTop: 12,
        paddingBottom: 8,
        backgroundColor: "#eef2ff",
        borderTopLeftRadius: 14,
        borderTopRightRadius: 14,
      }}
    >
      <View style={{ flexDirection: "row", alignItems: "baseline", gap: 8 }}>
        <Text role="header" style={{ fontSize: 17, fontWeight: "600", flex: 1 }}>
          {group().label}
        </Text>
        <Text style={{ fontSize: 12, color: "#6b7280" }}>
          {t("nodes.count", { count: group().members.length })}
        </Text>
      </View>
      <Text style={{ fontSize: 12, color: "#6b7280", marginTop: 4 }}>
        {strategyLabel(group().kind)} · {t("conn.currentNode")}：{group().current ?? "—"}
      </Text>
    </View>
  );
}

function NodeRow(props: { node: () => NodeItem; onPress: () => void }) {
  const node = () => props.node();
  return (
    <SharedNodeRow
      name={node().displayName || node().name}
      current={node().current}
      delayMs={node().delayMs}
      onPress={props.onPress}
    />
  );
}

function Dispatch(props: { row: () => Row }) {
  return (
    <If
      when={props.row().kind === "source"}
      else={
        <If
          when={props.row().kind === "source-node"}
          else={
            <If
              when={props.row().kind === "policy"}
              else={
                <NodeRow
                  node={() => (props.row() as PolicyNodeRow).node}
                  onPress={() => {
                    const row = props.row() as PolicyNodeRow;
                    selectPolicyNode(row.group.id, row.node.name);
                  }}
                />
              }
            >
              <PolicyHeader row={props.row} />
            </If>
          }
        >
          <NodeRow
            node={() => (props.row() as SourceNodeRow).node}
            onPress={() => {
              const row = props.row() as SourceNodeRow;
              selectSourceNode(row.node.name);
            }}
          />
        </If>
      }
    >
      <SourceHeader row={props.row} />
    </If>
  );
}

function MobileNodesPage() {
  return (
    <View class={embedded ? "flex-1 bg-background" : "flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"}>
      <View style={{ paddingHorizontal: 20, paddingTop: 18, gap: 12 }}>
        <View style={{ flexDirection: "row", alignItems: "baseline", gap: 10 }}>
          <Text style={{ fontSize: 28, fontWeight: "600", flex: 1 }}>{t("nodes.title")}</Text>
          <Text style={{ fontSize: 13, color: "#6b7280" }}>
            {t("nodes.count", { count: page().global.total })} · NSH
          </Text>
        </View>
        <Text style={{ fontSize: 13, color: "#6b7280" }}>
          {t("nodes.hint")}
        </Text>
        <Input
          value={query()}
          onChange={(value) => query.set(value)}
          placeholder={t("nodes.search")}
          style={{
            borderWidth: 1,
            borderColor: "#d1d5db",
            borderRadius: 12,
            paddingHorizontal: 14,
            paddingVertical: 10,
            fontSize: 16,
          }}
        />
        <View
          onPress={refresh}
          role="button"
          style={{
            alignSelf: "flex-start",
            borderRadius: 10,
            backgroundColor: refreshing() ? "#c4b5fd" : "#7c3aed",
            paddingHorizontal: 14,
            paddingVertical: 8,
          }}
        >
          <Text style={{ color: "#ffffff", fontSize: 14 }}>
            {refreshing() ? t("nodes.refreshing") : t("nodes.refresh")}
          </Text>
        </View>
        {message() ? (
          <Text style={{ fontSize: 12, color: "#6b7280" }}>{message()}</Text>
        ) : null}
        <View
          style={{
            borderRadius: 14,
            backgroundColor: "#f3f4f6",
            padding: 14,
            gap: 6,
          }}
        >
          <Text style={{ fontSize: 17, fontWeight: "600" }}>
            {page().global.label}
          </Text>
          <Text style={{ fontSize: 13, color: "#4b5563" }}>
            所有订阅和手动节点 · {page().global.total} 个
          </Text>
          <Text style={{ fontSize: 14, color: "#7c3aed" }}>
            {t("conn.currentNode")}：{page().global.current ?? "—"}
          </Text>
        </View>
      </View>

      {rows().length === 0 ? (
        <View style={{ padding: 20 }}>
          <Text style={{ fontSize: 14, color: "#6b7280" }}>
            {t("nodes.noMatch")}
          </Text>
        </View>
      ) : (
        <List
          each={rows()}
          estimatedItemSize={58}
          style={{ flex: 1, marginTop: 8 }}
        >
          {(row) => <Dispatch row={row} />}
        </List>
      )}
    </View>
  );
}


type Selection =
  | { kind: "group"; group: PolicyGroup }
  | { kind: "source"; source: Source }
  | { kind: "global" };

const selectedKey = signal("");

const selection = computed<Selection>(() => {
  const key = selectedKey();
  const groups = page().policyGroups;
  const sources = page().sources;
  if (key.startsWith("group:")) {
    const id = key.slice("group:".length);
    const group = groups.find((entry) => entry.id === id);
    if (group) return { kind: "group", group };
  }
  if (key.startsWith("source:")) {
    const id = key.slice("source:".length);
    const source = sources.find((entry) => entry.id === id);
    if (source) return { kind: "source", source };
  }
  if (key === "global") return { kind: "global" };
  if (groups.length > 0) return { kind: "group", group: groups[0] };
  if (sources.length > 0) return { kind: "source", source: sources[0] };
  return { kind: "global" };
});

const selectionNodes = computed<NodeItem[]>(() => {
  const current = selection();
  return current.kind === "group"
    ? current.group.members
    : current.kind === "source"
      ? current.source.nodes
      : [];
});

const visibleNodes = computed<NodeItem[]>(() => {
  const needle = query().trim().toLowerCase();
  const nodes = selectionNodes();
  if (!needle) return nodes;
  return nodes.filter((node) =>
    (node.displayName || node.name).toLowerCase().includes(needle),
  );
});

function selectionTitle() {
  const current = selection();
  if (current.kind === "group") return current.group.label;
  if (current.kind === "source") return current.source.name;
  return page().global.label;
}

function selectionDetail() {
  const current = selection();
  if (current.kind === "group") {
    return `${strategyLabel(current.group.kind)} · ${t("conn.currentNode")}：${current.group.current ?? "—"} · ${t("nodes.count", { count: current.group.members.length })}`;
  }
  if (current.kind === "source") {
    return `${sourceKindLabel(current.source.kind)} · ${current.source.enabled ? t("nodes.enabled") : t("nodes.disabled")} · ${t("nodes.count", { count: current.source.nodes.length })}`;
  }
  return `${t("nodes.sources")} · ${t("nodes.count", { count: page().global.total })} · ${t("conn.currentNode")}：${page().global.current ?? "—"}`;
}

function selectNode(node: NodeItem) {
  const current = selection();
  if (current.kind === "group") {
    selectPolicyNode(current.group.id, node.name);
  } else if (current.kind === "source") {
    selectSourceNode(node.name);
  }
}

function DesktopNavCard(props: {
  title: string;
  meta: string;
  count: number;
  selected: boolean;
  onPress: () => void;
}) {
  return (
    <View
      onPress={props.onPress}
      role="button"
      accessible
      accessibilityState={{ selected: props.selected }}
      style={{
        borderRadius: 8,
        borderWidth: 1,
        borderColor: props.selected ? desktopColors.primary : "transparent",
        backgroundColor: props.selected ? "#E6F1EE" : "transparent",
        paddingHorizontal: 10,
        paddingVertical: 8,
        gap: 3,
      }}
    >
      <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
        <Text
          style={{
            flex: 1,
            fontSize: 13,
            fontWeight: props.selected ? "600" : "400",
            color: desktopColors.text,
          }}
        >
          {props.title}
        </Text>
        <Text style={{ fontSize: 11, color: desktopColors.tertiary }}>{props.count}</Text>
      </View>
      <Text style={{ fontSize: 11, color: desktopColors.secondary }}>{props.meta}</Text>
    </View>
  );
}

function DesktopGroupList() {
  return (
    <View style={{ width: 300, flexDirection: "column" }}>
    <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 2 }}>
      <Text
        style={{
          fontSize: 11,
          fontWeight: "600",
          color: desktopColors.tertiary,
          paddingHorizontal: 10,
          paddingTop: 6,
          paddingBottom: 4,
        }}
      >
        {t("nodes.groups")}
      </Text>
      {page().policyGroups.map((group) => (
        <DesktopNavCard
          title={group.label}
          meta={`${strategyLabel(group.kind)} · ${group.current ?? "—"}`}
          count={group.members.length}
          selected={selectedKey() === `group:${group.id}` || (selectedKey() === "" && selection().kind === "group" && selection().group?.id === group.id)}
          onPress={() => selectedKey.set(`group:${group.id}`)}
        />
      ))}
      {page().policyGroups.length === 0 ? (
        <Text style={{ fontSize: 12, color: desktopColors.tertiary, paddingHorizontal: 10, paddingVertical: 6 }}>
          {t("nodes.noGroups")}
        </Text>
      ) : null}
      <Text
        style={{
          fontSize: 11,
          fontWeight: "600",
          color: desktopColors.tertiary,
          paddingHorizontal: 10,
          paddingTop: 14,
          paddingBottom: 4,
        }}
      >
        {t("nodes.sources")}
      </Text>
      {page().sources.map((source) => (
        <DesktopNavCard
          title={source.name}
          meta={sourceKindLabel(source.kind)}
          count={source.nodes.length}
          selected={selectedKey() === `source:${source.id}`}
          onPress={() => selectedKey.set(`source:${source.id}`)}
        />
      ))}
      {page().sources.length === 0 ? (
        <Text style={{ fontSize: 12, color: desktopColors.tertiary, paddingHorizontal: 10, paddingVertical: 6 }}>
          {t("nodes.noSources")}
        </Text>
      ) : null}
    </Scroll>
    </View>
  );
}

function DesktopDetailHeader() {
  return (
    <View
      style={{
        paddingHorizontal: 20,
        paddingTop: 16,
        paddingBottom: 12,
        gap: 6,
        borderBottomWidth: 1,
        borderBottomColor: desktopColors.outline,
      }}
    >
      <View style={{ flexDirection: "row", alignItems: "center", gap: 10 }}>
        <Text role="header" style={{ fontSize: 18, fontWeight: "600", flex: 1, color: desktopColors.text }}>
          {selectionTitle()}
        </Text>
        <View
          onPress={refresh}
          role="button"
          accessible
          style={{
            borderRadius: 8,
            backgroundColor: refreshing() ? "#8FBDB4" : desktopColors.primary,
            paddingHorizontal: 12,
            paddingVertical: 6,
          }}
        >
          <Text style={{ fontSize: 12, color: desktopColors.onPrimary }}>
            {refreshing() ? t("nodes.refreshing") : t("nodes.refresh")}
          </Text>
        </View>
      </View>
      <Text style={{ fontSize: 12, color: desktopColors.secondary }}>{selectionDetail()}</Text>
      <Input
        value={query()}
        onChange={(value) => query.set(value)}
        placeholder={t("nodes.search")}
        style={{
          marginTop: 4,
          borderWidth: 1,
          borderColor: desktopColors.outlineStrong,
          borderRadius: 8,
          paddingHorizontal: 10,
          paddingVertical: 7,
          fontSize: 13,
        }}
      />
      {message() ? (
        <Text style={{ fontSize: 11, color: desktopColors.secondary }}>{message()}</Text>
      ) : null}
    </View>
  );
}

function DesktopDetail() {
  if (selection().kind === "global") {
    return (
      <View style={{ flex: 1, padding: 20 }}>
        <Text style={{ fontSize: 13, color: desktopColors.secondary }}>
          {t("nodes.globalHint")}
        </Text>
      </View>
    );
  }
  return (
    <Scroll style={{ flex: 1 }}>
      {visibleNodes().map((node) => (
        <SharedNodeRow
          name={node.displayName || node.name}
          current={node.current}
          delayMs={node.delayMs}
          onPress={() => selectNode(node)}
        />
      ))}
      {visibleNodes().length === 0 ? (
        <Text style={{ padding: 20, fontSize: 12, color: desktopColors.tertiary }}>
          {t("nodes.noMatch")}
        </Text>
      ) : null}
    </Scroll>
  );
}

function DesktopInspectorCard(props: { title: string; children: any }) {
  return (
    <View
      style={{
        borderRadius: 10,
        borderWidth: 1,
        borderColor: desktopColors.outline,
        backgroundColor: desktopColors.high,
        padding: 12,
        gap: 6,
      }}
    >
      <Text style={{ fontSize: 11, fontWeight: "600", color: desktopColors.tertiary }}>
        {props.title}
      </Text>
      {props.children}
    </View>
  );
}

function DesktopInspector() {
  const connection = root().connection ?? {};
  const rules = root().rules ?? {};
  const current = page().global.current ?? "未选择";
  const warningCount = Array.isArray(rules.warnings) ? rules.warnings.length : 0;
  return (
    <View style={{ width: 320, flexDirection: "column", backgroundColor: desktopColors.low }}>
    <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
      <DesktopInspectorCard title={t("conn.chain")}>
        <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
          {t("conn.chainFlow")}
        </Text>
        <Text style={{ fontSize: 13, color: desktopColors.text }}>{t("conn.currentNode")}：{current}</Text>
        <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
          {t("nodes.mode")}：{connection.mode ? modeLabel(connection.mode) : "—"}
        </Text>
      </DesktopInspectorCard>
      <DesktopInspectorCard title={t("nodes.runtime")}>
        <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
          {connection.summary || t("nodes.waiting")}
        </Text>
        <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
          {connection.active ? t("conn.connected") : t("conn.disconnected")}
        </Text>
      </DesktopInspectorCard>
      <DesktopInspectorCard title={t("nodes.rules")}>
        <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
          {rules.summary || t("nodes.noSummary")}
        </Text>
        <Text style={{ fontSize: 12, color: warningCount > 0 ? desktopColors.warning : desktopColors.secondary }}>
          {warningCount > 0 ? t("nodes.warnings", { count: warningCount }) : t("nodes.noWarnings")}
        </Text>
      </DesktopInspectorCard>
    </Scroll>
    </View>
  );
}

function DesktopNodesPage() {
  return (
    <View style={{ flex: 1, flexDirection: "row", backgroundColor: desktopColors.high }}>
      <DesktopGroupList />
      <View
        style={{
          flex: 1,
          minWidth: 0,
          flexDirection: "column",
          borderLeftWidth: 1,
          borderRightWidth: 1,
          borderLeftColor: desktopColors.outline,
          borderRightColor: desktopColors.outline,
        }}
      >
        <DesktopDetailHeader />
        <DesktopDetail />
      </View>
      {layoutWidth() === 0 || layoutWidth() >= 1100 ? <DesktopInspector /> : null}
    </View>
  );
}

export function NodesPage() {
  return embedded ? <DesktopNodesPage /> : <MobileNodesPage />;
}
