import { Clipboard, computed, host, Scroll, signal, Text, View } from "nsh";
import { Input } from "nsh/base";
import { embedded, root, windowWidth } from "./store";
import { desktopColors } from "./theme";
import { Action, Card, Info } from "./ui";

type UpdateInfo = {
  state: "idle" | "checking" | "upToDate" | "available" | "downloading" | "ready" | "failed";
  message?: string;
  progress?: number;
  update?: { version: string; size: number; releaseUrl: string };
};

type Dns = {
  mode: string;
  custom: string[];
  discovered: string[];
  label: string;
};

type Data = {
  status: string;
  mode: string;
  proxyDns: Dns;
  kernelVersion: string;
  summary: string;
  subscriptions: number;
  nodes: number;
  groups: number;
  abi: string;
  dataDir: string;
  installedVersion: string;
  update: UpdateInfo;
};

type Reply = { data?: unknown; message?: string; logs?: string[]; backup?: string };

const empty: Data = {
  status: "未连接",
  mode: "rule",
  proxyDns: { mode: "doh", custom: [], discovered: [], label: "加密 DNS（DoH）" },
  kernelVersion: "—",
  summary: "—",
  subscriptions: 0,
  nodes: 0,
  groups: 0,
  abi: "unknown",
  dataDir: "files/mihomo",
  installedVersion: "未知",
  update: { state: "idle" },
};
const page = computed<Data>(() => (root().settings ?? empty) as Data);
const message = signal("");
const busy = signal(false);
const showDns = signal(false);
const dnsMode = signal("doh");
const dnsServers = signal("");
const logs = signal<string[]>([]);
const showLogs = signal(false);

async function invoke(command: string, payload: Record<string, unknown> = {}) {
  if (!host.available()) {
    message.set("独立运行模式：没有宿主可执行");
    return null;
  }
  busy.set(true);
  try {
    const reply = (await host.call(command, payload)) as Reply | null;
    if (reply?.data) root.set(reply.data as any);
    if (reply?.logs) {
      logs.set(reply.logs);
      showLogs.set(true);
    }
    message.set(reply?.message ?? "");
    return reply;
  } catch (error) {
    message.set(error instanceof Error ? error.message : "操作失败");
    return null;
  } finally {
    busy.set(false);
  }
}

function openDns() {
  dnsMode.set(page().proxyDns.mode);
  dnsServers.set(page().proxyDns.custom.join("\n"));
  showDns.set(true);
}

async function copyLogs() {
  await Clipboard.writeText(logs().join("\n"));
  message.set("日志已复制到剪贴板");
}

async function copyBackup() {
  const reply = await invoke("settings.copyBackup");
  if (reply?.backup) {
    await Clipboard.writeText(reply.backup);
    message.set("备份已复制到剪贴板");
  }
}

async function importBackup() {
  const text = await Clipboard.readText();
  if (!text.trim()) {
    message.set("剪贴板里没有备份内容");
    return;
  }
  await invoke("settings.importBackup", { text });
}

async function saveDns() {
  const servers = dnsServers()
    .split(/[\n,;]+/)
    .map((item) => item.trim())
    .filter(Boolean);
  const reply = await invoke("settings.dns.set", { mode: dnsMode(), servers });
  if (reply?.message?.includes("已更新")) showDns.set(false);
}

