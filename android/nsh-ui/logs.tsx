import { Clipboard, host, Scroll, signal, Text, View } from "nsh";
import { embedded } from "./store";
import { desktopColors } from "./theme";
import { Action, Card } from "./ui";

const lines = signal<string[]>([]);
const busy = signal(false);
const message = signal("");

async function loadLogs() {
  if (!host.available()) {
    message.set("独立运行模式：没有宿主可读取日志");
    return;
  }
  busy.set(true);
  try {
    const reply = (await host.call("settings.logs")) as { logs?: string[] } | null;
    lines.set(reply?.logs ?? []);
    message.set(`${lines().length} 行`);
  } catch (error) {
    message.set(error instanceof Error ? error.message : "读取日志失败");
  } finally {
    busy.set(false);
  }
}

async function copyLogs() {
  await Clipboard.writeText(lines().join("\n"));
  message.set("日志已复制到剪贴板");
}

if (host.available()) {
  void loadLogs();
}

function LogLines() {
  return (
    <Scroll style={{ flex: 1 }}>
      {lines().map((line) => (
        <Text
          style={{
            fontSize: embedded ? 11 : 11,
            fontFamily: "monospace",
            color: embedded ? desktopColors.text : "#374151",
            paddingHorizontal: embedded ? 12 : 0,
            paddingVertical: embedded ? 1 : 0,
          }}
        >
          {line}
        </Text>
      ))}
      {lines().length === 0 ? (
        <Text
          style={{
            fontSize: 12,
            color: embedded ? desktopColors.tertiary : "#6b7280",
            padding: 16,
          }}
        >
          还没有日志。
        </Text>
      ) : null}
    </Scroll>
  );
}

function DesktopLogsPage() {
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
            {message() || "内核日志来自本机 controller。"}
          </Text>
          <Action label={busy() ? "读取中…" : "刷新"} onPress={() => void loadLogs()} />
          <Action label="复制" primary onPress={() => void copyLogs()} />
        </View>
        <View style={{ flex: 1 }}>
          <LogLines />
        </View>
      </View>
      <View
        style={{
          width: 280,
          borderLeftWidth: 1,
          borderLeftColor: desktopColors.outline,
          backgroundColor: desktopColors.low,
        }}
      >
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
          <Card title="说明">
            <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
              日志由 Manis 自己的内核产生；复制后可直接贴到 issue 里。
            </Text>
          </Card>
        </Scroll>
      </View>
    </View>
  );
}

function MobileLogsPage() {
  return (
    <View class="flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]">
      <View style={{ flex: 1, paddingHorizontal: 16, paddingTop: 16, gap: 10 }}>
        <Text style={{ fontSize: 28, fontWeight: "600" }}>日志</Text>
        <Text style={{ fontSize: 12, color: "#6b7280" }}>{message() || "内核日志来自本机 controller。"}</Text>
        <View style={{ flexDirection: "row", gap: 8 }}>
          <Action label={busy() ? "读取中…" : "刷新"} onPress={() => void loadLogs()} />
          <Action label="复制" primary onPress={() => void copyLogs()} />
        </View>
        <View
          style={{
            flex: 1,
            borderWidth: 1,
            borderColor: "#e5e7eb",
            borderRadius: 12,
            padding: 10,
            backgroundColor: "#ffffff",
          }}
        >
          <LogLines />
        </View>
      </View>
    </View>
  );
}

export function LogsPage() {
  return embedded ? <DesktopLogsPage /> : <MobileLogsPage />;
}
