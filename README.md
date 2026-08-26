# Manis

Manis 是一个使用 Rust 与 GPUI 构建的跨平台代理策略工作台，品牌名取自穿山甲属的拉丁学名。它借鉴 Quantumult X 易于理解的“规则 → 策略组 → 节点”工作流，但不复制其界面，也不要求用户先学会编辑 YAML。

![Manis 原生宽屏界面](.impeccable/review/native-wide.png)

## 当前里程碑

- 原生 GPUI 窗口，可在宽、中、窄三档尺寸间自适应
- 一键读取 Mihomo 策略组、节点、规则、延迟与活跃连接；未连接时保留演示状态
- 在配置页验证并导入 HTTP/HTTPS 订阅，重启后自动恢复并列出全部节点
- 独立“节点”工作区按来源分组汇总节点、协议、健康状态与延迟，支持分组折叠、状态筛选和刷新
- 可选择连接外部 Mihomo，或由 Manis 校验并托管一个独立 Mihomo / sing-box 子进程
- 可解释的本地路由预测链，并明确区别于 Mihomo 已观察连接
- 浅色/深色主题、可恢复的系统代理开关与键盘可聚焦控件
- macOS 原生运行和 Metal 离屏截图已验证
- Windows、Linux 已配置对应的 GPUI 平台依赖，仍需各自原生 CI/设备验证

连接外部 Mihomo 时，Manis 保持只读，只请求官方的状态和来源接口，不会切换节点或改写其配置。只有 Manis 自己生成并托管的配置才开放该内核已声明支持的写操作；切换策略前会再次校验分组类型和候选节点。配置页的订阅预览会另起一个短生命周期、隔离目录的 Mihomo，只用于下载并解析待预览的订阅。

### 运行内核

“配置 → 运行内核”可以在 Mihomo 与 sing-box 之间切换。选择会保存到平台用户数据目录的 `kernel.kind`；切换采用“生成配置 → 官方内核预检 → 停止 Manis 持有的旧进程 → 保存选择”的顺序，失败时保留原内核。Manis 不会停止或改写 Clash Verge 等其他应用持有的进程。

Mihomo 仍是默认且功能完整的内核。当前 sing-box 适配器支持已保存的单个 VLESS TCP/TLS/Reality 节点、手动选择、自动延迟选择、规则/直连/全局模式、全局节点选择、系统 HTTP/SOCKS 代理与受密钥保护的本机 Clash API。Mihomo `proxy-provider` 订阅、故障转移、负载均衡和 TUN 尚未被静默近似；存在这些需求时，界面会明确禁用切换并说明原因。

Manis 会从 `MANIS_SING_BOX_BINARY`、`PATH` 以及 macOS Homebrew 常见位置寻找 sing-box。生成的 `manis-generated.json` 和控制器密钥只写入权限受限的 Manis runtime，密钥不会进入进程参数或日志。

## 运行

仓库通过 `rust-toolchain.toml` 固定 Rust 工具链。macOS 需要 Xcode Command Line Tools；Linux 还需要 Wayland/X11 的系统开发库。

```bash
cargo run -p manis-ui
```

首次从旧品牌版本启动时，Manis 会在确认旧目录是真实目录而非符号链接后，将其中的订阅、规则、节点选择、日志和系统代理恢复文件整体迁移到新的 Manis 用户数据目录。若迁移无法安全完成，旧目录会原样保留，不会被删除或覆盖。新的环境变量统一使用 `MANIS_*`；对应的旧 `RELAY_*` 运行变量暂时仍可作为低优先级兼容入口，若两者同时存在则以 `MANIS_*` 为准。

没有保存来源时，Manis 默认连接 `http://127.0.0.1:9090`。存在已保存的订阅或 VLESS 节点、且没有显式设置外部 controller 时，Manis 会自动寻找 Mihomo、生成私有配置并进入托管模式。如果 Mihomo 配置了 `secret`，或控制器使用了其他本机回环端口，可通过环境变量强制连接外部内核：

```bash
MANIS_MIHOMO_CONTROLLER=http://127.0.0.1:9090 \
MANIS_MIHOMO_SECRET='your-controller-secret' \
cargo run -p manis-ui
```

### 调试交互日志

设置 `MANIS_UI_TRACE=debug` 可在终端输出结构化的界面与连接生命周期事件：

```bash
MANIS_UI_TRACE=debug cargo run -p manis-ui
```

日志格式为 `manis_ui ts_ms=... level=DEBUG event=...`。它只记录固定事件名，例如订阅源诊断展开、规则预览和 Mihomo 连接成功/失败；不会记录订阅 URL、token、本机路径、节点名或控制器错误正文。