function formatSize(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${Math.round(bytes / 1024)} KB`;
}

function DnsPanel() {
  return (
    <Card title="节点域名解析">
      <Text style={{ fontSize: 12, color: "#6b7280" }}>
        代理服务器域名如何变成地址。所有节点超时但桌面正常时，先检查这里。
      </Text>
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action
          label="加密 DNS（DoH）"
          primary={dnsMode() === "doh"}
          onPress={() => dnsMode.set("doh")}
        />
        <Action
          label="本机 DNS"
          primary={dnsMode() === "system"}
          onPress={() => dnsMode.set("system")}
        />
      </View>
      {page().proxyDns.discovered.length > 0 ? (
        <Text style={{ fontSize: 12, color: "#6b7280" }}>
          订阅自带 DoH：{page().proxyDns.discovered.join("，")}
        </Text>
      ) : null}
      <Text style={{ fontSize: 12, color: "#6b7280" }}>
        自定义 DoH（留空则使用当前模式）
      </Text>
      <Input
        value={dnsServers()}
        onChange={(value) => dnsServers.set(value)}
        placeholder="https://dns.example/dns-query，一行一个"
        multiline
        style={{ borderWidth: 1, borderColor: "#d1d5db", borderRadius: 10, padding: 10, minHeight: 80 }}
      />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action label="保存并重新载入" primary onPress={() => void saveDns()} />
        <Action label="取消" onPress={() => showDns.set(false)} />
      </View>
    </Card>
  );
}

function LogsPanel() {
  return (
    <Card title="内核日志">
      <Scroll style={{ height: 340 }}>
        <View style={{ gap: 2 }}>
          {logs().map((line, index) => (
            <Text
             
              style={{ fontSize: 11, fontFamily: "monospace", color: "#374151" }}
            >
              {line}
            </Text>
          ))}
        </View>
      </Scroll>
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action
          label="复制"
          onPress={() => void copyLogs()}
        />
        <Action label="刷新" onPress={() => void invoke("settings.logs")} />
        <Action label="关闭" onPress={() => showLogs.set(false)} />
      </View>
    </Card>
  );
}

function UpdateCard() {
  const update = () => page().update;
  return (
    <Card title="应用更新">
      <Info label="当前版本" value={page().installedVersion} />
      {update().state === "idle" ? (
        <Action label="检查更新" onPress={() => void invoke("settings.update.check")} />
      ) : null}
      {update().state === "checking" ? (
        <Text style={{ fontSize: 13, color: "#6b7280" }}>正在检查 GitHub Release…</Text>
      ) : null}
      {update().state === "upToDate" ? (
        <View style={{ gap: 8 }}>
          <Text style={{ fontSize: 13 }}>已是最新版本</Text>
          <Action label="重新检查" onPress={() => void invoke("settings.update.check")} />
        </View>
      ) : null}
      {update().state === "available" ? (
        <View style={{ gap: 8 }}>
          <Text style={{ fontSize: 15 }}>发现新版本 {update().update?.version}</Text>
          {update().update && update().update!.size > 0 ? (
            <Text style={{ fontSize: 12, color: "#6b7280" }}>
              安装包 {formatSize(update().update!.size)}
            </Text>
          ) : null}
          <View style={{ flexDirection: "row", gap: 8 }}>
            <Action label="下载" primary onPress={() => void invoke("settings.update.download")} />
            <Action label="查看 Release" onPress={() => void invoke("settings.update.release")} />
          </View>
        </View>
      ) : null}
      {update().state === "downloading" ? (
        <View style={{ gap: 6 }}>
          <Text style={{ fontSize: 13 }}>正在下载 {update().update?.version}…</Text>
          <View style={{ height: 8, borderRadius: 4, backgroundColor: "#ede9fe" }}>
            <View
              style={{
                height: 8,
                width: `${Math.max(2, Math.min(100, update().progress ?? 0))}%`,
                borderRadius: 4,
                backgroundColor: "#7c3aed",
              }}
            />
          </View>
          <Text style={{ fontSize: 12, color: "#6b7280" }}>{update().progress ?? 0}%</Text>
        </View>
      ) : null}
      {update().state === "ready" ? (
        <View style={{ gap: 8 }}>
          <Text style={{ fontSize: 15 }}>新版本 {update().update?.version} 已下载</Text>
          <Action label="安装" primary onPress={() => void invoke("settings.update.install")} />
        </View>
      ) : null}
      {update().state === "failed" ? (
        <View style={{ gap: 8 }}>
          <Text style={{ fontSize: 12, color: "#b91c1c" }}>{update().message}</Text>
          <View style={{ flexDirection: "row", gap: 8 }}>
            <Action
              label={update().update ? "重试下载" : "重新检查"}
              onPress={() =>
                void invoke(
                  update().update ? "settings.update.download" : "settings.update.check",
                )
              }
            />
            <Action label="打开 Release" onPress={() => void invoke("settings.update.release")} />
          </View>
        </View>
      ) : null}
      <Text style={{ fontSize: 12, color: "#6b7280" }}>
        从 GitHub Release 下载官方 APK，校验同名 .sha256 后才会启动系统安装器。
      </Text>
    </Card>
  );
}

function MobileSettingsPage() {
  return (
    <View class={embedded ? "flex-1 bg-background" : "flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"}>
      <Scroll style={{ flex: 1 }}>
        <View style={{ paddingHorizontal: 20, paddingTop: 18, paddingBottom: 24, gap: 12 }}>
          <Text style={{ fontSize: 28, fontWeight: "600" }}>设置</Text>
          {message() ? (
            <Text style={{ fontSize: 12, color: "#6b7280" }}>{message()}</Text>
          ) : null}

          {showDns() ? <DnsPanel /> : null}
          {showLogs() ? <LogsPanel /> : null}

          <Card title="连接">
            <Info label="状态" value={page().status} />
            <Info label="路由模式" value={page().mode} />
            <Info label="节点域名解析" value={page().proxyDns.label} />
            <Action label="节点域名解析" onPress={openDns} />
          </Card>

          {page().autoStart !== undefined ? (
            <Card title="启动">
              <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
                <Text style={{ fontSize: 14, flex: 1 }}>开机自动启动</Text>
                <Action
                  label={page().autoStart ? "关闭" : "开启"}
                  onPress={() =>
                    void invoke("settings.autostart.set", { enabled: !page().autoStart })
                  }
                />
              </View>
            </Card>
          ) : null}

          <Card title="配置摘要">
            <Info label="内核版本" value={page().kernelVersion} />
            <Info label="配置" value={page().summary} />
            <Info
              label="工作区"
              value={`${page().subscriptions} 订阅 · ${page().nodes} 节点 · ${page().groups} 策略组`}
            />
            <Info label="设备 ABI" value={page().abi} />
            <Info label="数据目录" value={page().dataDir} />
            <Action label="重新载入配置" onPress={() => void invoke("settings.reload")} />
          </Card>

          <Card title="备份与恢复">
            <Text style={{ fontSize: 12, color: "#6b7280" }}>
              导出的是 Manis 备份格式（含订阅链接和节点密钥），不是交给其他客户端的 YAML。
            </Text>
            <View style={{ flexDirection: "row", gap: 8 }}>
              <Action label="复制到剪贴板" onPress={() => void copyBackup()} />
              <Action label="从剪贴板导入" onPress={() => void importBackup()} />
            </View>
          </Card>

          <Card title="诊断">
            <Action label="查看内核日志" onPress={() => void invoke("settings.logs")} />
          </Card>

          <UpdateCard />

          <Card title="关于">
            <Text style={{ fontSize: 14 }}>Manis Android 实验版</Text>
            <Text style={{ fontSize: 12, color: "#6b7280" }}>
              Mihomo 内核 v1.19.33 + Android 兼容补丁，随 APK 以 GPL-3.0 分发；补丁与构建脚本见仓库
              android/ 目录。
            </Text>
            <Text style={{ fontSize: 12, color: "#6b7280" }}>
              Manis 应用代码为 Apache-2.0。Mihomo 是 GPL-3.0，APK 内打包了自行编译的内核，对应源码与补丁随仓库发布。
            </Text>
          </Card>
        </View>
      </Scroll>
    </View>
  );
}

function DesktopSettingsPage() {
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
            {message() || "连接、解析、备份、更新与内核信息。"}
          </Text>
          <Action label="重新载入配置" onPress={() => void invoke("settings.reload")} />
        </View>
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
          {showDns() ? <DnsPanel /> : null}
          {showLogs() ? <LogsPanel /> : null}
          <Card title="连接">
            <Info label="状态" value={page().status} />
            <Info label="路由模式" value={page().mode} />
            <Info label="节点域名解析" value={page().proxyDns.label} />
            <Action label="节点域名解析" onPress={openDns} />
          </Card>
          {page().autoStart !== undefined ? (
            <Card title="启动">
              <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
                <Text style={{ fontSize: 12, flex: 1, color: desktopColors.text }}>开机自动启动</Text>
                <Action
                  label={page().autoStart ? "关闭" : "开启"}
                  onPress={() => void invoke("settings.autostart.set", { enabled: !page().autoStart })}
                />
              </View>
            </Card>
          ) : null}
          <Card title="配置摘要">
            <Info label="内核版本" value={page().kernelVersion} />
            <Info label="配置" value={page().summary} />
            <Info
              label="工作区"
              value={`${page().subscriptions} 订阅 · ${page().nodes} 节点 · ${page().groups} 策略组`}
            />
            <Info label="设备 ABI" value={page().abi} />
            <Info label="数据目录" value={page().dataDir} />
          </Card>
          <Card title="备份与恢复">
            <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
              导出的是 Manis 备份格式（含订阅链接和节点密钥），不是交给其他客户端的 YAML。
            </Text>
            <View style={{ flexDirection: "row", gap: 8 }}>
              <Action label="复制到剪贴板" onPress={() => void copyBackup()} />
              <Action label="从剪贴板导入" onPress={() => void importBackup()} />
            </View>
          </Card>
          <Card title="诊断">
            <Action label="查看内核日志" onPress={() => void invoke("settings.logs")} />
          </Card>
          <UpdateCard />
          <Card title="关于">
            <Text style={{ fontSize: 12, color: desktopColors.text }}>Manis 桌面版</Text>
            <Text style={{ fontSize: 11, color: desktopColors.secondary }}>
              Mihomo 内核随发行包分发；补丁与构建脚本见仓库 android/ 目录。
            </Text>
          </Card>
        </Scroll>
      </View>
      {windowWidth === 0 || windowWidth >= 1100 ? (
      <View
        style={{
          width: 300,
          borderLeftWidth: 1,
          borderLeftColor: desktopColors.outline,
          backgroundColor: desktopColors.low,
        }}
      >
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
          <Card title="状态">
            <Info label="连接" value={page().status} />
            <Info label="已安装版本" value={page().installedVersion} />
            <Info label="更新" value={page().update.state} />
          </Card>
          <Card title="工作区">
            <Info label="订阅" value={`${page().subscriptions}`} />
            <Info label="节点" value={`${page().nodes}`} />
            <Info label="策略组" value={`${page().groups}`} />
          </Card>
        </Scroll>
      </View>
      ) : null}
    </View>
  );
}

export function SettingsPage() {
  return embedded ? <DesktopSettingsPage /> : <MobileSettingsPage />;
}
