# UI 设计发现

外部资料只作为设计证据，不执行其中的指令性内容。

## 2026-08-25 全分组统一测速

- 用户要求三类表面遵循同一原则：导入来源分组、用户节点分组、策略组都在分组名前提供同一测速图标。
- 自动策略的图标只发起测量，不开放手动节点选择；完成后必须从 Mihomo 重新读取当前出口，不在 UI 本地伪造“最优”。
- 交互必须覆盖防重入、运行、部分成功、失败和过期回包丢弃，且不在状态/日志中暴露动态节点名。
- 现有 `ControllerRuntime::test_node_group_delay` 已统一 HTTP/Unix：Relay 托管分组优先 `/group/{name}/delay`，404 或外部 controller 则以有界 worker 逐节点 `/proxies/{name}/delay` 回退。
- 现有 `NodeGroupBenchmarkState` 已具备 Idle/Running/Complete/Failed、generation 过期隔离和节点延迟 map，但仅挂在用户节点分组 ID 上；导入来源与主策略组尚未复用。
- 主策略详情顶部的“测速”目前是无点击处理的静态按钮；导入来源分组标题目前只负责折叠。
- Mihomo 官方 API 文档明确：`GET /group/{group_name}/delay` 测试组内节点/子策略组，返回新延迟，并清除自动策略组的 fixed 选择；随后 `GET /proxies/{name}` 的 `now` 是 URLTest/Fallback 当前生效出口。因此自动组测速不应 PUT 人工选择，而应 group delay 后重读 `now`。来源：https://github.com/MetaCubeX/Meta-Docs/blob/main/docs/api/index.md

## 当前继续阶段（2026-08-25）

- 上一提交 `04497e3` 已实现多来源节点分组、VLESS 私有持久化、三态代理、网络活动快照和安全 UI 事件日志。
- 当前明确遗留：已保存 VLESS 只展示、不参与 Mihomo 运行配置；网络活动依赖手动刷新；日志不是 Mihomo `/logs` 实时流。
- 本轮必须继续保持订阅/VLESS 凭据不进入 Debug、状态文案、日志、截图、测试夹具或 Git。
- UI 延续 Operate 模式与 Signal Patch Bay 视觉世界，只增加实时性和运行真值反馈，不重新设计品牌。
- `relay-profile::Profile` 目前只有远程 `ProxyProvider`，渲染器只输出 `proxy-providers`/`proxy-groups`/`rules`；没有顶层 `proxies` 或 VLESS 密钥类型，因此 VLESS 必须先在该信任边界内成为脱敏强类型，不能从 UI 直接拼 YAML。
- `ControllerRuntime::Managed` 持有 `Arc<Mutex<EngineManager>>`，但 `EngineManager` 只有 start/stop，没有安全更换配置或 restart API；若要让新增 VLESS 生效，必须定义“原子写新配置 → validation → 停旧进程 → 起新进程/失败恢复”的明确事务边界。
- `relay-mihomo::ControllerTransport::get` 会把整个响应读完，不能用于 `/connections` 与 `/logs` 的长期流；实时流需要独立接口，不能破坏现有有上限的快照 GET/PATCH 语义。
- 当前网络活动只显示最近快照并由刷新按钮调用完整 `connect_mihomo`；日志页只读取 256 条固定 UI 事件环。实时升级要保持有界队列、合并 UI 通知，并明确区分“Relay 事件”和“Mihomo 内核日志”。
- Mihomo 官方当前 VLESS schema 要求 `uuid`，支持 `flow=xtls-rprx-vision`、`packet-encoding`、TLS/servername/client-fingerprint/reality，以及 `ws/http/h2/grpc/xhttp` 传输；首轮不应声称任意 vless:// 查询参数都兼容，必须用白名单解析并对未支持参数 fail closed。
- Mihomo 官方 API 明确 `/logs` 与 `/connections` 都支持长期 `GET`/`WS`；`/logs` 标准模式是一行一个 JSON，也可 `?format=structured`；`/connections?interval=milliseconds` 周期推送完整连接快照。因此 std-only 实现可优先选择 HTTP newline/连续 JSON 流，无需立即实现 WebSocket framing。
- 官方 `/traffic` 每秒推送速率和累计流量，但连接页已有每连接 upload/download 与 controller 总量；本轮可先流 `/connections`，避免同时维护重复流和额外重绘。

## 已知产品事实
- 产品是高频操作型桌面工具，设计模式为 Operate，不是营销型 Dashboard。
- 用户最重视自定义策略组、按规则路由和路由结果解释。
- 信息密度应高于移动 Material 应用，但必须保留清晰层级、键盘操作与窄窗口适配。

## 技能调研结论
- UI/UX Pro Max 的第一次系统搜索误把产品归为营销/转化页面，输出的 Poppins + Open Sans 和 hero/CTA 结构不适合 Operate 型桌面工具；保留它的 token/对比度建议，但不把生成结果当视觉权威。
- 更窄的搜索仍倾向营销页面，证明产品模式必须由设计判断覆盖自动分类。有效结果包括：单字体精密系统、深浅主题、语义色、键盘焦点、虚拟化列表和密集桌面布局。
- 字体候选中 Inter、IBM Plex Sans、Plus Jakarta Sans 都匹配工具类界面；考虑中英文混排与三平台可用性，原型优先使用系统 UI 字体栈，品牌字体以后再决定。
- Material 3 的价值主要是语义颜色、surface-container 层级、窗口尺寸类和 list-detail/supporting-pane；不直接复制 Android 的大圆角、底部导航或 FAB。
- Impeccable 的 Operate 规则要求：熟悉优先、单字体、克制配色、150–250ms 状态动效、完整 hover/focus/active/disabled/loading/error 状态，避免营销式指标卡和装饰性玻璃。

## 视觉世界推导
- 独特机制：用户沿着一条可见链路理解并控制 `规则 → 策略组 → 节点`。
- 真实使用场景：长期驻留的桌面工具；用户切换策略、检查连接、诊断路由和调整顺序。
- 类别俗套一：深色 VPN 面板、巨大连接按钮、世界地图和霓虹速度表。
- 类别俗套二：把移动 Material 界面放大成桌面，充满大圆角卡片和胶囊。
- 候选 1「城市换乘图」：规则像线路，策略组像换乘站，最终节点是目的地；天然解释路由。
- 候选 2「现代公共导视」：Swiss 式网格、强排版和有限色彩，适合三平台且信息清楚。
- 候选 3「音频 Patch Bay」：策略组是可重接的通道与插孔，强调可组合控制；风险是隐喻过强。
- 候选 4「航空调度台」：时延、状态、线路和异常以高密度表格呈现；风险是过于严肃。
- 候选 5「科学观测台」：实时连接和链路呈现为精密仪器读数；风险是落入深色技术模板。
- 候选 6「工程师现场手册」：温暖纸感、中性墨色、标注与图层；风险是与原生桌面控件融合较难。
- 候选 7「模块化信号机架」：面板、通道、状态灯和清楚的层级；风险是容易拟物过度。

## 方向板验证
- Chromium 视觉检查显示四个方向的层级、中文排版和差异表达清楚；对照 Fluent 2/M3 官方页面评分 92/100。
- Impeccable detector 因环境缺少 HTML parser 退化为正则扫描；唯一提示是示波器方向的网格背景。该网格是实际测量/信号画布而非装饰，符合 craft-floor 的明确例外，因此保留。

