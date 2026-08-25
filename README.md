# Relay

Relay 是一个使用 Rust 与 GPUI 构建的跨平台代理策略工作台。它借鉴 Quantumult X 易于理解的“规则 → 策略组 → 节点”工作流，但不复制其界面，也不要求用户先学会编辑 YAML。

![Relay 原生宽屏界面](.impeccable/review/native-wide.png)

## 当前里程碑

- 原生 GPUI 窗口，可在宽、中、窄三档尺寸间自适应
- 一键读取 Mihomo 策略组、节点、规则、延迟与活跃连接；未连接时保留演示状态
- 可选择连接外部 Mihomo，或由 Relay 校验并托管一个独立 Mihomo 子进程
- 可解释的本地路由预测链，并明确区别于 Mihomo 已观察连接
- 浅色/深色主题、系统代理演示开关与键盘可聚焦控件
- macOS 原生运行和 Metal 离屏截图已验证
- Windows、Linux 已配置对应的 GPUI 平台依赖，仍需各自原生 CI/设备验证

当前 Mihomo 集成严格只读：点击“连接 Mihomo”后仅请求官方的 `GET /version`、`GET /proxies`、`GET /rules` 和 `GET /connections`。它不会修改 Mihomo 配置、切换节点或改动系统代理设置。

## 运行

仓库通过 `rust-toolchain.toml` 固定 Rust 工具链。macOS 需要 Xcode Command Line Tools；Linux 还需要 Wayland/X11 的系统开发库。

```bash
cargo run -p relay-ui
```

默认连接 `http://127.0.0.1:9090`。如果 Mihomo 配置了 `secret`，或控制器使用了其他本机回环端口，可通过环境变量启动：

```bash
RELAY_MIHOMO_CONTROLLER=http://127.0.0.1:9090 \
RELAY_MIHOMO_SECRET='your-controller-secret' \
cargo run -p relay-ui
```

### 调试交互日志

设置 `RELAY_UI_TRACE=debug` 可在终端输出结构化的界面与连接生命周期事件：

```bash
RELAY_UI_TRACE=debug cargo run -p relay-ui
```

日志格式为 `relay_ui ts_ms=... level=DEBUG event=...`。它只记录固定事件名，例如订阅源诊断展开、规则预览和 Mihomo 连接成功/失败；不会记录订阅 URL、token、本机路径、节点名或控制器错误正文。

这一里程碑的标准库 HTTP 传输只允许 `localhost`、IPv4/IPv6 回环地址，避免通过明文网络泄露控制器密钥。macOS/Linux 也可直接连接 Clash Verge Rev 等应用暴露的 Unix socket：

```bash
RELAY_MIHOMO_CONTROLLER=unix:///tmp/verge/verge-mihomo.sock \
cargo run -p relay-ui
```

