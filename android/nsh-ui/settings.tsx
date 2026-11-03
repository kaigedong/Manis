import { Clipboard, computed, host, Scroll, signal, Text, View } from "nsh";
import { Input } from "nsh/base";
import { lang, setLanguage, t } from "./copy";
import { embedded, layoutWidth, root } from "./store";
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
    <Card title={t("settings.dns.title")}>
      <Text style={{ fontSize: 12, color: "#6b7280" }}>
        {t("settings.dns.hint")}
      </Text>
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action
          label={t("settings.dns.doh")}
          primary={dnsMode() === "doh"}
          onPress={() => dnsMode.set("doh")}
        />
        <Action
          label={t("settings.dns.system")}
          primary={dnsMode() === "system"}
          onPress={() => dnsMode.set("system")}
        />
      </View>
      {page().proxyDns.discovered.length > 0 ? (
        <Text style={{ fontSize: 12, color: "#6b7280" }}>
          {t("settings.dns.discovered", { servers: page().proxyDns.discovered.join("，") })}
        </Text>
      ) : null}
      <Text style={{ fontSize: 12, color: "#6b7280" }}>
        {t("settings.dns.custom")}
      </Text>
      <Input
        value={dnsServers()}
        onChange={(value) => dnsServers.set(value)}
        placeholder="https://dns.example/dns-query，一行一个"
        multiline
        style={{ borderWidth: 1, borderColor: "#d1d5db", borderRadius: 10, padding: 10, minHeight: 80 }}
      />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Action label={t("settings.dns.save")} primary onPress={() => void saveDns()} />
        <Action label={t("settings.dns.cancel")} onPress={() => showDns.set(false)} />
      </View>
    </Card>
  );
}

function LogsPanel() {
  return (
    <Card title={t("settings.logs.title")}>
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
          label={t("settings.logs.copy")}
          onPress={() => void copyLogs()}
        />
        <Action label={t("settings.logs.refresh")} onPress={() => void invoke("settings.logs")} />
        <Action label={t("settings.logs.close")} onPress={() => showLogs.set(false)} />
      </View>
    </Card>
  );
}

function UpdateCard() {
  const update = () => page().update;
  return (
    <Card title={t("settings.updates")}>
      <Info label={t("settings.currentVersion")} value={page().installedVersion} />
      {update().state === "idle" ? (
        <Action label={t("settings.checkUpdates")} onPress={() => void invoke("settings.update.check")} />
      ) : null}
      {update().state === "checking" ? (
        <Text style={{ fontSize: 13, color: "#6b7280" }}>{t("settings.checking")}</Text>
      ) : null}
      {update().state === "upToDate" ? (
        <View style={{ gap: 8 }}>
          <Text style={{ fontSize: 13 }}>{t("settings.upToDate")}</Text>
          <Action label={t("settings.checkAgain")} onPress={() => void invoke("settings.update.check")} />
        </View>
      ) : null}
      {update().state === "available" ? (
        <View style={{ gap: 8 }}>
          <Text style={{ fontSize: 15 }}>{t("settings.available", { version: update().update?.version ?? "" })}</Text>
          {update().update && update().update!.size > 0 ? (
            <Text style={{ fontSize: 12, color: "#6b7280" }}>
              {t("settings.packageSize", { size: formatSize(update().update!.size) })}
            </Text>
          ) : null}
          <View style={{ flexDirection: "row", gap: 8 }}>
            <Action label={t("settings.download")} primary onPress={() => void invoke("settings.update.download")} />
            <Action label={t("settings.viewRelease")} onPress={() => void invoke("settings.update.release")} />
          </View>
        </View>
      ) : null}
      {update().state === "downloading" ? (
        <View style={{ gap: 6 }}>
          <Text style={{ fontSize: 13 }}>{t("settings.downloading", { version: update().update?.version ?? "" })}</Text>
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
          <Text style={{ fontSize: 15 }}>{t("settings.ready", { version: update().update?.version ?? "" })}</Text>
          <Action label={t("settings.install")} primary onPress={() => void invoke("settings.update.install")} />
        </View>
      ) : null}
      {update().state === "failed" ? (
        <View style={{ gap: 8 }}>
          <Text style={{ fontSize: 12, color: "#b91c1c" }}>{update().message}</Text>
          <View style={{ flexDirection: "row", gap: 8 }}>
            <Action
              label={update().update ? t("settings.retryDownload") : t("settings.checkAgain")}
              onPress={() =>
                void invoke(
                  update().update ? "settings.update.download" : "settings.update.check",
                )
              }
            />
            <Action label={t("settings.openRelease")} onPress={() => void invoke("settings.update.release")} />
          </View>
        </View>
      ) : null}
      <Text style={{ fontSize: 12, color: "#6b7280" }}>
        {t("settings.updateNote")}
      </Text>
    </Card>
  );
}