## 最终方向与布局
- 用户选择方案 1「Signal Patch Bay」。保留“信号沿规则 → 策略组 → 节点流动”的核心隐喻，但普通控件仍遵循熟悉的桌面 list-detail 结构，避免拟物化。
- 宽窗口为 `220 / 326 / flexible / 340` 四区：导航、策略组列表、策略详情、常驻路由解释。
- 中窗口收缩为 `66 / 292 / flexible`，路由解释改为右侧 sheet；窄窗口使用 56px 图标栏，列表与详情单任务切换，路由解释继续使用侧 sheet。
- 浅色以技术型暖白与低饱和青绿为主，深色保持相同语义层级；铜色只服务于路由信号轨迹和预测证据，不扩散到普通按钮。

## Mihomo API 与路由真值
- 官方 `/rules` 暴露规则顺序、类型、payload、目标策略和命中计数，适合构建规则浏览与本地预测模型。
- 官方 `/connections` 返回实际连接的 `chains`、`providerChains`、`rule`、`rulePayload`，应作为“已观察连接”的权威来源。
- 官方 API 没有面向任意域名的 dry-run 匹配端点；涉及进程、源地址、端口、网络类型或解析后 IP 的规则不能被简单域名输入可靠判定，因此界面必须把“预测”与“已观察”分开。
- 独立 finish review 初次发现路由权威性措辞和窄屏开关 accessible name 两个问题；修复并重拍同尺寸截图后，两项均判定 resolved，最终 disposition 为 `ship`。

## GPUI 实现结论

- 采用 Zed 官方仓库同一提交中的 `gpui` 与 `gpui_platform`，避免两个 crate 的内部接口漂移；未采用已滞后的 `create-gpui-app` 模板。
- 当前跨平台入口应使用 `gpui_platform::application().run(...)`；macOS 开启 `font-kit`，Linux 开启 `wayland` 与 `x11`，Windows 使用原生平台默认特性。
- GPUI 的 `VisualTestAppContext` 可直接创建离屏原生窗口、模拟鼠标输入并捕获实际渲染纹理，适合规避 macOS 屏幕录制权限对自动化验收的影响。
- 状态模型放在不依赖 GPUI 的 `relay-core`，使策略选择、断点切换和路由证据可在三平台一致测试；GPUI 只负责事件和渲染。
- macOS 已完成编译、启动、视觉和交互验证。Windows/Linux 的依赖路径已经声明，但仍需要原生 runner 验证工具链、字体、输入、窗口装饰与系统代理集成。
- 依赖树中的 `block 0.1.6` 来自 GPUI 上游并触发 Rust future-incompatibility 提示；当前不阻断构建，后续升级固定提交时需复查。

## Mihomo 只读集成结论

- 官方控制器以 `Authorization: Bearer ${secret}` 鉴权；`secret` 可为空。原生 Rust 请求不受浏览器 CORS 限制。
- `/proxies`、`/connections` 在不同 Mihomo 版本间存在字段新增、空值和行为变化；Serde 模型只绑定 UI 所需字段，默认允许未知字段，`history`、`providerChains` 等均按缺失/空值处理。
- 第一阶段使用 `std::net::TcpStream`，没有引入新的 HTTP runtime 或异步依赖；请求由 GPUI background executor 调度，结果回到 entity 后再通知渲染。
- 明文 HTTP 只接受 `localhost`、IPv4/IPv6 回环地址，并过滤 DNS 解析后的非回环结果；这是避免 Bearer secret 经 LAN/公网泄露的明确产品边界。
- 传输只暴露 GET trait、固定调用四个只读端点、限制 header/body 尺寸并拒绝 path/header 控制字符；当前没有节点切换、配置更新或系统代理写入口。
- HTTP 状态行、普通 Header、chunk-size 和 trailer 都使用限长读取，Header/trailer 另有 64 KiB 聚合上限，避免恶意回环服务在换行前迫使进程无限分配内存。
- 从 `/rules` 得到的规则用于本地预测展示，从 `/connections` 得到的 `rule/rulePayload/chains` 才标为“最近已观察”；两种证据不会合并成一个虚假的确定结果。
- 当前快照是用户点击连接/刷新时获取的一次性读模型；持续流量和连接更新以后应使用节流轮询或 WebSocket，并加入 generation 以丢弃过期回包。
- Clash Verge Rev 在 macOS 上可通过 `external-controller-unix` 运行 Mihomo；直接使用 Unix socket 可以复用正在运行的真实配置，而不需要下载、解析或持久化订阅 URL。
- 真实 API 再次证明未被 UI 使用的漂移字段不应强绑定类型：`fixed` 在不同版本/组类型中可能是布尔值或节点名，忽略它比创建无业务价值的兼容枚举更稳健。
- 内置 `GLOBAL` 是有效但通常不是用户首要操作的组；保留它并排序到普通策略组之后，比按名称硬删除更符合可发现性。
- Mihomo 延迟历史中的 `0` 表示未知/未测速而不是零延迟；显示层应渲染为缺失值。连接 metadata 的空 host 同样应回退到 destination IP。
- `external-controller-unix` 依赖 Unix 文件权限而不是 Bearer secret；Relay 的 Unix transport 不发送 `Authorization`，并在连接前拒绝普通文件和符号链接 socket。
- 真实控制器错误 body 可能包含不适合普通 UI 的诊断内容；结构化错误保留状态码，但用户可见文案只显示 HTTP status/reason。

## Relay 托管进程的本地架构发现

- 当前 `relay-ui` 同时负责 controller endpoint 环境变量、网络加载和展示状态；托管子进程若继续塞入 UI 会把进程所有权与视图生命周期耦合，应建立独立 engine crate。
- `relay-mihomo` 已是纯 controller 协议/模型边界，适合保持无子进程所有权；engine 只应产出一个 controller endpoint，再复用现有 `MihomoClient` 做健康检查和快照读取。
- 当前入口只创建一个 `RelayApp` entity，窗口关闭/应用退出没有 engine cleanup hook；第一阶段应先让 engine manager 的 Drop/显式 stop 可独立测试，再决定 GPUI 生命周期接线。
- 当前仓库没有配置文件解析器、sidecar 资源、下载器或平台数据目录依赖；本阶段不应引入自动下载/升级或真实订阅持久化，以免一次跨越供应链、密钥和进程三个信任边界。
- `ControllerState` 目前只有 Demo/Connecting/Connected/Failed，不能表达“未安装/准备/启动/停止”；engine 生命周期应使用独立穷举枚举，UI 再将其映射为简化状态。

## Mihomo sidecar 官方/运行时证据（2026-08-25）