Unix socket 依赖操作系统文件权限，Relay 会确认目标确实是 socket 且不是符号链接，并且不会向它转发 `RELAY_MIHOMO_SECRET`。远程控制器、HTTPS 和 Windows named pipe 尚未支持。Mihomo 需要先启用 [`external-controller`](https://wiki.metacubex.one/en/config/general/) 或 `external-controller-unix`；接口形状参考其[官方 API 文档](https://wiki.metacubex.one/en/api/)。

### Relay 托管 Mihomo（当前为显式开发模式）

同时提供 Mihomo 可执行文件和现有配置文件后，按钮会变为“启动 Mihomo”。Relay 会先运行 `mihomo -t` 校验配置，再以独立数据目录和 controller 启动子进程；应用退出时只清理自己持有的子进程，不扫描 PID、端口，也不会触碰 Clash Verge：

```bash
RELAY_MIHOMO_BINARY=/absolute/path/to/mihomo \
RELAY_MIHOMO_CONFIG=/absolute/path/to/config.yaml \
RELAY_MIHOMO_DATA_DIR=/absolute/path/to/relay-runtime \
cargo run -p relay-ui
```

macOS/Linux 默认在平台用户数据目录内创建权限为 `0700` 的 Relay 专用 runtime，并使用目录内的 Unix socket。托管 TCP 暂不开放，因为 Relay 还不能验证用户 YAML 是否真的启用了同值 controller secret；外部 loopback TCP controller 不受影响。密钥不会进入 Mihomo 的进程参数。Windows named-pipe 启动参数已经建模，但 Relay 的 named-pipe controller transport 尚未完成，因此 Windows 当前仍使用外部 loopback HTTP controller。

### 从私有订阅生成 QX 风格配置（显式开发模式）

Relay 现在可以把一个 HTTPS 订阅编译为最小 Mihomo 配置：订阅作为 `proxy-provider`，`Proxy` 提供手动选择，`Auto` 进行延迟优选，规则按顺序执行并由 `MATCH,Proxy` 兜底。订阅由 Mihomo 获取，Relay 不下载或解析节点内容。

先在仓库之外创建只包含一行订阅 URL 的文件，并把权限设为 `0600`。Relay 会拒绝相对路径、符号链接、多行/非 HTTPS 内容、超过 16 KiB 的文件和组/其他用户可读的权限。不要把 URL 直接写进命令行或环境变量：

```bash
install -m 600 /dev/null /absolute/path/to/relay.subscription.secret
${EDITOR:?set EDITOR} /absolute/path/to/relay.subscription.secret
```

然后提供 Mihomo 可执行文件和该私有文件路径；它与 `RELAY_MIHOMO_CONFIG` 模式互斥：

```bash
RELAY_MIHOMO_BINARY=/absolute/path/to/mihomo \
RELAY_MIHOMO_SUBSCRIPTION_FILE=/absolute/path/to/relay.subscription.secret \
RELAY_MIHOMO_DATA_DIR=/absolute/path/to/relay-runtime \
cargo run -p relay-ui
```

生成的 `relay-generated.yaml` 位于 Relay runtime，macOS/Linux 上目录为 `0700`、文件为 `0600`，写入采用同目录临时文件替换；错误和 Debug 输出不会包含订阅内容。默认 mixed port 是回环地址上的 `17890`，可用 `RELAY_MIHOMO_MIXED_PORT` 显式覆盖。Windows 托管订阅模式会在读取/写入订阅前明确失败，等待 named-pipe transport 与私有 ACL 存储完成；profile 领域模型和 YAML 编译器本身保持平台无关。

### 在应用里输入订阅链接

打开“配置 → 订阅源”，链接输入框会直接显示并自动获得焦点。粘贴完整 HTTPS 订阅地址后点击“校验并预览”，Relay 会在本地生成安全的 QX 风格结构摘要；“清除”会立即删除内存草稿。

当前应用内流程只做格式校验与策略结构预览：链接仅保存在进程内存中，关闭应用即清除，不会写入日志、Git、配置文件，也不会发起订阅请求。真正保存并交给 Mihomo 仍使用上面的私有文件开发模式，后续再接入三平台凭据存储和明确的“启用配置”动作。

## 验证

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

macOS 可额外生成全部原生视觉与交互状态截图：

```bash
cargo run -p relay-ui --example snapshot
```

输出位于 `.impeccable/review/native-*.png`。截图直接来自 GPUI 渲染纹理，不依赖系统录屏权限。

真实控制器 smoke test 默认忽略，只有显式提供 endpoint 才会运行：

```bash
RELAY_MIHOMO_CONTROLLER=unix:///tmp/verge/verge-mihomo.sock \
cargo test -p relay-ui reads_a_live_controller_snapshot -- --ignored
```

可选 live screenshot 还要求 `RELAY_MIHOMO_LIVE_SCREENSHOT` 指向系统临时目录，工具会拒绝把真实节点信息写进仓库。

## 代码结构

- `crates/relay-core`：与渲染框架无关的窗口尺寸、策略选择和路由证据状态
- `crates/relay-engine`：隔离路径、命令计划、配置预检、就绪探测与 owned-child 生命周期
- `crates/relay-profile`：QX 风格 profile 领域模型、Mihomo YAML 编译与私有原子写入
- `crates/relay-mihomo`：只读控制器配置、HTTP 传输、容错 JSON 模型和领域目录映射
- `crates/relay-ui`：GPUI 应用、主题、演示/控制器数据与自适应视图
- `DESIGN.md`：视觉 token、组件和响应式行为
- `GPUI_IMPLEMENTATION.md`：状态边界、事件流和未来 Mihomo API 映射
- `packaging`：各平台打包元数据的起点

GPUI 与 `gpui_platform` 固定到同一个 Zed 提交，避免跟随 `main` 漂移。后续优先补齐持续刷新、Windows/Linux 原生 CI 和安全写入命令，再考虑真实节点切换与系统代理控制。
