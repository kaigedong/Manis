import { computed, host, List, signal, Text, View } from "nsh";
import { Input } from "nsh/base";

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

const params = (host.params ?? {}) as { data?: Page; stress?: boolean; embedded?: boolean };
const initialPage: Page = params.data?.sources
  ? JSON.parse(JSON.stringify(params.data))
  : emptyPage;

if (params.stress) {
  const stressNodes: NodeItem[] = Array.from({ length: 500 }, (_, index) => ({
    name: `测试节点 ${String(index + 1).padStart(3, "0")}`,
    displayName: `测试节点 ${String(index + 1).padStart(3, "0")}`,
    delayMs: index % 7 === 0 ? null : 20 + ((index * 37) % 300),
    current: false,
  }));
  initialPage.sources.push({
    id: "stress",
    name: "压力测试",
    detail: "500 个节点 · 列表回收测试",
    kind: "other",
    enabled: true,
    nodes: stressNodes,
  });
  initialPage.global.total += 500;
}

const page = signal<Page>(initialPage);
host.on("nodes.updated", (payload) => {
  const next = (payload as { page?: Page } | undefined)?.page;
  if (next) page.set(next);
});
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
    message.set("独立运行模式：没有宿主可刷新");
    return;
  }
  refreshing.set(true);
  message.set("");
  try {
    const reply = (await host.call("nodes.refresh")) as { page?: Page };
    if (reply?.page) page.set(reply.page);
    message.set("已刷新");
  } catch (error) {
    message.set(error instanceof Error ? error.message : "刷新失败");
  } finally {
    refreshing.set(false);
  }
}

function selectSourceNode(name: string) {
  page.update((current) => ({
    ...current,
    global: { ...current.global, current: name },
    sources: current.sources.map((source) => ({
      ...source,
      nodes: source.nodes.map((node) => ({
        ...node,
        current: node.name === name,
      })),
    })),
  }));
  if (host.available()) {
    void host.call("nodes.select", { name }).catch(() => undefined);
  }
}

function selectPolicyNode(groupId: string, name: string) {
  page.update((current) => ({
    ...current,
    policyGroups: current.policyGroups.map((group) =>
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
  }));
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
          {source().nodes.length} 个
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
          {group().members.length} 个
        </Text>
      </View>
      <Text style={{ fontSize: 12, color: "#6b7280", marginTop: 4 }}>
        {group().kind} · 当前：{group().current ?? "未选择"}
      </Text>
    </View>
  );
}

function NodeRow(props: {
  node: () => NodeItem;
  onPress: () => void;
}) {
  const node = () => props.node();
  return (
    <View
      onPress={props.onPress}
      role="button"
      accessible
      accessibilityLabel={`${node().displayName || node().name}${node().current ? "，当前" : ""}，${node().delayMs === null ? "未测速" : `${node().delayMs} 毫秒`}`}
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
      <Text style={{ fontSize: 15, flex: 1 }}>
        {node().displayName || node().name}
      </Text>
      {node().current ? (
        <Text style={{ fontSize: 11, color: "#7c3aed" }}>当前</Text>
      ) : null}
      <Text style={{ fontSize: 12, color: "#6b7280" }}>
        {node().delayMs === null ? "—" : `${node().delayMs} ms`}
      </Text>
    </View>
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

export function NodesPage() {
  return (
    <View class={params.embedded ? "flex-1 bg-background" : "flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"}>
      <View style={{ paddingHorizontal: 20, paddingTop: 18, gap: 12 }}>
        <View style={{ flexDirection: "row", alignItems: "baseline", gap: 10 }}>
          <Text style={{ fontSize: 28, fontWeight: "600", flex: 1 }}>节点</Text>
          <Text style={{ fontSize: 13, color: "#6b7280" }}>
            {page().global.total} 个 · NSH
          </Text>
        </View>
        <Text style={{ fontSize: 13, color: "#6b7280" }}>
          按来源分组；点选立即切换，搜索支持中文和英文。
        </Text>
        <Input
          value={query()}
          onChange={(value) => query.set(value)}
          placeholder="搜索节点"
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
            {refreshing() ? "刷新中…" : "刷新"}
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
            当前：{page().global.current ?? "未选择"}
          </Text>
        </View>
      </View>

      {rows().length === 0 ? (
        <View style={{ padding: 20 }}>
          <Text style={{ fontSize: 14, color: "#6b7280" }}>
            没有匹配的节点。
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