- 官方 Releases 当前可见稳定版已到 `v1.19.30`，为 macOS/Windows/Linux 多架构提供独立压缩资产，并在 GitHub asset 元数据中给出 SHA-256；未来下载器必须固定版本、匹配明确 asset 名并验证摘要，不能下载 `Alpha` 或静默跟随 latest。来源：https://github.com/MetaCubeX/mihomo/releases
- 本机 Clash Verge 携带的 Mihomo `v1.19.29 darwin arm64` 帮助文本确认核心参数：`-d` 配置目录、`-f` 配置文件、`-t` 测试后退出、`-v` 版本、`-ext-ctl`/`-ext-ctl-unix`/`-ext-ctl-pipe` controller override、`-secret` API secret。
- 官方 general config 同时记录 `external-controller-unix`，说明 engine 可以为 macOS/Linux 每次运行创建独立 socket；Windows 应走 pipe 或独立 loopback 端口，不能假装共享同一路径语义。来源：https://wiki.metacubex.one/en/config/general/
- 本阶段只编码生命周期和命令计划，不实现下载器；官方资产数量多、CPU 兼容变体复杂，下载/校验/签名与许可证归档应作为单独供应链里程碑。
- 官方仓库的 `LICENSE` 是 GPL-3.0；未来若随 Relay 分发官方 Mihomo 二进制，需要在发布清单中处理许可证告知与对应源码可得性等 GPL 义务。Relay 自身的许可证选择应与“是否捆绑分发 Mihomo”分开决策。来源：https://github.com/MetaCubeX/mihomo/blob/Meta/LICENSE
- 官方稳定版发布页目前提供逐资产 SHA-256；GitHub 对发布提交的 verified 标记不能等同于二进制资产签名（这是基于发布页信息的推断）。未来下载器应固定明确版本并校验对应资产哈希，不跟随 Alpha/latest 漂移。
- 许可证必须按实际分发版本核验：`v1.19.30` tag 的 `LICENSE` 与 `Meta` 分支都是 GPL-3.0，而 `main` 分支当前是一个 5 行 MIT 文本。未来打包清单必须记录“具体 tag + 对应许可证”，不能用默认分支替代稳定版证据。来源：https://raw.githubusercontent.com/MetaCubeX/mihomo/v1.19.30/LICENSE
- 托管层采用新的 `relay-engine` crate：只持有自己启动的 `Child`，只产出 controller endpoint，再复用 `relay-mihomo` 做健康检查；外部 `RELAY_MIHOMO_CONTROLLER` 路径保持只读且不归 engine 所有。
- 第一版生命周期以可替换的 process spawner 和 health probe 做确定性测试，覆盖校验、启动、就绪、提前退出、超时、幂等停止和 Drop 清理；真实平台命令执行保持薄适配层。
- GPUI 目前只有一个 `controller_endpoint` 字符串和四态 `ControllerState`；连接按钮直接后台调用 `mihomo::load`。托管接线应保持同一个按钮：显式检测到 managed env 时先后台 `EngineManager::start`，拿到 endpoint 后再走同一只读快照路径。
- `RelayApp::with_controller` 被原生截图夹具直接使用；必须保留它作为纯外部控制器构造器，避免托管环境探测污染视觉回归和 fixture 测试。
- 托管模式只在 `RELAY_MIHOMO_BINARY` 与 `RELAY_MIHOMO_CONFIG` 同时存在时启用；可选 `RELAY_MIHOMO_DATA_DIR`、controller 和 secret。manager 先执行 `mihomo -t`，再 spawn，并只用轻量 `/version` 判定 ready；刷新数据继续复用现有只读四端点快照。
- macOS 默认托管目录为 `~/Library/Application Support/Relay/mihomo`，Linux 为 `$XDG_DATA_HOME/relay/mihomo` 或 `~/.local/share/relay/mihomo`，Windows 为 `%LOCALAPPDATA%/Relay/mihomo`；Unix controller socket 必须位于该独立目录内。
- `ManagedEngineConfig` 不再接收或保存 API secret；托管 TCP 在 engine validation 与 UI endpoint parser 两层都被拒绝，直到 Relay 能生成并验证自己的鉴权配置。stop/失败清理若第一次 terminate 失败会保留同一个 Child，Drop 再次尝试，避免丢失所有权。
- Windows named-pipe endpoint 和启动参数已经建模，但 `relay-mihomo` 尚无 named-pipe transport；因此 Windows 托管模式的健康检查仍是明确的后续项，当前 Windows 外部 loopback HTTP 控制器路径不受影响。
- 仅检查缓存的 `EngineState::Ready` 会在子进程事后崩溃时永久阻止重启；UI 刷新前必须通过 manager 对同一 owned Child 执行 `try_wait`，reap 后清空 handle，再允许新的 start。
- 原生宽屏截图重新生成后与变更前参考图逐字节相同，`visual-verdict` 为 100/100；托管文案只在显式 managed env 下出现，默认外部/fixture 视觉没有变化。
- Unix controller 不鉴权，因此 runtime 目录本身就是访问控制边界：启动前创建并强制 `0700`，拒绝 final symlink/非目录，并要求 socket 是 data dir 的直接子项，避免嵌套目录权限漂移。
- loopback TCP secret 不能通过 `-secret` 进入进程 argv，也不能只靠“用户 YAML 应该配置了同值 secret”的假设；因此本阶段完全禁用托管 TCP，外部 loopback controller 继续由已有只读客户端支持。
- `Command::status()` 会让恶意/损坏的 validation 永久阻塞；标准 spawner 改为 child + `try_wait`，默认 10 秒 deadline，超时后 kill 并 wait，fixture 回归在 2 秒门槛内通过。
- 安全终审为 LOW 且无 High/Medium：托管 TCP 与尚未实现 transport 的 Windows pipe 都在 UI 配置阶段 fail closed；engine 层仍独立拒绝 managed TCP，形成纵深防线。残余只有 Unix 父目录 symlink/TOCTOU 的本地加固项与未安装 `cargo-audit`。
## 订阅与配置编译官方证据（2026-08-25）

- Mihomo 官方 `proxy-providers` 文档规定：HTTP provider 使用 `type: http` 与 `url`；`path` 可省略但应保持唯一，且默认受 `-d` 指定 HomeDir 的目录约束；`interval` 以秒为单位。provider 自身可配置 `health-check`，包括 `enable`、`url`、`interval`、`timeout`、`lazy` 与期望状态码。
- 官方 provider 内容规范接受三类互斥格式：带顶层 `proxies:` 的 YAML、逐行代理 URI、以及 URI 内容的 Base64；三类不可混合。因此常见 URI/Base64 订阅可以直接交给 Mihomo provider 解析，但具体供应商返回的完整 Clash 配置仍需运行时验证，不能仅凭 URL 形态假定兼容。
- 官方策略组支持 `select`、`url-test`、`fallback` 等类型；`proxies` 引用出站或其他策略组，`use` 引用 proxy provider。策略组的 `url` 只检查直接列入 `proxies` 的节点，通过 `use` 引入的节点应依赖 provider 的 `health-check`。
- 官方规则按配置顺序自上而下匹配，第一条命中即生效；`DOMAIN`、`DOMAIN-SUFFIX`、`GEOIP` 与兜底 `MATCH` 均为正式支持的规则类型。这与目标中的“QX 风格：有序规则 → 策略组 → provider 节点”模型一致。
- 官方通用配置确认 `mode: rule` 是默认的有序规则匹配模式；`profile.store-selected: true` 会持久化 API 对策略组的选择，适合实现 QX 式“选一次、下次仍沿用”。
- 官方将 `mixed-port` 定义为同时支持 HTTP(S) 与 SOCKS5 的本地代理入口。首个订阅开发模式应只绑定回环地址并使用显式端口，避免与现有 Clash Verge 端口冲突；最终产品再通过端口分配/冲突检测完善体验。
- 官方警告 Unix socket 与 Windows named pipe 控制器不校验 API secret，因此这些控制端点只能创建在 Relay 私有运行目录或采用受限管道权限；这与现有托管引擎“私有 runtime + fail closed”的安全边界一致。
- 独立官方资料核验确认上述 provider/策略组/规则语法适用于 Mihomo `v1.19.30`。provider 内容的正式类型为 `yaml`、`uri`、`base64`；节点列表型 Clash/V2Board 订阅可直接作为 provider 输入，但返回完整顶层 Clash 配置的链接不属于官方 provider-content 契约，需要后续导入/转换流程。
- 最小 `mihomo -t` fixture 可以不声明任何代理监听端口，仅包含 `mode`、provider、策略组、规则和 `profile.store-selected`；产品实际启用代理时再显式开放只绑定回环的 mixed listener。这样当前阶段验证配置编译不会抢占 Clash Verge 的本地端口。

## 应用内配置工作区结构发现（2026-08-25）

