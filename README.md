# Manis

[![CI](https://github.com/kaigedong/Manis/actions/workflows/ci.yml/badge.svg)](https://github.com/kaigedong/Manis/actions/workflows/ci.yml)
[![Security](https://github.com/kaigedong/Manis/actions/workflows/security.yml/badge.svg)](https://github.com/kaigedong/Manis/actions/workflows/security.yml)
[![Package](https://github.com/kaigedong/Manis/actions/workflows/package.yml/badge.svg)](https://github.com/kaigedong/Manis/actions/workflows/package.yml)
[![License](https://img.shields.io/badge/source-Apache--2.0-blue.svg)](LICENSE)

**简体中文** · [English](README.en.md)

Manis 是一个使用 Rust 和 GPUI 编写的小型实验项目。它目前只聚焦一件事：把“有序规则
→ 策略组 → 节点”的路径展示清楚，并为一部分常用的 Mihomo 和 sing-box 操作提供界面。

这是一个个人开源尝试，不是对代理工具的重新定义，也无意取代成熟客户端。现有工具已经能
很好地服务大多数用户；Manis 主要是尝试一种维护者自己觉得更容易理解的工作方式。

> [!WARNING]
> Manis 目前仍是 alpha 软件。macOS 是主要开发和运行验证平台；Windows 与 Linux 已作为
> 编译目标维护，但仍需要更多真实设备测试。请不要在生产机器上把 Manis 当成唯一的网络
> 恢复手段。

## 项目范围

Manis 只尝试把一条路由链路直接呈现出来：

```text
有序规则 -> 策略组 -> 最终节点
```

- 分流规则决定一条连接进入哪个策略组。
- 手动策略组使用用户在组内选中的节点。
- 自动策略组测速后，按照配置的策略自动选择候选节点。
- 全局模式使用“节点”页面中激活的全局出口。
- 网络活动明确区分内核已经观察到的事实和本地规则预测。

这个模型刻意保持简单，不会适合所有代理配置。项目参考了 Quantumult X 较易理解的
策略工作流，但不复制其界面或配置格式。

## 目前实现

- 原生自适应 GPUI 界面，支持中英文、浅色和深色模式。
- HTTP/HTTPS 订阅与 VLESS 导入，并在本地进行私有持久化。
- 手动、延迟优选、故障转移和负载均衡策略模型；不可用能力由内核能力门禁控制。
- 有序分流规则、QX 规则列表导入、域名与端口组合匹配、明确的兜底规则。
- 订阅来源和策略组测速，逐节点增量展示结果。
- 直连、全局和规则三种路由模式。
- 系统 HTTP/SOCKS 代理控制与 macOS TUN 集成。
- 活跃连接、真实路由证据、内核日志和脱敏的应用诊断。
- 以 Mihomo 为主要托管内核，并提供按能力启用的 sing-box 适配器。

Manis 源码仓库不提交预编译代理内核。发行构建会从 Mihomo 官方 Release 下载对应架构的
稳定版，校验上游 SHA-256 后作为首次启动种子；应用之后只使用并更新自己托管的内核。

## 平台状态

| 平台 | 编译目标 | 运行状态 |
| --- | --- | --- |
| macOS 13+ | 持续维护 | 主要开发平台；系统代理已验证，TUN 通过管理员授权路径支持测试 |
| Windows | 持续维护 | 实验性；托管 controller transport 尚未完成，暂不能启动 Mihomo |
| Linux | 持续维护 | 实验性；仍需覆盖更多发行版与桌面环境 |

CI 会检查三个平台，但“能够编译”不代表该平台上的所有网络集成都已经完成真实验证。

Package workflow 会分别构建 Apple Silicon、Intel 的未公证 macOS 应用包，以及支持原生
Wayland 的实验性 Arch Linux `x86_64` 软件包，并附带校验文件。每次提交合并到 `main` 后，
工作流会覆盖更新名为 `latest` 的公开滚动 Pre-release；手动运行的 Actions 产物保留 14 天。
滚动构建使用 `0.1.<Package 运行编号>` 作为单调递增版本，并发布由 GitHub Release 摘要
保护的更新清单。通过这些测试包装入的 Manis 会每小时在后台检查和校验更新；下载完成后，
可在“设置 → 通用”或底部状态栏点击“重启并更新”。Arch/CachyOS 安装时会通过 polkit 请求
一次管理员授权，以便继续由 pacman 管理软件包。
推送任意版本标签时，工作流还会创建独立的 Draft Release，待维护者完成发布清单后再决定
是否公开。安装前请阅读
[macOS](packaging/macos/README.md) 与 [Arch Linux](packaging/archlinux/README.md) 打包说明。

## 下载测试构建

可以直接从 [Releases 页面](https://github.com/kaigedong/Manis/releases) 中名为
`Latest Manis development build` 的 Pre-release 下载最近一次成功合并到 `main` 的测试
产物，也可以在 [GitHub Actions 的 Package 页面](https://github.com/kaigedong/Manis/actions/workflows/package.yml)
下载单次运行产物。Apple Silicon Mac 选择 `arm64`，Intel Mac 选择 `x86_64`；CachyOS 和
Arch Linux 使用 `.pkg.tar.zst` 软件包。版本标签产生的正式候选包仍只保存为 Draft Release。

这些 macOS 包经过 ad-hoc 签名，但没有 Developer ID 签名和 Apple 公证，只适合测试。
macOS 首次打开仍会显示 Gatekeeper 提示；请只使用可信的官方 Release，并同时取得
`.sha256` 文件校验压缩包。GitHub 下载版默认支持 TUN，但首次启用 TUN，以及每次更换
Manis 应用版本后再次启用 TUN，都需要管理员授权。该路径由 root-owned LaunchDaemon 固定
批准时的应用、`manis-helperctl`、特权 helper 指纹和当前用户 ID，不需要付费 Apple
开发者账号。Developer ID/SMAppService 签名路径仍作为维护者可选发布方式保留。

## 从源码运行

仓库通过 `rust-toolchain.toml` 固定 Rust 工具链，并在 `crates/manis-ui/Cargo.toml` 中固定
GPUI 版本。macOS 需要 Xcode Command Line Tools；Linux 需要 GPUI 使用的 Wayland/X11、
fontconfig 与 ALSA 开发包。Linux 托盘通过会话 D-Bus 使用 StatusNotifierItem 协议，
不依赖 GTK 3；GNOME 需要启用 AppIndicator 扩展才能显示托盘。

```bash
git clone https://github.com/kaigedong/Manis.git
cd Manis
cargo run -p manis-ui
```

Manis 只启动自己管理的 Mihomo 进程，并且只使用自己从界面数据生成的配置。它不会连接
其他程序启动的 controller，也不会运行用户提供的 Mihomo YAML。发行包携带一个经过
SHA-256 校验的官方稳定版种子内核；首次运行会安装到 Manis 私有数据目录，之后由应用内
更新器负责下载、校验、版本验证、原子替换和失败回滚。

未添加节点时，Manis 会准备一个只含 `DIRECT` 兜底的空配置；添加订阅或单独节点后，配置由
Manis 校验并写入私有运行目录。controller endpoint 也由 Manis 分配，不作为用户配置项。

## 开发验证

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo check -p manis-ui --example snapshot --features snapshot-fixtures --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

真实内核测试默认忽略，必须通过环境变量显式开启，并使用合成测试数据。
私人订阅不能进入 Git 或公开测试输出。完整贡献流程见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 迁移到另一台设备

在「设置 → 通用 → 备份与迁移」中选择「导出完整配置」，将生成的 `.manis.json` 文件
传到另一台设备，再使用「从文件导入」。导出和文件导入只打开系统文件选择窗口，不会叠加
「备份与迁移」弹窗；操作进度和结果显示在底部状态栏，不改变设置卡片高度。
文件选好并通过校验后，导入才显示预览确认。
「修改配置」会在文本编辑窗口中打开当前完整配置，可直接修改或粘贴其他 `.manis.json` 文件
的完整内容；存在失效策略组引用时也能打开修复。点击「校验并预览」检查后再确认应用；
无效配置不能应用，也可以返回继续修改，取消不会改变配置。
这是 Manis 的备份格式，不是可直接交给其他代理客户端的 Mihomo YAML。

备份包含订阅链接、单独保存的节点、策略组、规则来源及其内容、手动规则、路由和节点选择、
界面语言及内核选择。内核程序、系统权限、正在使用的代理模式、日志和测速结果不会迁移。
订阅节点会在另一台设备上重新加载，需要能访问订阅地址。

导入会先校验并显示数量；确认「替换并重启」后，Manis 会关闭代理并停止内核，备份原配置，
完整替换配置并重启。导入不会合并，也不会自动开启系统代理或 TUN。原配置保存在用户数据目录
的 `configuration-backups` 中，可通过「查看自动备份」找到并重新导入。

**配置文件含明文订阅凭据和节点密码，请私下传输并妥善保管，勿提交到 Git 或公开分享。**

## 策略组的动态 Proxy 出口

添加或编辑策略组时，在「节点范围 → 选择节点或策略组」中勾选 **Proxy**，即可跟随
节点页面的手动选择。之后在首页换节点，新连接会走新的出口，无需重新编辑策略组；已有连接
不会被强制断开。手动和自动策略组都支持这个候选项。自动策略组还可设置内核原生的毫秒切换
余量，减轻小幅延迟波动造成的切换；本项目不附加自定义内核补丁。

侧栏使用内置 SVG 图标，宽窗口保留文字，窄窗口悬停可查看完整名称。控制器连接异常统一显示
在状态栏。英文说明和复现命令见 [开发文档](docs/development.md#portable-configuration-and-policy-selection)。

策略组的自动选择和手动选择统一使用中性色高亮及「当前出口」标签，不再显示前置圆圈或对号。
配置页的代理来源和规则来源使用整行悬停底色，左侧启用框相对整条内容垂直居中。
策略组按内容向下展开，超出窗口后滚动，不会挤压其他策略组。分流规则页将说明、规则统计和
添加按钮集中在顶部，下方直接展示规则组；组标题悬停显示手形光标。日志编号、级别、内容和
时间在每行中垂直居中。复现命令见 [列表布局检查](docs/development.md#list-layout-checks)。

## 安全与隐私

订阅链接、token、节点凭据、controller secret 和生成的内核配置都属于私有数据。Manis 将
其保存到平台用户数据目录，并从自身诊断日志中脱敏。明文 HTTP 订阅在网络上天然可见，应
优先使用 HTTPS。

macOS TUN 使用固定用途的特权 helper。GitHub ad-hoc 包通过管理员授权固定本版本代码指纹；
旧的 `MANIS_ALLOW_INSECURE_LOCAL_HELPER` 本地调试绕过已经废弃，不能用于发布。
测试特权能力前请阅读 [SECURITY.md](SECURITY.md)，安全漏洞请通过 GitHub 私有漏洞报告提交。

## 仓库结构

| 路径 | 职责 |
| --- | --- |
| `crates/manis-core` | 与内核无关的策略、路由证据和应用状态 |
| `crates/manis-engine` | 内核发现、校验、进程所有权和生命周期 |
| `crates/manis-profile` | 类型化配置，以及 Mihomo/sing-box 配置编译 |
| `crates/manis-mihomo` | 受限的 Mihomo controller 传输和领域映射 |
| `crates/manis-ui` | GPUI 应用、持久化边界和平台集成 |
| `packaging/macos` | macOS 打包与固定用途的特权 helper |
| `packaging/archlinux` | 支持 Wayland 的实验性 Arch Linux 软件包 |
| `docs` | 架构、设计和维护者文档 |

相关文档：

- [产品原则](PRODUCT.md)
- [设计系统](DESIGN.md)
- [GPUI 实现说明](GPUI_IMPLEMENTATION.md)
- [开发指南（英文）](docs/development.md)
- [直连与组合规则](docs/architecture/direct-rules.md)
- [macOS 打包](packaging/macos/README.md)
- [发布检查清单](docs/maintainers/release-checklist.md)

## 许可证

Manis 源码使用 [Apache License 2.0](LICENSE)。Rust 链接依赖和可选代理内核拥有各自的
许可证，其中包含 GPL 组件。本仓库不存放预编译代理内核；分发二进制前必须阅读
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