function MobileSettingsPage() {
  return (
    <View class={embedded ? "flex-1 bg-background" : "flex-1 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"}>
      <Scroll style={{ flex: 1 }}>
        <View style={{ paddingHorizontal: 20, paddingTop: 18, paddingBottom: 24, gap: 12 }}>
          <Text style={{ fontSize: 28, fontWeight: "600" }}>{t("settings.title")}</Text>
          {message() ? (
            <Text style={{ fontSize: 12, color: "#6b7280" }}>{message()}</Text>
          ) : null}

          {showDns() ? <DnsPanel /> : null}
          {showLogs() ? <LogsPanel /> : null}

          <Card title={t("settings.language")}>
            <Text style={{ fontSize: 12, color: embedded ? desktopColors.secondary : "#6b7280" }}>
              {t("settings.languageHint")}
            </Text>
            <View style={{ flexDirection: "row", gap: 8 }}>
              <View
                onPress={() => void setLanguage("zh")}
                role="button"
                accessible
                accessibilityState={{ selected: lang() === "zh" }}
                style={{
                  borderRadius: 8,
                  paddingHorizontal: 12,
                  paddingVertical: 6,
                  backgroundColor: lang() === "zh" ? (embedded ? desktopColors.primary : "#7c3aed") : embedded ? desktopColors.low : "#f3f4f6",
                }}
              >
                <Text style={{ fontSize: 12, color: lang() === "zh" ? "#ffffff" : embedded ? desktopColors.text : "#374151" }}>
                  中文
                </Text>
              </View>
              <View
                onPress={() => void setLanguage("en")}
                role="button"
                accessible
                accessibilityState={{ selected: lang() === "en" }}
                style={{
                  borderRadius: 8,
                  paddingHorizontal: 12,
                  paddingVertical: 6,
                  backgroundColor: lang() === "en" ? (embedded ? desktopColors.primary : "#7c3aed") : embedded ? desktopColors.low : "#f3f4f6",
                }}
              >
                <Text style={{ fontSize: 12, color: lang() === "en" ? "#ffffff" : embedded ? desktopColors.text : "#374151" }}>
                  English
                </Text>
              </View>
            </View>
          </Card>
          <Card title={t("settings.connection")}>
            <Info label={t("settings.status")} value={page().status} />
            <Info label={t("settings.mode")} value={page().mode} />
            <Info label={t("settings.dnsLabel")} value={page().proxyDns.label} />
            <Action label={t("settings.dns.title")} onPress={openDns} />
          </Card>

          {page().autoStart !== undefined ? (
            <Card title={t("settings.startup")}>
              <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
                <Text style={{ fontSize: 14, flex: 1 }}>{t("settings.launchAtLogin")}</Text>
                <Action
                  label={page().autoStart ? t("settings.turnOff") : t("settings.turnOn")}
                  onPress={() =>
                    void invoke("settings.autostart.set", { enabled: !page().autoStart })
                  }
                />
              </View>
            </Card>
          ) : null}

          <Card title={t("settings.summaryCard")}>
            <Info label={t("settings.kernelVersion")} value={page().kernelVersion} />
            <Info label={t("settings.config")} value={page().summary} />
            <Info
              label={t("settings.workspace")}
              value={`${page().subscriptions} 订阅 · ${page().nodes} 节点 · ${page().groups} 策略组`}
            />
            <Info label={t("settings.abi")} value={page().abi} />
            <Info label={t("settings.dataDir")} value={page().dataDir} />
            <Action label={t("settings.reload")} onPress={() => void invoke("settings.reload")} />
          </Card>

          <Card title={t("settings.backup")}>
            <Text style={{ fontSize: 12, color: "#6b7280" }}>
              {t("settings.backupHint")}
            </Text>
            <View style={{ flexDirection: "row", gap: 8 }}>
              <Action label={t("settings.copyBackup")} onPress={() => void copyBackup()} />
              <Action label={t("settings.importBackup")} onPress={() => void importBackup()} />
            </View>
          </Card>

          <Card title={t("settings.diagnostics")}>
            <Action label={t("settings.viewLogs")} onPress={() => void invoke("settings.logs")} />
          </Card>

          <UpdateCard />

          <Card title={t("settings.about")}>
            <Text style={{ fontSize: 14 }}>{t("settings.aboutAndroid")}</Text>
            <Text style={{ fontSize: 12, color: "#6b7280" }}>
              {t("settings.aboutAndroidHint")}
            </Text>
            <Text style={{ fontSize: 12, color: "#6b7280" }}>
              {t("settings.aboutLicenses")}
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
            {message() || t("settings.headerHint")}
          </Text>
          <Action label={t("settings.reload")} onPress={() => void invoke("settings.reload")} />
        </View>
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
          {showDns() ? <DnsPanel /> : null}
          {showLogs() ? <LogsPanel /> : null}
          
          <Card title={t("settings.language")}>
            <Text style={{ fontSize: 12, color: embedded ? desktopColors.secondary : "#6b7280" }}>
              {t("settings.languageHint")}
            </Text>
            <View style={{ flexDirection: "row", gap: 8 }}>
              <View
                onPress={() => void setLanguage("zh")}
                role="button"
                accessible
                accessibilityState={{ selected: lang() === "zh" }}
                style={{
                  borderRadius: 8,
                  paddingHorizontal: 12,
                  paddingVertical: 6,
                  backgroundColor: lang() === "zh" ? (embedded ? desktopColors.primary : "#7c3aed") : embedded ? desktopColors.low : "#f3f4f6",
                }}
              >
                <Text style={{ fontSize: 12, color: lang() === "zh" ? "#ffffff" : embedded ? desktopColors.text : "#374151" }}>
                  中文
                </Text>
              </View>
              <View
                onPress={() => void setLanguage("en")}
                role="button"
                accessible
                accessibilityState={{ selected: lang() === "en" }}
                style={{
                  borderRadius: 8,
                  paddingHorizontal: 12,
                  paddingVertical: 6,
                  backgroundColor: lang() === "en" ? (embedded ? desktopColors.primary : "#7c3aed") : embedded ? desktopColors.low : "#f3f4f6",
                }}
              >
                <Text style={{ fontSize: 12, color: lang() === "en" ? "#ffffff" : embedded ? desktopColors.text : "#374151" }}>
                  English
                </Text>
              </View>
            </View>
          </Card>
          <Card title={t("settings.connection")}>
            <Info label={t("settings.status")} value={page().status} />
            <Info label={t("settings.mode")} value={page().mode} />
            <Info label={t("settings.dnsLabel")} value={page().proxyDns.label} />
            <Action label={t("settings.dns.title")} onPress={openDns} />
          </Card>
          {page().autoStart !== undefined ? (
            <Card title={t("settings.startup")}>
              <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
                <Text style={{ fontSize: 12, flex: 1, color: desktopColors.text }}>{t("settings.launchAtLogin")}</Text>
                <Action
                  label={page().autoStart ? t("settings.turnOff") : t("settings.turnOn")}
                  onPress={() => void invoke("settings.autostart.set", { enabled: !page().autoStart })}
                />
              </View>
            </Card>
          ) : null}
          <Card title={t("settings.summaryCard")}>
            <Info label={t("settings.kernelVersion")} value={page().kernelVersion} />
            <Info label={t("settings.config")} value={page().summary} />
            <Info
              label={t("settings.workspace")}
              value={`${page().subscriptions} 订阅 · ${page().nodes} 节点 · ${page().groups} 策略组`}
            />
            <Info label={t("settings.abi")} value={page().abi} />
            <Info label={t("settings.dataDir")} value={page().dataDir} />
          </Card>
          <Card title={t("settings.backup")}>
            <Text style={{ fontSize: 12, color: desktopColors.secondary }}>
              {t("settings.backupHint")}
            </Text>
            <View style={{ flexDirection: "row", gap: 8 }}>
              <Action label={t("settings.copyBackup")} onPress={() => void copyBackup()} />
              <Action label={t("settings.importBackup")} onPress={() => void importBackup()} />
            </View>
          </Card>
          <Card title={t("settings.diagnostics")}>
            <Action label={t("settings.viewLogs")} onPress={() => void invoke("settings.logs")} />
          </Card>
          <UpdateCard />
          <Card title={t("settings.about")}>
            <Text style={{ fontSize: 12, color: desktopColors.text }}>{t("settings.aboutDesktop")}</Text>
            <Text style={{ fontSize: 11, color: desktopColors.secondary }}>
              {t("settings.aboutHint")}
            </Text>
          </Card>
        </Scroll>
      </View>
      {layoutWidth() === 0 || layoutWidth() >= 1100 ? (
      <View
        style={{
          width: 300,
          borderLeftWidth: 1,
          borderLeftColor: desktopColors.outline,
          backgroundColor: desktopColors.low,
        }}
      >
        <Scroll style={{ flex: 1 }} contentStyle={{ padding: 12, gap: 10 }}>
          <Card title={t("settings.status")}>
            <Info label={t("settings.connection")} value={page().status} />
            <Info label={t("settings.installedVersion")} value={page().installedVersion} />
            <Info label={t("settings.updateState")} value={page().update.state} />
          </Card>
          <Card title={t("settings.workspace")}>
            <Info label={t("subs.subscriptions")} value={`${page().subscriptions}`} />
            <Info label={t("nodes.title")} value={`${page().nodes}`} />
            <Info label={t("subs.groups")} value={`${page().groups}`} />
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