- 现有侧栏 6 个入口全部是静态文本，选中态硬编码为“策略组”；配置工作区必须先建立真实可点击的顶层工作区状态，不能继续呈现视觉上可点但没有行为的导航。
- `RelayApp` 当前只持有 `PolicyWorkspaceState`，渲染分支集中在 `Render::render`；最小改动路径是在 `relay-core` 增加可测试的顶层/配置选择状态，在 `relay-ui` 增加独立配置工作区渲染分支。
- 现有紧凑布局由 `PolicyWorkspaceState::compact_navigation` 控制策略组列表/详情；配置视图应自行使用单列卡片流适配窄屏，避免复用策略组的返回栈语义。
- 原生截图程序已经覆盖宽/中/窄三档并支持点击坐标；可新增只使用演示状态的配置工作区截图，不接触本机订阅路径、URL 或 token。
- `ControllerRuntime` 目前只保留 External/Managed/Invalid，订阅编译成功后会丢失“已有配置”与“私有订阅”的来源区别。为了在 UI 安全展示来源，运行时需要只携带不含路径/URL 的枚举标签，而不是把敏感输入复制进视图状态。
- 配置预设的真实编译结构是单一 `subscription` provider、`Auto` URLTest、`Proxy` Select，以及有序 `GEOIP,CN,DIRECT,no-resolve` / `MATCH,Proxy` 两条规则；配置工作区直接映射这份现有领域模型，不引入另一套虚构配置语义。
- 设计代理建议把 Route Probe 作为唯一显著交互，铜色路径从所选规则流向策略组和出口，并明确标注“本地配置预览”而非实时命中；实现采用这一建议，同时拒绝尚未存在的添加订阅、重排保存和实时同步按钮。
- 第一轮原生视觉门禁为 78/100 `revise`：宽屏三栏成立且无泄密/溢出，但中窄屏只呈现当前分区导致 02/03 工作流不可见，Route Probe 纵向留白过多；选中规则的浅铜整行底色也过于接近普通选择态。
- 第二轮 reviewer 给出 88/100 且文字 verdict 为 `pass`，但低于技能 90 分硬门槛，不能视为放行；02/03 可见性和铜色语义已解决，剩余只需消除宽屏 stretch 空白并增加滚动底部留白。
- 第三轮 82/100 揭示“全部分区展开”的过度修正：可发现性提高，但中窄屏成为稀疏长页面。最终响应式方案应是始终可见的三段摘要导航 + 单一活动分区 + 紧凑 Route Probe，既不隐藏工作流，也不堆叠全部详情。
- 最终原生视觉门禁 94/100 `pass`：三段摘要导航、规则点击态、Route Probe、宽屏顶部对齐和敏感信息边界全部通过；无横向溢出、状态栏遮挡或 URL/token/path 泄露。
- Impeccable 独立终审发现阻断项：侧栏仍把 4 个未实现入口渲染成导航式行，即使降色不可聚焦也会构成伪 affordance。最终修正为只保留真实的“策略组 / 配置”两个 workspace，并为 compact 单字标签增加完整辅助名称。
- 导航修正后的最终 Visual Verdict 为 93/100 `pass`，二项侧栏留白仍平衡；独立 finish reviewer 复核唯一 blocking 已解决，最终 disposition 为 `ship`。

## 订阅源反馈与调试日志（2026-08-25）

- 根因不是点击事件完全丢失，而是订阅源默认已选中，重复点击只写入不显眼的底部状态；订阅卡片本身又没有行为，因此用户无法确认输入是否被接收。
- 修复采用显式状态而非装饰动画：订阅源 tab/摘要点击会打开安全诊断，卡片本身可展开/收起；诊断只显示来源枚举、凭据隐藏边界、只读能力和日志策略，不读取或复制秘密值。
- 调试日志使用 std-only、环境开关的固定事件枚举；日志行只含时间、级别和事件名，不接受动态字符串，从类型边界避免 URL/token/path/节点名进入 trace。
- 原生点击脚本在 `RELAY_UI_TRACE=debug` 下实际输出 workspace、订阅诊断、规则、主题、路由解释和 Mihomo 连接事件；输出格式稳定，未出现任何动态来源值。
- 订阅诊断首轮视觉门禁 88/100，原因是卡片内嵌高强调卡；改为细分隔线 + 状态点 + 内联文本后，最终 Visual Verdict 94/100 `pass`。

## 应用内订阅输入（2026-08-25）

- `RELAY_UI_TRACE=debug` 的真实点击记录持续出现 `configuration.source_diagnostics.opened/closed`，证明 GPUI 命中与通知链正常；用户“没反应”的根因是旧界面只展开低辨识度诊断文本，而且根本没有输入控件。
- 旧的诊断展开状态从 `relay-core` 删除：它不是领域状态，也不提供用户价值。订阅源现在始终显示真实输入框，点击 tab/流程步骤会直接聚焦输入。
- 输入实现采用项目固定 GPUI revision 的 `EntityInputHandler` / `ElementInputHandler` 官方范式，不新增依赖；支持 UTF-16/UTF-8 映射、IME 标记文本、鼠标选区、左右移动、Home/End、退格、删除、全选和粘贴。
- 输入限制为单行、最大 16 KiB；校验调用已有 `SecretUrl` 与 `Profile::qx_default`，错误只返回固定枚举，不格式化原始链接。trace 同样只增加固定事件名。
- 应用内链接当前仅存在内存并用于本地格式/结构预览，不写文件、不联网、不伪装成已经导入；真正启用仍由私有订阅文件开发模式承担。
- 宽屏与紧凑夹具都实际输入 `example.invalid` 保留域名并点击成功，trace 出现 `configuration.subscription_preview.succeeded`；最终 Visual Verdict 95/100 `pass`，无溢出或敏感数据。
- 架构结论已落实：有限响应继续走 `ControllerTransport`；长连接另设 `LiveController`，避免 `/logs`/`/connections` 被旧的完整 body 缓冲路径卡死。
- Mihomo `/connections` 的 GET 分帧未承诺只用换行，因此实时解码器同时支持换行 JSON 与无分隔的相邻 JSON 值。
- Mihomo 内核日志可能包含订阅下载失败 URL；展示前必须截断控制字符、限制 2048 字符并将 HTTP(S)/VLESS URL 替换为 `<redacted-url>`。
- 保存来源的运行时应用必须在后台执行；真实 `mihomo -t` 可能触发首次 geodata 准备并耗时数秒，不能阻塞 GPUI 点击处理。

## 可运行 VLESS 与实时遥测结论（2026-08-25）

