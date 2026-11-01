import { computed, host, List, mount, signal, Text, View } from "nsh";
import { Input } from "nsh/base";

type NodeItem = {
  name: string;
  delay: number | null;
  current?: boolean;
};

type Source = {
  name: string;
  detail: string;
  nodes: NodeItem[];
};

type Row =
  | { kind: "header"; source: Source }
  | { kind: "node"; source: Source; node: NodeItem };

const sampleSources: Source[] = [
  {
    name: "我的订阅",
    detail: "订阅 · example.com",
    nodes: [
      { name: "香港 01", delay: 42, current: true },
      { name: "日本 02", delay: 78 },
      { name: "新加坡 03", delay: 95 },
    ],
  },
  {
    name: "手动节点",
    detail: "分享链接导入",
    nodes: [{ name: "ManualA", delay: null }],
  },
];

const params = (host.params ?? {}) as { sources?: Source[]; stress?: boolean };
const initialSources = params.sources?.length ? params.sources : sampleSources;

if (params.stress) {
  initialSources.push({
    name: "压力测试",
    detail: "500 个节点 · 列表回收测试",
    nodes: Array.from({ length: 500 }, (_, index) => ({
      name: `测试节点 ${String(index + 1).padStart(3, "0")}`,
      delay: index % 7 === 0 ? null : 20 + ((index * 37) % 300),
    })),
  });
}

const sources = signal<Source[]>(initialSources);
const query = signal("");
const refreshing = signal(false);
const message = signal("");

const rows = computed<Row[]>(() => {
  const needle = query().trim().toLowerCase();
  const result: Row[] = [];
  for (const source of sources()) {
    const nodes = needle
      ? source.nodes.filter((node) => node.name.toLowerCase().includes(needle))
      : source.nodes;
    if (nodes.length === 0) continue;
    result.push({ kind: "header", source });
    for (const node of nodes) result.push({ kind: "node", source, node });
  }
  return result;
});

const totalNodes = computed(() =>
  sources().reduce((sum, source) => sum + source.nodes.length, 0),
);

async function refresh() {
  if (!host.available()) {
    message.set("独立运行模式：没有宿主可刷新");
    return;
  }
  refreshing.set(true);
  message.set("");
  try {
    const reply = (await host.call("nodes.refresh")) as { sources?: Source[] };
    if (reply?.sources?.length) sources.set(reply.sources);
    message.set("已刷新");
  } catch (error) {
    message.set(error instanceof Error ? error.message : "刷新失败");
  } finally {
    refreshing.set(false);
  }
}

function selectNode(node: NodeItem) {
  sources.update((current) =>
    current.map((source) => ({
      ...source,
      nodes: source.nodes.map((item) => ({
        ...item,
        current: item.name === node.name,
      })),
    })),
  );
  if (host.available()) {
    void host.call("nodes.select", { name: node.name }).catch(() => undefined);
  }
}

type HeaderRowData = Extract<Row, { kind: "header" }>;
type NodeRowData = Extract<Row, { kind: "node" }>;

function HeaderRow(props: { row: () => Row }) {
  const source = () => (props.row() as HeaderRowData).source;
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

function NodeRow(props: { row: () => Row }) {
  const node = () => (props.row() as NodeRowData).node;
  return (
    <View
      onPress={() => selectNode(node())}
      role="button"
      accessible
      accessibilityLabel={`${node().name}${node().current ? "，当前" : ""}，${node().delay === null ? "未测速" : `${node().delay} 毫秒`}`}
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
      <Text style={{ fontSize: 15, flex: 1 }}>{node().name}</Text>
      {node().current ? (
        <Text style={{ fontSize: 11, color: "#7c3aed" }}>当前</Text>
      ) : null}
      <Text style={{ fontSize: 12, color: "#6b7280" }}>
        {node().delay === null ? "—" : `${node().delay} ms`}
      </Text>
    </View>
  );
}

function App() {
  return (
    <View class="flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]">
      <View style={{ paddingHorizontal: 20, paddingTop: 18, gap: 12 }}>
        <View style={{ flexDirection: "row", alignItems: "baseline", gap: 10 }}>
          <Text style={{ fontSize: 28, fontWeight: "600", flex: 1 }}>节点</Text>
          <Text style={{ fontSize: 13, color: "#6b7280" }}>
            {totalNodes()} 个 · NSH
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
          <Text style={{ fontSize: 17, fontWeight: "600" }}>全局</Text>
          <Text style={{ fontSize: 13, color: "#4b5563" }}>
            所有订阅和手动节点 · {totalNodes()} 个
          </Text>
          <Text style={{ fontSize: 14, color: "#7c3aed" }}>
            当前：
            {sources()
              .flatMap((source) => source.nodes)
              .find((node) => node.current)?.name ?? "未选择"}
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
          {(row) => (
            <If
              when={row().kind === "header"}
              else={<NodeRow row={row} />}
            >
              <HeaderRow row={row} />
            </If>
          )}
        </List>
      )}
    </View>
  );
}

mount(App);