这一里程碑的标准库 HTTP 传输只允许 `localhost`、IPv4/IPv6 回环地址，避免通过明文网络泄露控制器密钥。macOS/Linux 也可直接连接 Clash Verge Rev 等应用暴露的 Unix socket：

```bash
MANIS_MIHOMO_CONTROLLER=unix:///tmp/verge/verge-mihomo.sock \
cargo run -p manis-ui
```

Unix socket 依赖操作系统文件权限，Manis 会确认目标确实是 socket 且不是符号链接，并且不会向它转发 `MANIS_MIHOMO_SECRET`。远程控制器、HTTPS 和 Windows named pipe 尚未支持。Mihomo 需要先启用 [`external-controller`](https://wiki.metacubex.one/en/config/general/) 或 `external-controller-unix`；接口形状参考其[官方 API 文档](https://wiki.metacubex.one/en/api/)。

### Manis 托管 Mihomo

导入至少一个订阅或 VLESS 节点后，正常启动 Manis 即可准备托管 Mihomo。Manis 依次查找应用同目录的 `mihomo`、`PATH`、macOS Homebrew 常见位置；macOS 开发环境还会回退到 Clash Verge 自带的 `verge-mihomo`。可以只设置 `MANIS_MIHOMO_BINARY` 来覆盖自动发现，同时继续使用 Manis 已保存的来源。

需要使用现有 YAML 时，同时提供 Mihomo 可执行文件和配置文件。Manis 会先运行 `mihomo -t` 校验配置，再以独立数据目录和 controller 启动子进程；应用退出时只清理自己持有的子进程，不扫描 PID、端口，也不会触碰 Clash Verge：

```bash
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo \
MANIS_MIHOMO_CONFIG=/absolute/path/to/config.yaml \
MANIS_MIHOMO_DATA_DIR=/absolute/path/to/manis-runtime \
cargo run -p manis-ui
```

macOS/Linux 默认在平台用户数据目录内创建权限为 `0700` 的 Manis 专用 runtime，并使用目录内的 Unix socket。托管 TCP 暂不开放，因为 Manis 还不能验证用户 YAML 是否真的启用了同值 controller secret；外部 loopback TCP controller 不受影响。密钥不会进入 Mihomo 的进程参数。Windows named-pipe 启动参数已经建模，但 Manis 的 named-pipe controller transport 尚未完成，因此 Windows 当前仍使用外部 loopback HTTP controller。

### 从私有订阅生成 QX 风格配置（显式开发模式）

Manis 现在可以把一个 HTTPS 订阅编译为最小 Mihomo 配置：订阅作为 `proxy-provider`，`Proxy` 提供手动选择，`Auto` 进行延迟优选，规则按顺序执行并由 `MATCH,Proxy` 兜底。订阅由 Mihomo 获取，Manis 不下载或解析节点内容。

先在仓库之外创建只包含一行订阅 URL 的文件，并把权限设为 `0600`。Manis 会拒绝相对路径、符号链接、多行/非 HTTPS 内容、超过 16 KiB 的文件和组/其他用户可读的权限。不要把 URL 直接写进命令行或环境变量：

```bash
install -m 600 /dev/null /absolute/path/to/manis.subscription.secret
${EDITOR:?set EDITOR} /absolute/path/to/manis.subscription.secret
```

然后提供 Mihomo 可执行文件和该私有文件路径；它与 `MANIS_MIHOMO_CONFIG` 模式互斥：

```bash
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo \
MANIS_MIHOMO_SUBSCRIPTION_FILE=/absolute/path/to/manis.subscription.secret \
MANIS_MIHOMO_DATA_DIR=/absolute/path/to/manis-runtime \
cargo run -p manis-ui
```

生成的 `manis-generated.yaml` 位于 Manis runtime，macOS/Linux 上目录为 `0700`、文件为 `0600`，写入采用同目录临时文件替换；错误和 Debug 输出不会包含订阅内容。默认 mixed port 是回环地址上的 `17890`，可用 `MANIS_MIHOMO_MIXED_PORT` 显式覆盖。Windows 托管订阅模式会在读取/写入订阅前明确失败，等待 named-pipe transport 与私有 ACL 存储完成；profile 领域模型和 YAML 编译器本身保持平台无关。

### 在应用里输入订阅链接

打开“配置 → 订阅源”，链接输入框会直接显示并自动获得焦点。入口可识别 HTTP/HTTPS 订阅地址以及单个 `vless://` 节点链接；点击“导入订阅”后，HTTP/HTTPS 订阅会先由隔离的 Mihomo 实际下载和解析，只有成功返回节点后才原子保存。页面显示真实来源数、节点总数，并在可滚动列表中列出每个节点的名称与协议。HTTP 会显示明文风险提示；`vless://` 当前仍只做不含 UUID 的安全节点预览，因此按钮会明确显示“预览 VLESS 节点”。

预览进程使用权限为 `0700` 的临时目录，配置和 Mihomo 缓存在预览结束后删除；日志和错误不会包含订阅 URL。Manis 会依次查找 `MANIS_MIHOMO_PREVIEW_BINARY`、应用同目录的 `mihomo`、`PATH`，并在 macOS 上回退到 Clash Verge 的内置 Mihomo。需要显式指定时可这样启动：

```bash
MANIS_MIHOMO_PREVIEW_BINARY=/absolute/path/to/mihomo cargo run -p manis-ui
```

导入的订阅写入平台用户数据目录：macOS 为 `~/Library/Application Support/Manis/subscriptions`，Linux 为 `$XDG_DATA_HOME/manis/subscriptions`（缺省回退到 `~/.local/share/manis/subscriptions`）。目录和文件分别强制为 `0700`、`0600`，写入使用同目录临时文件原子替换；加载时拒绝符号链接、宽松权限、超限、多行或损坏内容。应用不会把恢复的 URL/token 回填到可见输入框，调试日志和错误也不包含它。重新导入只有在新订阅验证成功后才替换旧订阅，“移除订阅”只删除这一个持久来源。

节点工作区把同一订阅解析出的节点放在同一个可折叠分组中。分组名只读取订阅 URL 中显式、长度受限且不含控制字符的 `name` 参数；没有安全名称时显示固定的“订阅 1”，不会用域名、路径或 token 兜底。单个 `vless://` 当前仍只支持安全预览，后续持久保存时会进入独立的“已保存”分组，不会混入订阅组。

macOS/Linux 会把持久订阅、VLESS、节点分组和受支持的 QX 规则编译进 Manis 托管配置。开启系统代理前，Manis 会先以私有文件保存原系统设置；正常退出会恢复，异常退出则在下次启动时恢复。真实 `networksetup`、流量命中以及 TUN 路由/DNS 仍应在目标机器上人工验收。Windows 会明确拒绝持久导入，直到 owner-only DACL、可靠原子替换和 named-pipe 控制器传输全部完成，不会把继承 ACL 的普通文件误称为私有存储。

## 验证

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

macOS 可额外生成全部原生视觉与交互状态截图：

```bash
cargo run -p manis-ui --example snapshot
```

输出位于 `.impeccable/review/native-*.png`。截图直接来自 GPUI 渲染纹理，不依赖系统录屏权限。

真实控制器 smoke test 默认忽略，只有显式提供 endpoint 才会运行：

```bash
MANIS_MIHOMO_CONTROLLER=unix:///tmp/verge/verge-mihomo.sock \
cargo test -p manis-ui reads_a_live_controller_snapshot -- --ignored
```

隔离订阅导入也有默认忽略的真实 Mihomo 集成测试，可显式提供测试二进制运行；测试使用本地两节点订阅 fixture，覆盖导入、关闭后的持久恢复和再次解析，不请求私人订阅：

```bash
MANIS_MIHOMO_TEST_BINARY=/absolute/path/to/mihomo \
cargo test -p manis-ui real_mihomo_previews_all_nodes_from_a_subscription -- --ignored
```

可选 live screenshot 还要求 `MANIS_MIHOMO_LIVE_SCREENSHOT` 指向系统临时目录，工具会拒绝把真实节点信息写进仓库。

## 代码结构

- `crates/manis-core`：与渲染框架无关的窗口尺寸、内核能力、策略选择和路由证据状态
- `crates/manis-engine`：隔离路径、命令计划、配置预检、就绪探测与 owned-child 生命周期
- `crates/manis-profile`：QX 风格 profile 领域模型、Mihomo YAML / sing-box JSON 编译与私有原子写入
- `crates/manis-mihomo`：受限控制器配置、有界 HTTP 传输、容错 JSON 模型和领域目录映射
- `crates/manis-ui`：GPUI 应用、主题、演示/控制器数据与自适应视图
- `DESIGN.md`：视觉 token、组件和响应式行为
- `GPUI_IMPLEMENTATION.md`：状态边界、事件流和未来 Mihomo API 映射
- `packaging`：各平台打包元数据的起点

GPUI 与 `gpui_platform` 固定到同一个 Zed 提交，避免跟随 `main` 漂移。后续优先补齐持续刷新、Windows/Linux 原生 CI 和安全写入命令，再考虑真实节点切换与系统代理控制。