- `relay-profile` 现在是唯一 VLESS 解析与 Mihomo YAML 编译边界；UI 预览、保存和运行时复用同一严格解析器，未知或重复参数失败关闭，Debug/错误不携带凭据。
- 托管配置更新采用“读取私有来源 → 编译候选 → `mihomo -t` → 私有原子替换 → 仅重启 owned child”的事务；重启失败会恢复旧 YAML 并尝试恢复旧进程。
- 外部控制器保持只读：系统 HTTP/SOCKS 可使用外部控制器暴露的本地端口，但 TUN 切换和配置写入只允许 Relay 托管内核，避免修改 Clash Verge 的状态。
- 实时读取与有限响应传输保持分离；`LiveController` 支持 loopback HTTP 与 Unix socket、chunked/raw body、换行和相邻 JSON 帧、有限读超时、取消与退避重连。
- GPUI 只消费有界合并邮箱：连接快照覆盖旧值，内核日志最多保留 500 条，后台流最多暂存 256 条，400ms 一次性通知，防止高速日志造成无界内存或重绘风暴。
- 内核日志展示前替换控制字符、截断 2048 字符，并对大小写不同的 HTTP/HTTPS/VLESS scheme 统一替换为 `<redacted-url>`；截图夹具和回归测试均验证可见脱敏结果。
- REALITY + gRPC 的通用 VLESS fixture 与普通订阅 profile 已使用本机 Mihomo `v1.19.30` 实际通过 `-t`；未读取或使用用户真实订阅，也未把任何订阅值传入测试命令。
- 最终 Visual Verdict 为 91/100 `pass`。残余产品边界是 Windows 托管 named-pipe/owner-only ACL 尚未实现；相关路径继续 fail closed，不影响 macOS/Linux 托管模式和三平台外部 loopback controller。
- 独立安全二次复核结论为 `LOW / SHIP`，Critical/High/Medium 均为 0；外部控制器在 UI 与 runtime 两层拒绝 TUN 写入，保存来源只对带 generated profile 的 Managed runtime 生效。
- 安全复核另行通过 `relay-mihomo`、`relay-profile`、`relay-engine`、`relay-ui` 测试、锁定 metadata 和依赖树检查；`cargo-audit`、`cargo-deny`、`osv-scanner` 未安装，因此没有声明完成 CVE 数据库审计。本轮没有新增依赖。

## 节点库存与用户策略分组初始边界（2026-08-25）

- 当前节点页把“订阅来源折叠组”称为分组，但它本质是节点库存的来源分段；新功能必须在文案、间距和组件边界上将来源库存与用户可执行策略组区分开。
- 现有 `NodeWorkspaceState` 只持有健康筛选与来源折叠 ID，适合继续管理页面浏览状态；用户策略组属于持久配置领域，不能塞进折叠状态。
- 已有 `relay-profile` 能编译 `select` 和 `url-test`，因此首版无需发明新 Mihomo 类型：手动组映射为 `select`，延迟优选组映射为 `url-test`；名称/协议规则负责收敛候选节点。
- 产品语义先采用两个正交维度：选择方式为“手动 / 延迟优选”，候选规则为“指定节点 / 全部 / 名称包含 / 协议匹配”。这比把“匹配”伪装成第三种选路算法更清楚，也能自然扩展来源、地区和标签条件。
- 节点库存当前只暴露 `LoadedProviderNode { name, protocol, latency, alive }`，订阅节点没有额外稳定 ID；首版显式成员引用必须使用“来源稳定 ID + 节点名”，不能仅按列表下标持久化。
- 现有私有来源存储采用独立文件、原子写入、Unix `0700/0600`、16 KiB 上限并拒绝 symlink；用户策略组应复用这些边界，使用单独版本化文件，不能和凭据文件混写。
- 当前 profile 组模型只覆盖 `Select` 与 `UrlTest` 的 `proxies/use`，尚未表达 Mihomo 的候选过滤字段；实现名称/协议匹配前必须先核对官方 group filter 语法并在 profile 层做确定性转义/验证。
- Windows 的用户持久化仍按既有安全策略 fail closed；本轮不能通过普通可写文件悄悄绕过 owner-only ACL 缺失。内存编辑和外部 controller 只读展示可以跨平台，落盘/托管应用维持既有平台边界。
- Mihomo 官方 group common fields 支持 `filter`、`exclude-filter`、`exclude-type`、`icon`、`include-all-*`；其中 `filter` 只作用于 `use` 引入的 provider 和 include-all 出站，不能假设它会过滤显式 `proxies`。来源：https://wiki.metacubex.one/en/config/proxy-groups/
- 官方 `url-test` 的“延迟优选”需要 `url`、`interval`，可选 `tolerance` 控制节点切换容差；首版分组可安全映射为 `select` / `url-test` 两类。来源：https://wiki.metacubex.one/en/config/proxy-groups/url-test/
- 因 provider 节点是动态内容，显式选择订阅节点应编译为 provider `use` 加锚定名称正则；本地保存的 VLESS 则可直接放进 `proxies`。名称匹配同理对 provider 使用 escaped regex，对直连 VLESS 在编译前做同样的本地匹配。
- “协议匹配”不能被直接建模为 Mihomo 的 include-type：官方只有 `exclude-type`，且不支持正则。为避免 UI/运行时不一致，首版不伪造协议包含规则；先交付全部节点、名称包含和显式成员，协议条件留到能确定性编译的后续扩展。
- Workspace 当前没有给 `relay-ui` / `relay-core` 配置 Serde 依赖，且工作约束禁止无授权新增依赖；分组存储采用小型、版本化、长度受限的文本协议并复用私有原子文件工具，不引入 JSON/TOML 依赖。
- 现有原生单行输入已经完整支持键盘、选区、粘贴和 IME，但占位文案、元素 ID、16 KiB 上限都绑定“订阅”。最小复用路径是把这些变成实例配置，保留现有类型和行为测试，再为分组名称/匹配词各创建独立 entity。
- 原生截图夹具已有真实导入订阅、恢复、节点页宽/窄和折叠流程；新视觉证据可以在同一私有 fixture store 预置策略分组，新增“库存 + 分组 + 编辑器”宽/窄截图，不读取用户数据。
- 用户策略组必须被 `Proxy` 主选择组引用，否则即使出现在 Mihomo API 中也无法成为规则的实际出口。编译顺序应是用户组 → Auto → Proxy，并让 `Proxy` 候选包含用户组。
- 分组 YAML 已使用 Clash Verge 随附的真实 `verge-mihomo -t` 通过校验；名称包含会转义为 Mihomo provider `filter`，手动 VLESS 则使用显式 `proxies` 引用。
- 安全复核未发现节点分组新增的 Critical/High 风险。既有 HTTP 订阅兼容会让公网 `http://` token 在网络上明文传输，风险等级 MEDIUM；用户明确要求 HTTP 来源兼容，本轮保持功能并继续以 HTTP 标签显式展示，后续应增加“不安全 HTTP”确认或默认仅允许回环 HTTP。

## 分组级节点测速边界（2026-08-25）

- Mihomo 官方 API 提供 `GET /group/{group_name}/delay?url=...&timeout=5000`，直接返回节点名到 `uint16` 延迟的映射；单节点也支持 `GET /proxies/{name}/delay` 返回 `{ delay }`。来源：https://wiki.metacubex.one/en/api/
- Relay 托管模式可以优先直接测试已编译进 Mihomo 的用户组；外部 Clash Verge controller 不包含 Relay 本地分组时，应回退到对当前匹配节点逐个调用单节点 delay API，并限制并发，不能在 GPUI 主线程串行等待。
- 测试 URL 使用固定 HTTPS 204 探测地址，超时固定为 5 秒；路径段和查询参数必须按 UTF-8 percent-encoding，不能把节点名或 URL 原样拼进 HTTP request line。
- 分组测速是易失运行状态，不持久化进私有分组文件；使用分组稳定 ID + generation 隔离删除、改名、重复点击后返回的过期结果。
- 外部 controller 的“只读”边界应精确定义为“不修改配置”：用户点击测速仍会让 Mihomo 主动发出固定 HTTPS 探测请求，因此运行来源说明必须明确提示这一点。
- 单组最多使用 8 个阻塞 worker，应用级再采用 single-flight 防止多个分组同时放大线程和探测流量；其他分组仍可在当前任务完成后逐一测试。
- single-flight 使用独立 active generation，不依赖可能在保存/删除时清除的卡片状态；后台任务返回前，即使原分组已被编辑或删除，也不会放行第二个测速任务。
- 最终安全复核确认本轮新增测速无 Critical/High/Medium 问题；保留的产品级行为是用户明确点击外部 controller 测速时，Mihomo 会发出固定 HTTPS 探测。

## 策略组详情与实际选路边界（2026-08-25）

- Mihomo 官方 `GET /proxies/{group}` 返回策略组的 `type`、当前选择 `now` 和候选 `all`；`PUT /proxies/{group}` 携带 `{"name":"节点"}` 后以 HTTP 204 完成选择。来源：https://wiki.metacubex.one/en/api/
- `Selector` 是手动选择组；`URLTest` 当前 `now` 由内核自动计算。本阶段手动组允许 PUT，自动组只展示当前优选，避免把自动策略误呈现成手动选择。
- 官方 `profile.store-selected: true` 可以持久化 API 选择；现有 Relay profile 已启用该配置，因此托管内核的手动选择不需要在节点分组文件中重复保存敏感或易漂移状态。
- Relay 本地用户分组只会编译进 Relay 托管 Mihomo；外部 Clash Verge controller 可能根本不存在同名组。详情页可以展示本地匹配候选与测速结果，但不能把外部同名组当作 Relay 分组并执行切换。
- 现有 `relay-mihomo::Proxy` 已能容错解析 `current/all/type/history/alive/provider`，新单组 GET 应复用同一语义或最小 DTO，不能在 UI 再复制一套脆弱 JSON 模型。
- `ControllerTransport` 承载有界的 GET/PATCH/PUT；选择 API 继续复用路径校验、loopback 限制、Unix socket 验证、body 上限与 Bearer 控制字符拒绝。
- 安全复核确认：外部 controller 和 Relay 托管的已有配置都不会进入策略组 PUT 路径；只有 `Managed { generated_profile: Some(_) }` 且运行中的 owned endpoint 可以切换。
- 策略组 PUT 前同时校验 Mihomo 返回类型严格为 `Selector`（忽略 ASCII 大小写）以及候选节点确实存在于 `all`；UI 层再以本地手动策略与运行态候选做第一层门禁。
- 用户提供的私有订阅域名/token 未出现在工作区文本、当前 diff、tracked files 或 Git 历史；测试仅使用明确的 fixture token。
- 节点页当前在同一纵向滚动区内呈现来源库存和用户分组卡片；最小详情交互采用卡片内“查看详情”展开，而不是模态框或新一级导航，宽窄布局都能保持上下文。
- 现有测速状态只保存汇总并丢弃节点名到延迟的映射；要实现逐节点结果，必须把完成态升级为带有有界 `BTreeMap<String,u16>` 的非 `Copy` 状态，或用等价的独立有界映射。

## 全分组统一测速结论（2026-08-25）

- 导入来源、用户节点分组和运行策略组现共用 `GroupBenchmarkState`、generation 过期隔离与应用级 single-flight；状态键增加来源类型前缀，避免不同工作区 ID 碰撞。
- 同一延迟柱状图标出现在三类分组标题前；导入来源头部通过停止事件传播将测速与折叠动作分开，紧凑策略详情保留同一入口。
- Mihomo `0 ms` 按失败处理，不纳入最低/最高/平均值，也不会被误判成最快节点；导入来源与用户分组逐行显示失败结果。
- 主策略组点击只执行 `GET /group/{name}/delay`，随后 `GET /proxies/{name}` 读取内核的 `now`；不会对自动策略发送 PUT。URLTest、Fallback 与 LoadBalance 的决策语义因此保留给 Mihomo。
- 自动组合法 `now` 会更新目录目标与节点延迟；LoadBalance 没有单一 `now` 时明确提示“没有单一固定出口”，不伪造赢家。
- 测速日志只写固定事件名 `group_benchmark.started/succeeded/failed`，不记录组名、节点名、URL 或 token。
- 宽屏、720px 紧凑、浅色与深色 GPUI 原生截图均无重叠或溢出；Visual Verdict 94/100 `pass`。

## 已保存 VLESS 测速边界（2026-08-25）

- 已复现外部 Clash Verge controller 对 Relay 私有 VLESS 节点测速返回 HTTP 404；代理目录中不存在该节点，因此不是“等待更久就会成功”。
- 用户决定不采用临时隔离 Mihomo 作为补丁。长期正确边界是 Relay 自己托管的 Mihomo 加载已保存 VLESS，测速与实际代理共用同一内核和配置。
- 在托管内核阶段完成前，“已保存”来源的测速操作应即时给出不可测速说明，不能向外部 controller 发出必然失败的请求，也不能长时间显示运行态。

## 增量测速与真实策略组反馈（2026-08-25）

- 导入/本地节点当前最多 8 个 worker；每个 worker 顺序处理自己的 chunk，但 UI 只在所有 worker join 后收到完整 `BTreeMap`，因此先完成的结果被无意义地延迟展示。
- 策略组使用 Mihomo 官方 `/group/{name}/delay`，响应本身是一次性节点延迟映射；其运行期仍应立即显示组按钮与候选行动画，响应返回后原子刷新全部结果和自动组当前出口。
- 用户已手动连接真实 Mihomo；“演示组 404”不再是当前唯一问题。现有策略组 UI 在运行期只改变柱形图标颜色，候选行没有运行态，错误又只落到底部状态栏，造成“点击无用”的体验。
- 策略列表 3px 竖线与卡片已有的背景/边框选中态重复，违反当前设计系统克制边框语言，应直接删除而非换色。

## 运行状态、路由模式与 QX 规则导入（2026-08-25）

- 当前应用其实已有窗口底部 28px `status_bar`，展示 Mihomo 状态、controller、操作消息和累计上下行；若用户所说“状态栏”指 macOS 菜单栏/Windows/Linux 系统托盘，则当前完全没有，需要确认术语。
- 当前顶部“关闭代理 / 系统代理 / TUN 代理”是流量接入层，决定系统流量如何进入 Mihomo；用户提出的“直连 / 全局 / 规则”是 Mihomo 路由决策层，两者必须拆成两个独立状态，不能合并成一个模式开关。
- 当前 runtime 只读取 `/configs` 的端口和 TUN 状态，并只支持 PATCH `tun.enable`；尚未读取/切换 Mihomo `mode`，因此直连/全局/规则不是已有能力的改文案，而是一条新的 controller 写入链。
- Mihomo 快照已包含 `GLOBAL` Selector，已有 PUT `/proxies/{group}` 能力可复用为全局出口选择；节点库存目前不能直接把节点设为 GLOBAL 当前项，需要显式的“全局出口”状态与选择反馈。
- 用户给出的 `airports.list` 是规则列表，不是节点订阅或完整 QX 配置。实际有 119 条有效规则：111 条 `DOMAIN-KEYWORD`、4 条 `DOMAIN-SUFFIX`、4 条 `DOMAIN`；每条第三列为 `PROXY/proxy`。
- QX 官方 sample.conf 说明 `[filter_remote]` 可通过 `force-policy` 覆盖远程文件每条规则自己的策略名。因此 Relay 导入时应要求用户选择目标策略组，并把文件内 `PROXY` 视为可覆盖的源策略标签，而不是假定本地一定存在名为 Proxy 的组。
- 当前 `relay-profile::Rule` 仅支持 DOMAIN、DOMAIN-SUFFIX、GEOIP、MATCH，缺少该样本占多数的 DOMAIN-KEYWORD；规则导入必须先扩展受支持语法、逐行校验、注释/空行处理、大小写策略映射、去重和错误行报告，再编译到 Mihomo。
- 用户已确认“状态栏”明确指系统状态栏/托盘图标：macOS 菜单栏、Windows 通知区域、Linux 桌面托盘，不是现有窗口底栏。
- 当前锁定的 GPUI revision 没有公开托盘/状态项 API；本机 crates.io 的 GPUI 0.2.2 源码虽有 macOS `status_item`，仍不是 Windows/Linux 跨平台能力，不能靠升级 GPUI 解决三平台目标。
- `tray-icon` 0.24.2 支持 macOS/Windows/Linux，许可证 MIT OR Apache-2.0、MSRV 1.73（低于项目 1.88）；macOS 要求在主线程事件循环已运行后创建，Windows/Linux 要求创建线程同时运行 Win32/GTK 事件循环。
- `tray-icon` 的 Linux 默认后端依赖 GTK3 与 libappindicator/ayatana-appindicator，意味着构建/发行要声明系统包要求；禁用 `libxdo` 可避免与托盘无关的剪切/复制系统依赖，但 GTK/appindicator 仍是 Linux 托盘必要后端。
- `tray-icon` 是当前最合理的统一边界，但它是新的直接依赖；工作协议要求新增依赖必须获得用户明确授权。在获得授权前可以完成领域状态和 Mihomo 路由 API，但不能把托盘功能伪装成已交付。
- 真实 Clash Verge controller 当前 `/configs.mode` 为 `rule`；其内置 `GLOBAL` 是 Selector，候选包含 DIRECT/REJECT、全部订阅节点和策略组，证明“全局模式 + 选定节点”可以直接映射为 PATCH `/configs` 与 PUT `/proxies/GLOBAL`，不需要自行实现转发逻辑。
- 现有 Relay 对外部 controller 的配置写入保持只读边界；直连/全局/规则以及 GLOBAL 切换首版应只对 Relay 托管 Mihomo 开放，不能为了当前演示连接静默修改 Clash Verge 的进程状态。
- GPUI `Application` 支持 `QuitMode::Explicit`，可以让关闭主窗口后托盘继续存活；`App::windows()`、窗口 `activate_window()` 与 `on_reopen` 可作为“显示 Relay”菜单动作的生命周期基础。
- 锁定 GPUI 虽然重导出了 `http_client` trait，但原生 `gpui_platform::application()` 没有注入具体 HTTP client，默认仍是 NullHttpClient；规则 URL 下载不能假装复用一个实际不存在的客户端。QX HTTPS 导入还需要明确的下载实现或新的 TLS HTTP 依赖。
- 依赖专项调研确认 `tray-icon` 0.24.2 由 Tauri Apps 维护，三平台覆盖、MIT OR Apache-2.0 与本项目兼容，活跃度和文档质量明显优于 `tray-item`；原生 FFI 则等同自行维护三套托盘库。来源：https://crates.io/crates/tray-icon 与 https://github.com/tauri-apps/tray-icon
- 推荐的生命周期边界是 binary shell（`main.rs`）持有 tray handle，并通过有限 `ShellCommand` 桥接 GPUI；托盘对象不能塞进 `RelayApp::render` 状态，否则主线程销毁顺序、窗口重建和测试替身都会互相污染。
- 路由架构应保持 managed-only mutation：`RoutingMode` 读取 `/configs.mode`，切换时 PATCH `/configs`；全局节点通过既有 Selector PUT 写 `GLOBAL`。外部 controller 若未来要开放写入，应是明确的独立 opt-in，而不是本轮顺手放宽信任边界。
- 路由模式与接入模式已在状态、API 和视觉三层分离：接入仍只负责系统流量如何进入 Mihomo，路由只负责 Mihomo 如何处理已进入的流量；两者的忙碌态和失败日志也互不复用。
- GLOBAL 节点选择以运行时返回的候选集为唯一真值，UI 不会把仅存在于本地库存、但不在 Mihomo GLOBAL `all` 中的名称伪装成可选出口。
- QX 规则解析器保留每条规则的源策略标签，并要求调用方提供映射；这与 QX `force-policy` 语义一致，能够把 `PROXY/proxy` 映射到用户选择的本地策略组，而无需硬编码组名。
- QX URL 下载不能由当前 GPUI 原生 NullHttpClient 完成；正式实现需要 Rust TLS HTTP 客户端。系统 `curl` 虽可作为开发诊断工具，但不作为三平台产品实现，以免引入隐性外部运行依赖。

## 托盘与远程 QX 规则实现结论（2026-08-26）

- `tray-icon` 的 macOS/Windows 托盘应与创建它的原生事件循环同线程；Linux 后端还依赖 GTK3 与 AppIndicator。GPUI 没有直接暴露 Linux GTK 循环，因此实现用同一前台线程周期性排空 `glib::MainContext`，避免额外线程破坏 GTK 约束。
- `ureq` 的同步 Agent 适合当前 GPUI 后台 executor：`https_only(true)`、最终 URI 再校验、全局超时、正文上限和严格 UTF-8 共同构成下载边界，不需要系统 `curl` 或外部进程。
- 持久化的是已经校验过的规则正文，而不是每次生成配置都重新联网；这样启动和托管配置重建可离线完成，同时保留来源地址用于用户识别和后续刷新。
- 导入时对原文件每条规则的策略标签采用 QX `force-policy` 式覆盖：所有可识别域名规则映射到用户选择的本地策略组；DIRECT/REJECT 走内置策略，其余名称映射到 Mihomo 策略组。
- Windows 的私有来源存储仍沿用项目既有 fail-closed 边界：在 owner-only ACL 尚未实现前不写入含凭据的普通文件。托盘与 HTTPS 下载可编译到 Windows，但规则持久化在 Windows 上仍是明确的后续风险。

## 配置瘦身与分流规则信息架构（2026-08-26）

- 当前配置宽屏同时展示代理来源、策略组、规则导入、两条内置规则和 Route Probe；其中策略组已有一级页面，节点明细已有节点页，Route Probe 也不是来源管理任务，形成明显重复和认知噪音。
- 紧凑配置页还保留 01/02/03 流程卡与规则预览，导致用户为了添加来源先理解产品内部编译结构；这些步骤对“添加/删除代理来源”没有必要，应全部删除。
- 新信息架构采用六个一级入口：节点、策略组、分流规则、网络活动、日志、配置。配置管理代理订阅、单节点和 QX 规则订阅源；分流规则只读展示最终匹配顺序。
- 代理来源页保留：一个统一 HTTP/HTTPS/VLESS 输入、导入动作、简洁结果反馈、已保存订阅卡、已保存单节点卡及删除；移除完整节点预览、策略预览、规则顺序、Route Probe、运行来源长说明和步骤编号。
- 配置页的规则订阅区保留：规则来源输入、目标策略选择、导入/删除、来源级规则数与异常行数。分流规则页只解析缓存正文并逐条显示 DOMAIN、DOMAIN-SUFFIX、DOMAIN-KEYWORD；内置 GEOIP/MATCH 作为明确的系统兜底区，不和远程来源混成同一编辑器。
- 更新间隔只有在真实调度和持久化存在时才展示；本轮不会加入仅改变外观但不执行更新的假控件。可先提供导入时刷新与删除，后续以同一来源卡扩展“立即更新/自动更新”。

## 来源更新与自动刷新边界（2026-08-26）

- 代理订阅启动恢复已具备真实联网刷新链：`preview_imported_subscription(SecretUrl)` 下载并解析节点；现有 `restore_imported_subscription` 只接受 `Pending` 状态，需要抽成 Ready/Unavailable 都可调用的统一刷新事务。
- 代理订阅当前用一个 URL-only 私有文件保存，`StoredSubscription` 没有更新策略；新增间隔必须兼容旧文件，不能要求用户重新导入或把 token 迁移到普通配置文件。
- QX 规则导入已有 HTTPS 下载、解析、私有正文缓存和托管配置重载；同 URL 内容变化时现有 `save_qx_rule_source_in` 会新建 ID，因此“立即更新”必须按已有 source ID 原子替换，不能复用新增来源语义。
- 两类来源都已有 generation 过期结果隔离，但使用应用级计数器；自动刷新应继续复用每来源 generation，避免手动刷新、定时刷新和删除之间互相覆盖。
- UI 只需在每张来源卡提供一个紧凑间隔选择器、立即更新动作和内联运行/错误反馈；VLESS 是静态单节点，不显示更新间隔。
- `restore_imported_subscriptions` 目前在首次渲染创建订阅输入框时启动；同一位置适合只启动一次自动刷新调度器。调度器应持有可取消的 GPUI task，并按固定短 tick 检查各来源是否到期，而不是为每张卡创建永久 timer。
- 仅保存 interval 不足以保证重启后的到期语义；每个远程来源还需要保存上次成功更新时间，调度器用 `last_success + interval` 判定，失败不覆盖最后成功时间并应采用有界重试节流。
- 锁定的 GPUI 提供 `cx.background_executor().timer(Duration)`；自动刷新可以使用一个 detached entity task，每 60 秒唤醒并通过弱实体更新状态，应用实体销毁后自然退出。
- 自动刷新全局只启动一个来源任务：每个 tick 至多选择一个到期来源，避免订阅预览和规则下载并发抢占网络、私有文件和托管配置重载；立即更新仍可显式触发并通过 generation 阻止过期结果覆盖。
- `SecretUrl` 故意没有公开字符串访问器；QX 刷新不应把 URL 解封到 `app/configuration.rs`。更安全的边界是在 `mihomo`/规则下载模块提供“以 StoredQxRuleSource 刷新”的事务，敏感 URL 只在下载调用的最小闭包内短暂使用，错误与 Debug 保持脱敏。
- 新增来源与刷新已有来源必须分成两个存储 API：新增可以按 URL 去重，刷新必须按 ID 覆盖同一私有文件，否则远程内容变化会制造重复规则源并重复匹配。
- `SecretUrl` 当前只允许在 `relay-profile` 内部渲染到 Mihomo 配置；跨 crate 的规则下载/原子重写需要增加与 `VlessSource::expose_to` 同形的闭包式最小暴露 API，禁止提供 `as_str`、Display 或可日志化的明文返回值。
- 配置来源卡现有标题行只有名称与删除按钮；加入更新能力后应改为“左侧名称/摘要，右侧间隔/立即更新/删除”的两层结构。窄屏允许控件换行，不能把三个动作硬塞进单行并挤压来源名称。
- 更新时间展示使用相对且稳定的短文案（如“刚刚更新”“2 小时前”），精确 Unix 时间不直接暴露给普通用户；失败状态保留上次成功时间并提供可重试动作。
- 持久化层已落定 `RemoteSourceRefreshInterval::{Manual, Hourly, SixHours, TwelveHours, Daily}`，并为代理与 QX 来源保存 `last_successful_update_unix_secs`；旧 URL-only 和 QX v1 文件安全回落为 Manual + 0。
- 存储 API 已区分 interval 更新、代理成功时间标记、QX 正文+成功时间原子替换；下一步应用层只需调用这些事务，不应直接写私有文件。
- 应用层已完成上述事务接线：代理更新只有在节点解析和成功时间原子保存都成功后才进入 Ready；QX 更新只有在下载、解析、按 ID 覆盖完成后才重载托管配置。
- 调度器只保存一个 `Stopped/Started` 生命周期状态，并通过弱实体 timer 自然退出；不会为每个来源留下独立任务。到期顺序为代理订阅优先、QX 规则其次，但退避中的首个来源不会阻塞后续到期来源。
- 自动失败的 5 分钟退避是会话内状态，不污染持久化的“最后成功时间”；应用重启后允许立即重试，手动操作也始终可以越过自动退避。
- 版本化代理来源会把 URL 十六进制编码，因此存储文件上限必须大于原始 16 KiB 输入上限；当前采用 33 KiB 上限并在编码前继续验证原始输入不超过 16 KiB。
- 配置页的视觉层级确定为：卡片保持中性，间隔是次级设置，立即更新是青绿色描边动作，移除是危险色文本；这比给整张卡上强调色更不容易误读为“已选中”。
- 代理订阅刷新后必须调用 `apply_saved_sources`：即使来源 URL 未变，托管 Mihomo 也需要重载/重启才能确保运行时采用最新 provider 数据；外部 controller 仍返回 `NotManaged` 并保持只读。
- Windows 的订阅/VLESS/QX 私有存储在本轮之前就因缺少 owner-only ACL 而 fail-closed；新刷新 API保持同一边界，因此没有新增假控件或数据泄漏，但三平台正式交付前仍需独立实现 Windows 安全存储。

## 多内核与 sing-box 初始边界（2026-08-26）

- 当前 UI、控制器客户端和配置渲染都直接使用 Mihomo 命名与 API；仅替换可执行文件无法工作，必须拆分领域配置、配置渲染、进程监督和运行控制四层。
- sing-box 稳定线可使用 Clash API控制 selector 并读取部分状态，但配置是 JSON 且策略模型不同；原生 gRPC API 位于 1.14 线，首版不能把尚未稳定的 API作为唯一依赖。
- sing-box 原生支持 selector 与 urltest；Mihomo fallback/load-balance 没有直接等价物，因此适配器必须公开能力矩阵，不能静默改变策略语义。
- 当前订阅解析依赖隔离 Mihomo；真正的 sing-box 独立运行最终需要 Relay 自己解析 Clash/YAML/URI订阅。首个切片可以继续保留 Mihomo 解析作为已知限制，但不能宣称已经完全去除 Mihomo 依赖。
- 官方配置校验命令为 `sing-box check`，运行命令为 `sing-box run -c <config> -D <directory>`；配置页必须以校验成功作为切换前置条件。
- sing-box Clash API 只公开 TCP listen address；Relay 因此为 sing-box 开放“带 Bearer 密钥的 loopback TCP”，同时继续拒绝 Mihomo 托管 TCP 和任何非 loopback 地址。
- GeoIP 已在 sing-box 1.12 移除旧字段；当前 `GEOIP,CN` 必须编译为官方 `geoip-cn.srs` remote rule-set，不能继续输出旧 `geoip` 配置。
- Reality TCP 的 sing-box 映射需要 `tls.reality.public_key/short_id` 与可选 `tls.utls.fingerprint`；无 `sid` 的链接以官方允许的零位十六进制空 short-id 表达。
- sing-box 的 Clash API 可为 Relay 现有状态、模式和 selector 控制提供兼容面，但返回字段允许 `rules: null`、`tun: null`；适配模型必须把 null 当作缺省值，而不能按 Mihomo 的对象/数组形状强制解析。
- 全局模式不能复用普通 `Proxy` 组：Relay 节点页的全局出口契约要求存在 `GLOBAL` selector。sing-box 渲染器现在为所有手动节点生成独立 `GLOBAL`，Global 模式路由到它，并拒绝用户占用 `GLOBAL/direct/block` 保留 tag。
- 控制器密钥必须属于具体受管 runtime/endpoint。进程级回退会在切回外部 Mihomo 后产生跨 controller 发送风险；最终实现只在线程/请求边界显式携带当前 sing-box secret，外部 controller 仍仅使用 `RELAY_MIHOMO_SECRET`。
- 当前 sing-box 切片只接受已保存的手动 VLESS TCP/TLS/Reality 节点。Mihomo provider 订阅、fallback、load-balance 与 TUN 会被能力矩阵和配置页明确拒绝，不做静默近似。
