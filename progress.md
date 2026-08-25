# UI 设计进度

## 2026-08-25
- 用户同意安装 UI/UX Pro Max、Impeccable 和 Material 3，并开始 UI 设计。
- 三项技能已安装到 Codex 用户技能目录；下一步建立设计上下文和设计系统。
- 完成 PRODUCT.md：记录用户、核心任务、产品差异、GPUI/三平台约束和可访问性目标；未确认的名称与品牌资产保持开放。
- 运行 UI/UX Pro Max 设计系统与针对性搜索；识别并拒绝其营销页面误分类，保留可用于桌面工具的单字体、语义色和交互建议。
- 阅读 Material 3 自适应布局/导航/颜色/字体以及 Impeccable Operate/Adapt 规则，形成七个不依赖 VPN 俗套的视觉世界候选。
- 运行 Impeccable direction concept seed，得到 Signal Patch Bay 主方向及外部挑战方向；本地决策页因环境无浏览器而退出，转为对话内可交互方向板。
- 完成四个方向的可交互选择板并在 Chromium 中渲染截图；以 Fluent 2 与 Material 3 官方页面为风格参考完成视觉审查，得分 92/100，通过 90 分门槛。
- 完成 Impeccable mechanical detector；记录 parser 降级以及示波器测量网格的有意例外。
- 用户选择方案 1「Signal Patch Bay」；锁定以信号通路表达规则、策略组和出口节点的产品视觉机制。
- 完成单文件交互原型 `relay-signal-patch-bay.html`：策略组/节点切换、域名路由预测、测速反馈、系统代理开关、宽中窄预览与明暗主题均可操作。
- 在 Chromium 中重拍宽屏、带 inspector 的中屏、深色窄屏三种状态；三个视图均无水平溢出，视觉判定 95/100，通过 90 分门槛。
- 完成 `GPUI_IMPLEMENTATION.md`，定义 Rust 状态模型、GPUI 组件边界、自适应断点、事件流及 Mihomo API 映射。
- 根据 Mihomo 官方 API 明确区分本地规则预测和 `/connections` 已观察真值；独立 finish review 的两项问题修复后均为 resolved，最终结论 `ship`。
- 完成 `DESIGN.md`：固化视觉 token、三档响应式布局、组件职责、键盘/无障碍要求、路由真值语义与 GPUI 交接清单。
- 初始化 Git 仓库并提交设计基线与独立核心状态模型；所有后续实现保持小步 Conventional Commit。
- 建立 `relay-core` 与 `relay-ui` workspace，6 个核心测试覆盖窗口分类、紧凑导航、选择保持和预测/观察语义。
- 将官方 `gpui` 与 `gpui_platform` 固定到 Zed 提交 `5631830c564afa89b3aba679f45d9c3345f9460f`，按 macOS、Linux Wayland/X11、Windows 分别声明平台特性。
- 完成原生 GPUI Signal Patch Bay：宽屏四区、中屏图标轨道、窄屏单任务流、明暗主题、策略/节点选择和路由解释器均可操作。
- 使用 `VisualTestAppContext` 保存宽/中/窄截图，并模拟点击验证“选择策略 → 进入详情 → 切换深色 → 打开路由解释”完整紧凑交互流。
- 原生视觉判定 96/100，通过门槛；`cargo test --workspace --all-targets` 通过，严格 Clippy 通过，仅剩 GPUI 上游 `block 0.1.6` 的 future-incompatibility 提示。
- 原生实现独立终审结论为 `disposition: ship`，没有阻断修正；评审要求继续保持预测/观察语义、铜色信号线、紧凑返回路径和克制的边框语言。
- 调研 Mihomo 官方控制器契约，确认第一阶段只需读取 `/version`、`/proxies`、`/rules`、`/connections`，并按 Bearer secret 鉴权；模型允许未知字段和可空历史数据。
- 新建 `relay-mihomo`：标准库 TCP/HTTP GET 传输、16 MiB 响应上限、连接/读取超时、Content-Length/chunked/connection-close 解码、结构化错误和 secret 脱敏。
- 以 TDD 覆盖端点顺序、JSON 漂移、策略映射、规则命中、已观察链路、Bearer 请求、401、body cap、回环限制与 header injection；Mihomo/core 合计 26 个测试通过。
- 将静态策略数据升级为拥有所有权的 `PolicyCatalog` 与运行时 ID；GPUI 点击连接后在后台读取快照并原子替换目录，不阻塞主线程。
- 增加连接/刷新状态、版本/连接数/累计流量、错误反馈和 `/connections` 最近观察卡片；系统代理继续保持演示开关，不产生写操作。
- 用本机伪 Mihomo 控制器生成 `native-wide-connected.png`，视觉判定 97/100，通过门槛，预测与观察的来源色彩和标签保持清楚分离。
- 明文传输仅允许 localhost 与 IP 回环地址，并在 DNS 解析后再次检查回环；远程控制器必须等 HTTPS 传输完成后再开放。
- 独立安全复核发现 HTTP 行在事后限长可能造成本地内存 DoS；改为读取时即限长并覆盖状态行、Header、chunk-size、trailer，复核结论为 resolved。
- 最终 `fmt`、workspace 全目标测试、严格 Clippy、原生截图生成和 `git diff --check` 全部通过；仅保留 GPUI 上游 `block 0.1.6` 的 future-incompatibility 提示。
- 独立连接态视觉终审为 94/100 `pass`，没有阻断视觉或无障碍问题。
- 通过用户现有 Clash Verge Mihomo 的 Unix socket 执行真实只读 smoke test；订阅凭据未写入仓库、命令日志、测试夹具或文档。
- 新增 macOS/Linux `UnixSocketTransport` 与 `unix:///absolute/path` endpoint，保持同一 GET-only、Bearer、超时和响应限额边界；Windows 对 Unix endpoint 返回明确错误。
- 真实 `/proxies` 揭示 `fixed` 字段可能从布尔值漂移为节点名；删除未使用的强类型绑定并加入字符串回归样本，完整目录解析通过。
- 根据真实数据修正：普通自定义策略优先于内置 GLOBAL、空 host 回退 destination IP、0 ms 视为未知延迟。
- live smoke test 默认 `#[ignore]`；live screenshot 必须显式启用且输出路径必须位于系统临时目录，因此真实节点/流量不会进入 Git。
- 第二轮真实 GPUI 截图视觉判定 95/100 `pass`，大规模节点/规则仍保持清楚层级与滚动行为。
- 独立安全复核确认订阅 URL/token 未出现在 worktree 或 Git 历史；Unix transport 增加“必须为非 symlink socket”检查，并明确不转发 Bearer secret。
- live screenshot 使用 canonical parent 校验、拒绝已有目标文件，防止 `..`/symlink 绕回仓库；控制器 HTTP 错误 body 不再展示到 UI。
- `.gitignore` 增加 `.env`、订阅导出和 live artifact 防线，降低以后手工测试误提交敏感文件的风险。
- 最终 workspace 31 个常规测试、真实控制器 ignored smoke、严格 Clippy、fixture/live 原生截图和 `git diff --check` 全部通过；安全复核降为 LOW，无未解决 High/Medium 问题。
- 开始 Relay 托管 Mihomo 进程阶段；本里程碑只建立隔离生命周期和 GPUI 状态，不终止/复用 Clash Verge 进程，不下载二进制，也不持久化订阅凭据。
- 完成稳定版 `v1.19.30` 的官方发行、CLI、控制器与许可证核验；确定新增独立 `relay-engine` crate，并保留外部 controller 原路径。当前开始以 fake process/health probe 锁定状态机和进程所有权。
- `relay-engine` 首轮 TDD 已由编译失败转绿：6 个测试覆盖命令计划、路径/回环边界、validation-before-spawn、就绪重试、提前退出、超时清理、幂等 stop 与 Drop；Manager 不接受裸 PID。
- 生命周期现在共 8 个测试：新增 API secret Debug 脱敏，以及 stop 首次失败后保留原 Child、由 Drop 再次清理。GPUI 已支持显式 managed runtime，未设置 binary/config 时完全保留外部 controller 行为。
- 原生 fixture 进程通过真实 `std::process::Command` 完成 `-t`、spawn、ready、stop；GPUI 默认模式截图与改动前 byte-identical，视觉门禁 100/100。workspace 当时 42 个常规测试通过，严格 Clippy 通过。
- 增加 ready 后崩溃探测：刷新前会先 `try_wait`，已退出的 owned child 被 reap 并允许下一次重新启动；不会卡在陈旧 Ready 状态。worktree 与 Git 历史的订阅域名/token 模式扫描均无命中。
- 安全复核的 Unix runtime、argv secret 与 validation timeout 均已修复；二次复核指出无法证明用户 YAML 的 TCP secret 生效，因此进一步在 engine/UI 两层禁用托管 TCP并删除 engine secret 状态。外部 loopback TCP 不受影响，等待最终安全 verdict。
- 最终安全 verdict 为 LOW，High/Medium 为 0；托管 Windows pipe 同样在配置阶段明确提示尚未开放，不再启动后才超时。最终 45 个常规测试、真实 controller ignored smoke、fmt、严格 Clippy、diff check 全部通过；当前新编译 GPUI 应用已启动并持续运行。
- 开始订阅与 QX 风格配置编译阶段：目标是让 Mihomo 自己通过 `proxy-providers` 获取订阅，Relay 只编译策略组和顺序规则；继续禁止订阅进入 Git/日志，本阶段不新增第三方依赖。
- 架构核验完成：新增 std-only `relay-profile` 作为“密钥感知领域模型 + Mihomo YAML 编译 + 私有原子写入”边界；`relay-engine` 继续只管理进程，`relay-mihomo` 继续只读 controller，现有外部/已有配置模式保持兼容。
- 开发订阅入口改为私有文件路径，而不是原始 URL 环境变量，避免 URL 进入 shell 历史与进程环境；私有输入文件的内容绝不进入错误消息。
- 错误记录：首次按仓库相对路径读取 `.codex/prompts/executor.md` 失败（仓库没有该目录），已改用用户级路径；一次进度补丁因目标行措辞不完全一致而未应用，读取文件尾部后已用精确上下文重试。两次失败均未改动代码。
- `relay-profile` TDD 红灯确认后转绿：6 个行为测试覆盖 HTTPS secret 脱敏、确定性 YAML/转义、规则顺序、非法引用/终止规则、Unix `0700/0600` 与 symlink 防线；UI 新增 3 个私有订阅文件/模式选择测试。
- 显式订阅开发模式使用 `RELAY_MIHOMO_SUBSCRIPTION_FILE`，要求仓库外绝对普通文件、单行 HTTPS、最大 16 KiB，Unix 权限 `0600` 或更严格；与已有 `RELAY_MIHOMO_CONFIG` 互斥，默认回环 mixed port `17890`。
- 使用本机 Clash Verge 附带的 Mihomo `v1.19.30` 和 `example.invalid` fixture 运行真实 `mihomo -t`，生成配置校验成功；未读取或请求用户真实订阅。Mihomo 在临时 runtime 下载 GEOIP fixture 后目录已清理。
- 错误记录：首次大补丁遇到实现代理正处于“删除旧文件、准备重写”窗口，目标文件短暂不存在导致补丁未应用；中止停滞代理后由主代理重新添加实现。首轮编译遗漏 `fmt::Write` trait，首轮严格 Clippy 命中两个机械风格警告，均已定位并修正。
- 独立安全复核首轮为 MEDIUM（High 0、Medium 2）：修复 Mihomo validation/launch 子进程继承 Relay secret/input 环境变量；订阅模式现在先解析并验证托管 controller，Windows/不支持平台会在读取或落盘 secret 之前 fail closed。等待二次安全 verdict。
- 二次安全 verdict 为 LOW（High/Medium 0）；Windows ACL 是被 fail-closed 路径隔离的后续项，`cargo audit` 未安装但新 crate 为纯标准库且没有引入外部依赖。
- 最终 55 个常规测试通过、2 个默认 ignored；本机 Mihomo 生成配置 ignored 校验另行实际通过。fmt、workspace all-targets、严格 Clippy、build、diff check、worktree/Git 历史敏感模式扫描全部通过；更新后的 GPUI 应用已重新启动。
- 开始应用内配置工作区阶段。按既有 Signal Patch Bay 视觉世界做 Operate 模式的局部扩展，不重新选择品牌方向；本里程碑呈现订阅源安全状态、QX 风格策略组和有序规则，不伪造尚未实现的 keychain/file-picker/持久化能力。
- Impeccable context 确认 PRODUCT.md、DESIGN.md 与原生 GPUI shipping 基线有效；本阶段采用 code-led 局部扩展，继承系统字体、青绿动作色、铜色仅路由轨迹、三档响应式结构。
- 错误记录：代码映射 explorer 因 GPT-5.3-Codex-Spark 用量上限立即失败；未产生文件改动，改由主代理本地结构搜索继续。
- 错误记录：按仓库指导尝试 `omx explore`，当前版本明确返回 hard-deprecated；遵循其迁移提示改用正常 Codex 本地结构搜索，不再重试该命令。
- 已完成现有 GPUI 导航、`RelayApp` 状态、响应式渲染分支和原生截图入口的第一轮映射；确认配置入口目前没有交互。
- 下一步先用核心层测试定义真实导航与不持有密钥的配置选择状态，再接入 GPUI 配置工作区。
- 核心层 TDD 已完成红灯到绿灯：新增顶层工作区、配置分区和有界规则选择状态，12 个 `relay-core` 行为测试通过。
- 侧栏“策略组/配置”现已是可聚焦、可点击的真实导航；配置页完成宽屏三栏与中/窄屏单任务分区，订阅来源只使用不含 URL/path/token 的安全枚举摘要。
- 配置页已映射现有 QX 默认编译结构（subscription → Auto/Proxy → GEOIP/MATCH），所选规则通过铜色 Route Probe 解释依赖路径；所有写入类能力均保持未呈现。
- 第一轮宽/中/窄原生截图已生成；Visual Verdict 78/100 未过门槛，进入唯一一轮聚焦修正：中窄屏展示完整分区、压缩 Route Probe，并收窄铜色语义。
- 第二轮已修复分区可见性与铜色选中面，reviewer 文本判定 pass 但数值仅 88/100；按 90 分硬门槛继续一次机械抛光，不扩大交互或产品范围。
- 第三轮确认全展开不适合作为中窄屏最终形态；开始收敛为三段摘要导航控制单一详情，保持完整可发现性并恢复工具型信息密度。
- 最终响应式形态已完成并通过 Visual Verdict 94/100：中窄屏三段摘要导航控制单一详情，compact rules 点击截图证明规则与紧凑 Route Probe 同屏可见；进入独立终审和发布前验证。
- 独立终审首轮 disposition 为 `do-not-ship`，唯一 blocking 是 4 个未实现的伪导航入口；已删除这些入口，只保留真实可点击/可聚焦/具辅助名称的“策略组 / 配置”。
- 导航修正后 Visual Verdict 93/100 `pass`，独立 finish reviewer 最终 disposition `ship`；workspace 59 个常规测试通过、2 个默认 ignored，严格 Clippy、build、fmt、diff check 均通过。
- 开始修复“订阅源点击无变化”和调试日志不足：TDD 已锁定安全诊断展开/收起状态；订阅源 tab 与卡片开始接入真实可见反馈，新增 `RELAY_UI_TRACE=debug` 固定事件日志，禁止动态敏感字段。
- 修复完成：订阅源 tab 默认点击会展开安全诊断，订阅卡片可再次收起/展开；原生宽窄点击截图与结构化 trace 已验证，Visual Verdict 94/100。workspace 61 个常规测试通过、2 个默认 ignored，严格 Clippy、build、fmt 与 diff check 全部通过。
- 用户反馈仍无法输入后，trace 证明点击链正常，确认旧“安全诊断”并非订阅配置功能；已删除该无价值状态并实现真正的原生 GPUI 单行输入。
- HTTPS 校验与 QX 结构预览完成 TDD 红→绿；输入支持键盘、粘贴、选区、IME、单行与 16 KiB 限制，所有错误和 trace 都不携带链接。
- 宽屏/紧凑截图夹具实际输入 `example.invalid` 并成功生成 `1 个来源 · 2 个策略组 · 2 条规则`；最终 Visual Verdict 95/100 `pass`。当前进入全量验证、提交和带 trace 重启。
- 最终 workspace 62 个常规测试通过、2 个环境依赖测试默认 ignored；fmt、严格 Clippy、全目标 build、diff check 和 worktree/Git 历史敏感域名扫描全部通过。
- 修复来源输入占位文本参与点击定位的问题：空内容的鼠标命中和所有编辑/IME 区间现在都会收敛到合法 UTF-8 边界，加入陈旧选区清空后继续输入的崩溃回归测试。
- 来源入口升级为 HTTP/HTTPS 订阅与 `vless://` 单节点识别；删除假的 `1/2/2` 结构计数，接入 Mihomo `GET /providers/proxies` 并按 provider 展示实际节点、协议、存活状态与延迟。真实 Clash Verge Unix socket smoke test、全量测试、严格 Clippy 和 Visual Verdict 97/100 均通过。
- 用户指出上一轮只识别 URL、展示当前控制器节点，并没有读取输入订阅中的节点；重新将验收标准锁定为“实际下载、解析并在配置页完整列出该订阅的节点”。
- 新增短生命周期隔离订阅预览：Relay 生成不含 geodata/健康检查的最小 profile，启动私有 Mihomo，通过 provider-only 控制器接口读取结果，完成后停止进程并删除 `0700` 临时目录、配置与缓存；错误与 trace 不携带 URL。
- 配置页新增异步读取、成功、空订阅和失败状态；远程预览只展示本次订阅的来源与节点，不再混入已连接 Clash Verge 的当前节点，连续编辑通过 generation 丢弃陈旧结果。
- 使用 Clash Verge 内置 Mihomo 与本地两节点 HTTP fixture 完成真实端到端测试，页面实际显示 `1 个来源 · 2 个节点` 以及两个节点行；macOS Unix socket 路径上限导致的首轮超时已通过短私有 runtime 路径修复。
- 开始把“读取订阅”升级为真正的“导入”：只有隔离 Mihomo 成功解析节点后才持久保存，新导入失败时保留旧订阅；VLESS 仍明确标为预览，不伪装为已持久导入。
- 新增用户数据目录内的单订阅存储：同目录临时文件原子替换，Unix `0700/0600`，恢复时拒绝 symlink、宽松权限、超限、多行和损坏内容；所有错误继续保持 URL/token 脱敏。
- GPUI 新增导入中、已导入、启动恢复、恢复失败、保存失败和移除状态；导入成功后清空可见输入但不触发状态回滚，重启后自动重新读取节点。
- 原生截图夹具改为显式临时 store，避免读取或覆盖用户真实订阅；新增“导入成功”和“全新 app 实例恢复同一订阅”的独立截图证据。
- 持久导入安全复核首轮为 MEDIUM：Windows 缺少 owner-only DACL/可靠原子替换，因此改为 fail closed；Mihomo validation/launch 子进程改为最小环境白名单，不再继承父进程中的无关凭据。桌面端用户主动导入/恢复 URL 不构成跨主体 SSRF 权限提升，HTTP 与本地来源继续保留显式风险文案。
- 二次安全复核结论为 macOS/Linux `LOW / SHIP`，没有剩余阻断项；Windows 继续明确不支持持久导入，直到 owner-only DACL、原子替换和 named-pipe 全部完成。
- 最终 workspace 75 个常规测试通过、3 个环境依赖测试默认 ignored；真实 Mihomo 导入→持久读取→再次解析测试另行通过。严格 Clippy、全目标 build、fmt、diff check 和 worktree/Git 历史敏感模式扫描全部通过；Visual Verdict 96/100 `pass`。`cargo audit` 当前未安装，且本轮没有新增依赖。
- 新增一级“节点”工作区，直接复用已导入订阅（无导入时回退到当前 Mihomo）的 provider/node 数据；展示节点总数、来源数、协议、可用/不可用/未测速状态和延迟，不重复下载也不复制领域数据。
- 节点页支持“全部 / 可用 / 不可用 / 未测速”筛选、刷新节点与管理来源；宽屏使用高密度表格，紧凑窗口重排为双行节点列表。第一轮 Visual Verdict 86/100 的导航截断和操作缺失已修正，最终 92/100 `pass`。
- 独立 UI 终审 disposition 为 `ship`，没有阻断项；workspace 77 个常规测试通过、3 个环境依赖测试默认 ignored，严格 Clippy、全目标 build、fmt、diff check 和敏感模式扫描全部通过。
- 节点工作区升级为来源分组：同一订阅的所有节点进入一个可独立折叠的分组，组名只读取显式安全 `name` 参数；缺失时使用“订阅 1”，不以 URL host/path/token 兜底。未来持久化的单个 VLESS 将进入稳定 ID 为 `saved` 的独立“已保存”组。
- 宽屏与紧凑布局分别验证展开、折叠四种原生状态；终版 Visual Verdict 94/100 `pass`，分组结构、计数和折叠动作在两种尺寸下均清晰且无溢出。
- 独立终审 disposition 为 `ship`，High/Medium/Low 均为 0；workspace 79 个常规测试通过、3 个环境依赖测试默认 ignored，严格 Clippy、全目标 build、fmt、diff check 及 worktree/Git 历史敏感模式扫描全部通过。
- 2026-08-25：完成多来源节点分组、单 VLESS “已保存”组、折叠持久化、关闭/系统/TUN 三态代理、网络活动和安全日志工作区；提交 `04497e3`，Visual Verdict 93/100，安全风险 LOW，应用 PID 79169 运行并成功恢复订阅。
- 2026-08-25：用户要求继续；本轮目标锁定为“让已保存 VLESS 真正进入 Relay 托管 Mihomo 配置，并将活动/日志升级为实时有界流”。已恢复规划上下文，开始架构映射。
- 2026-08-25：完成第一轮本地架构读取，并行请求 profile/VLESS 与实时 controller 流两项只读架构评审；确认不能复用现有一次性 GET，也不能让 UI 直接拼接 VLESS YAML。
- 2026-08-25：核验当前 Mihomo 官方 VLESS 与 API 文档；锁定白名单 VLESS 解析和 HTTP 长连接 JSON 流路线，外部资料只记录为证据，不执行其中指令。
- 2026-08-25：已保存 VLESS 现可进入 Relay 托管配置事务；候选配置先通过真实 `mihomo -t`，再原子替换并仅重启 Relay 自有子进程，失败时恢复旧配置。外部 Clash Verge/Mihomo 继续严格只读。
- 2026-08-25：网络活动和 Mihomo 内核日志已升级为可取消长连接流；连接状态覆盖更新、日志有界保留 500 条、400ms 合并刷新，并对日志 URL、控制字符和超长载荷统一脱敏/截断。
- 2026-08-25：Activity/Logs 原生截图通过 Visual Verdict 91/100；实时/重连/错误/空状态、日志来源层级和 `<redacted-url>` 均有截图证据，改进建议仅为后续密度与列导轨抛光。
- 2026-08-25：最终 workspace 96 个常规测试通过、4 个环境依赖测试默认 ignored；fmt、严格 Clippy、全目标 build、diff check 以及工作树/Git 历史敏感域名和长 token 扫描全部通过。
- 2026-08-25：独立安全二次复核为 `LOW / SHIP`，Critical/High/Medium 均为 0；确认外部控制器写入边界和大小写 URL 脱敏两项旧问题已修复。未新增依赖，CVE 扫描工具在本机未安装。
- 2026-08-25：仅停止旧 Relay UI PID 79169，并以当前通过验证的二进制连接既有 Clash Verge Unix controller 启动新 PID 89215；未终止或修改 Mihomo/Clash Verge。无终端 `nohup` 在当前 macOS GPUI 环境会立即退出，因此最终使用持有 PTY 的运行会话保持应用生命周期。
- 2026-08-25：开始“节点库存与用户策略分组”阶段。验收锁定为节点页上半区展示按来源导入的节点，下半区展示用户策略分组；分组可创建、重命名、换图标，并配置手动成员或“候选匹配 + 延迟优选”规则。
- 2026-08-25：Impeccable 上下文确认继续使用既有 Signal Patch Bay / Operate 视觉世界；不复制 QX 外观，只复用其“节点库存与策略组分离”的产品逻辑。
- 2026-08-25：分组领域模型完成首轮 TDD 红→绿：18 个 `relay-core` 行为测试通过；模型覆盖安全 ID/名称、四种图标、手动/延迟优选、全部/名称/显式成员匹配、稳定来源节点身份和 128 成员上限。
- 2026-08-25：节点页已明确分成上方“导入的节点”和下方“节点分组”；完成创建、重命名、图标、手动/延迟优选、全部/名称/明确选择规则、私有持久化与删除交互。
- 2026-08-25：用户组已编译进 Relay 托管 Mihomo：手动=`select`、延迟优选=`url-test`，Proxy 主组引用所有用户组；外部 controller 继续只读。全仓测试、严格 Clippy 和格式检查已通过，Visual Verdict 93/100。
- 2026-08-25：真实使用 Clash Verge 自带 `verge-mihomo -t` 验证了带 `filter`/`url-test` 的用户分组 YAML，配置校验成功。独立安全复核未发现 Critical/High；保留一项既有 MEDIUM 风险：公网 HTTP 订阅会明文传输 token，这是兼容 HTTP 来源的产品边界，不是本轮新增。
- 2026-08-25：功能提交 `acea9e5`；仅停止旧 Relay UI PID 89215，并以新二进制连接既有 Clash Verge Unix controller 启动 PID 805。Clash Verge PID 19351 与 Mihomo PID 19379 保持运行且未被修改。
- 2026-08-25：开始“分组级节点测速”阶段。产品边界锁定为每个用户节点分组独立发起测速，只测试该组当前匹配的候选节点，并在分组卡片显示有界后台任务的实时状态与汇总结果。
- 2026-08-25：完成官方 API 与现有 transport 边界映射；分组测速状态 TDD 红→绿，覆盖成功/失败计数、最低/最高/平均延迟及 generation 过期结果隔离。
- 2026-08-25：分组卡片已接入未测速、运行、汇总和失败四态；Relay 托管组优先调用 group delay，外部 Clash Verge 按当前候选逐节点回退，固定 8 worker 且应用内仅允许一个分组测速任务。
- 2026-08-25：真实 Clash Verge Unix controller 单节点 HTTPS 204 探测返回有效延迟；宽屏/720px 原生 GPUI 截图无截断或溢出，Visual Verdict 95/100 `pass`。
- 2026-08-25：全 workspace 109 个常规测试通过、5 个环境依赖测试默认 ignored；fmt、all-targets check、严格 Clippy 和 diff check 全部通过。安全复核未发现 Critical/High，固定探测 URL、percent encoding、错误脱敏和单任务并发边界均已验证。
- 2026-08-25：仅停止旧 Relay UI PID 805，并以通过验证的新二进制连接既有 Clash Verge Unix controller 启动 PID 6498；Clash Verge PID 19351 与 Mihomo PID 19379 未被停止或修改。
- 2026-08-25：开始“策略组详情与实际选路”阶段。验收锁定为点击分组后查看真实候选节点、逐节点延迟和当前出口；手动组允许切换，延迟优选组只展示 Mihomo 当前优选。外部 controller 不伪装成已应用 Relay 本地分组。
- 2026-08-25：已核对 Mihomo 官方 group/proxy API：详情读取使用 GET，手动选择使用 PUT + JSON 并返回 204；现有 `profile.store-selected` 足以让托管内核保存选择。开始映射 GPUI 状态与 runtime 边界。
- 2026-08-25：完成现有 transport、Proxy 模型、NodeWorkspace 和分组卡片映射；详情将保留节点页上下文，不做模态框。逐节点延迟需要把测速结果从纯汇总升级为节点名映射。
- 2026-08-25：策略组详情 TDD 红灯已确认：当前缺少 `NodeGroupRuntimeState`，测速完成态也尚不能保存节点名到延迟映射；失败与预期新契约一致。
## 2026-08-25（续）

- 已恢复会话并完成当前边界盘点：`relay-profile` 尚无顶层直连代理模型，`EngineManager` 尚无安全的配置替换/重启事务，`relay-mihomo` 的只读传输会一次性缓冲响应，不能直接承载 `/connections` 与 `/logs` 长连接。
- 已核对 Mihomo 官方 VLESS 与控制器 API：运行时解析将采用明确白名单并对未知/重复参数失败关闭；实时连接与日志优先使用控制器长连接接口，采用有界缓冲、可取消读取与 UI 节流。
- 产品边界已明确：保存的 VLESS 只注入 Relay 自己托管的 Mihomo 配置；连接外部 Clash Verge/Mihomo 时仅展示保存状态，不改写外部配置。
- VLESS 编译链已完成测试先行实现：严格白名单解析、凭据脱敏、顶层 `proxies:`、Auto/Proxy 直接节点引用与多订阅来源合并均已转绿。
- Relay 托管配置现采用候选文件 `mihomo -t` 验证、私有原子替换、仅重启自有子进程和失败回滚；外部控制器/已有配置返回只读提示。
- `/connections` 与 `/logs` 已接入 std-only 长连接读取，支持 chunked/相邻 JSON 帧、取消、退避重连、连接快照合并、日志有界队列和 400ms GPUI 节流。
- 本机 Clash Verge Mihomo 已真实验证普通订阅配置与 REALITY+gRPC VLESS 通用夹具，两份 YAML 均通过 `mihomo -t`。
# 2026-08-25 策略组详情状态层

- 先写失败测试，锁定节点级测速结果必须按节点名保留，以及运行状态必须拒绝过期刷新和未知候选选择。
- `cargo test -p relay-ui group_ --locked` 已由预期 RED 转为 4/4 GREEN。
- Mihomo 客户端现已覆盖 HTTP/Unix 的策略组读取与 Selector PUT；下一步将边界接入 Relay 托管运行时和节点页。
- Relay 运行时只对 `generated_profile: Some` 的托管配置读取/切换本地策略组；外部控制器与已有配置返回只读状态，不会因同名分组发生误写。
- 节点页已接入内联详情、逐节点来源/协议/健康/延迟、当前出口、自动组选中项和手动选择动作；`cargo check -p relay-ui --locked` 通过且无项目警告。
- 原生宽屏/720px 紧凑详情截图已生成；Visual Verdict 94/100，通过 90 分门禁，无重叠、截断或横向溢出。
- 安全复核为 LOW：0 个 Critical/High/Medium；已将误导性的 `ReadonlyTransport` 重命名为 `ControllerTransport`，并把 Selector 类型校验收紧为 ASCII 大小写不敏感的精确匹配。
- 全仓验证通过：118 passed、5 ignored；workspace 全目标 Clippy `-D warnings` 通过；对正在运行的 Clash Verge Unix controller 只读 smoke test 通过。
- Git 提交 `d0dc12f feat(ui): add strategy group controls` 已创建；Relay 已换成新构建进程 PID 13356，Clash Verge 与 verge-mihomo PID 19351/19379 保持未动。

# 2026-08-25 全分组统一测速

- 已启用 planning-with-files-zh、Impeccable harden、rust-testing 与 rust-patterns；锁定 Operate 模式下的统一图标、状态反馈和防重入边界。
- 当前工作树从提交 `0e1ee1d` 干净开始；进入三类分组的本地架构映射。
- 两个 Explorer 都因 Spark 用量上限失败且没有修改文件；已切换到本地映射，不重试同一失败路径。
- 已确认可复用的核心是 `NodeGroupBenchmarkState` + `ControllerRuntime::test_node_group_delay`；主策略的现有测速按钮是伪交互，导入来源头部尚无测速动作。
- 官方 API 证据确认 group delay 会清除自动组 fixed 选择；实现将在测速后重读组 `now`，不发送人工 PUT。
- TDD RED 已确认：`PolicyCatalog` 尚不能原地应用组延迟/新出口，`ControllerRuntime` 也尚无 group delay 后重读自动 winner 的 API；失败正好命中新契约。
- TDD GREEN：`PolicyCatalog::apply_group_benchmark` 会更新延迟/健康和合法当前出口；`ControllerRuntime::test_policy_group_delay` 对 HTTP/Unix 执行 group delay 后重读 `now`。
- 三类分组已全部接入统一前置测速图标与四态反馈；来源组测速不会触发折叠，自动策略测速不会执行人工选择。
- 严格 Clippy 首轮发现两处声明式视图体积、一处参数数量和两个小型风格问题；已通过提取策略状态文案、缩减 row 参数和窄范围视图豁免修复，复跑通过。
- 全 workspace 测试通过：124 passed、5 ignored；fmt、严格 Clippy、diff check、私有订阅域名/token 扫描以及真实 Clash Verge Unix controller 只读 smoke test全部通过。
- 原生宽屏/紧凑截图已生成，Visual Verdict 94/100 `pass`；当前仅剩提交与只重启 Relay UI。
- Git 提交 `feat(ui): unify group speed tests` 已创建；仅停止旧 Relay UI PID 19410，并以相同 Unix controller 启动新 PID 25096。Clash Verge PID 14173 与 verge-mihomo PID 14201 保持运行且未被停止或修改。

# 2026-08-25 导入节点测速反馈与 VLESS 诊断

- 用户指出导入来源图标像选中态、节点“可用”语义失真、测速中缺少逐行反馈，并报告已保存 VLESS 测速长期无结果。
- Motion thesis：唯一循环动画放在正在等待结果的延迟单元格，用旋转缺口环表示“该节点尚在探测”；分组按钮只做中性控制，不使用选中背景；完成或失败立即停止动画。预算为每个当前可见节点一个纯旋转几何元素，不改变布局、不阻塞 GPUI。
- 实际向 Clash Verge Unix controller 请求已保存 VLESS 名称得到 HTTP 404，完整代理表也确认不存在；根因是外部控制器从未加载 Relay 私有 VLESS，而旧路径仍对外部控制器做单节点测速。
- 用户明确拒绝临时 Mihomo 绕路方案；已撤销 VLESS 隔离测速测试与实现方向。正确后续是 Relay 自己托管的 Mihomo 加载已保存 VLESS 后复用正式测速链，本轮不实现该架构阶段。
- TDD RED 继续只锁定导入节点的运行/成功/失败延迟状态；按用户要求，本轮不改动已保存 VLESS 的测速实现。
- TDD GREEN：导入节点状态映射覆盖运行中、部分失败和成功延迟；来源标题、宽屏状态列与紧凑状态标签不再显示“可用”。
- GPUI 延迟栏使用 8 点、720ms 循环旋转反馈，只有 `Running` 状态启动；成功、失败和空闲状态均为静态文本。
- 宽屏与 720px 紧凑原生截图通过 Visual Verdict 95/100 `pass`；中性测速按钮、列表列宽和响应式布局均无溢出或错位。
- 全 workspace 验证通过：126 passed、5 ignored；格式、严格 Clippy、diff check 和私有订阅域名/token 扫描均通过。
- Git 提交 `fix(ui): clarify imported node benchmarks` 已创建；仅停止旧 Relay UI PID 25096，并以相同 Unix controller 启动新 PID 28809。Clash Verge PID 14173 与 verge-mihomo PID 14201 保持运行且未被停止或修改。

# 2026-08-25 增量测速与真实策略组反馈

- 用户确认有限并发可以保留，但要求单节点结果一完成就立刻显示，不等待整组收尾。
- 用户已连接真实 Mihomo 后复现策略组测速无明显动画且完成后不刷新；本轮同时删除策略卡片冗余竖线。
- Motion thesis：测速按钮在运行期由静态柱形图切换为同一 8 点旋转器；候选行只在自身结果未到时旋转，单个结果到达立即停转并显示延迟或失败。除此之外不增加循环动效。
- TDD RED 已确认：当前状态模型没有逐节点 `record/node_state`，运行态只保存 generation；runtime 也没有带进度回调的节点测速入口，失败点与本轮需求一致。
- TDD GREEN：本地 HTTP 夹具用条件变量阻塞慢节点，确认快节点的 30 ms 会先于慢节点的 70 ms 回调；状态模型也验证逐节点成功、失败和过期 generation 隔离。
- 真实 Clash Verge Mihomo 的 `自动选择` 组返回 44 个有效延迟，证明官方 group delay 接口与当前内核兼容。
- GPUI 成功截图确认策略组候选延迟从 54/67 ms 刷新为 31/88 ms，完成摘要显示 2/2、最低 31、平均 60；第二轮 Visual Verdict 96/100 `pass`。
- 最终验证通过：全 workspace 128 passed、5 ignored；`cargo fmt --check`、严格 Clippy、`git diff --check` 和敏感订阅域名/token 扫描全部通过。
- Git 提交 `fix(ui): stream group benchmark results` 已创建；仅停止旧 Relay UI PID 28809，并以同一 Unix controller 启动新 PID 33768。Clash Verge PID 14173 与 verge-mihomo PID 14201 保持运行且未被停止或修改。

# 2026-08-25 运行状态、路由模式与 QX 规则导入

- 已启动 planning-with-files-zh 与 Impeccable shape，仅做需求与架构确认，尚未修改生产代码。
- 已核对现有底部状态栏、两层模式语义、Mihomo GLOBAL 选择能力和用户提供的 QX 规则文件；确认该文件可解析，但当前领域模型缺少 DOMAIN-KEYWORD 和远程规则源持久化。
- 用户确认目标是三平台系统托盘/菜单栏图标。已核对锁定 GPUI 与本地依赖树：没有可直接复用的跨平台托盘 API；正在评估 `tray-icon` 的事件循环、Linux 系统包和依赖许可边界。
- 已用真实 Mihomo 只读确认 `mode=rule` 和 GLOBAL 候选语义；同时确认 GPUI 可支持托盘驻留所需的 Explicit 生命周期，但托盘本身和 HTTPS 规则下载仍各自需要明确依赖方案。
- 依赖与架构两项独立评审均推荐 `tray-icon + binary shell command bridge`；已向用户请求新增依赖授权，同时并行进入 RoutingMode 与 `/configs` PATCH 的 TDD 红→绿实现。
- `RoutingMode::{Direct, Global, Rule}`、`RuntimeConfig.mode` 和 `/configs` PATCH 已完成 TDD；未知/缺失 mode 安全回落规则模式，HTTP/Unix controller 共用同一 API。
- GPUI 顶栏现在把“接入”和“路由”显示为两个独立分段控件；720px 紧凑窗使用带维度前缀的循环控件，避免把“关闭代理”误解成“直连”。
- 节点页新增 GLOBAL 候选与当前出口展示：Relay 托管内核可切换，外部 Clash Verge controller 显示“当前 · 只读”且不会发送 PUT；路由模式偏好写入私有 store 并参与下一次托管配置生成。
- `relay-profile` 已加入 DOMAIN-KEYWORD、QX 规则列表解析、结构化错误行、源策略标签映射，以及可持久渲染的 ProfileMode；代表性 airports.list 语义测试已通过。
- 已生成宽屏、720px 紧凑和真实 controller 节点页截图；新增 `nodes-wide-connected-global.png` 专门验证全局出口列和只读边界。系统托盘与 HTTPS 规则下载仍等待用户明确批准两个直接依赖。
- 首轮 Visual Verdict 88/100 指出窄窗控件与只读状态的交互暗示不足；改成带循环标记的紧凑控件和纯文本 `● 当前 / 外部只读` 后，第二轮为 91/100 `pass`。
- 最终本阶段验证通过：143 passed、5 ignored；workspace fmt、all-targets Clippy `-D warnings`、diff check 和用户私有订阅域名/token 扫描均通过。

# 2026-08-26 系统托盘与远程 QX 规则导入

- 系统托盘已使用 `tray-icon 0.24.2` 接入 GPUI 生命周期：支持显示/重开主窗口和退出；仅在托盘成功建立后切换为显式退出，初始化失败时应用仍可正常运行。Linux 复用 GTK 主上下文并由 GPUI 定时协作泵送事件。
- 远程规则下载使用 `ureq 3.4.0` 的 rustls 后端，只接受 HTTPS，限制 5 次跳转、15 秒总超时和 1 MiB 正文；错误、Debug 与运行日志均不包含来源 URL 或 token。
- QX 规则正文、来源和目标策略以版本化私有文件原子保存；启动恢复后在 GEOIP/MATCH 兜底前插入规则，并按用户所选的 Proxy、DIRECT 或自定义策略组统一映射。外部 Clash Verge controller 保持只读。
- 用户提供的公开 `airports.list` 已通过真实 HTTPS 下载、解析、私有保存和恢复测试，识别规则数大于 100；本地解析/持久化测试 5 通过、1 默认忽略，live 测试 1 通过。
- 原生宽屏和 720px 紧凑截图完成；紧凑路由说明改为完整单行摘要后 Visual Verdict 93/100 `pass`，无状态栏裁切。
- 全 workspace 验证通过：148 passed、6 ignored；fmt、严格 Clippy、all-targets check、build、diff check 和私有订阅域名/token 扫描全部通过。仅保留上游 `block 0.1.6` 的未来兼容提示。
- 仅停止旧 Relay UI PID 47280，并以新构建二进制连接同一 Clash Verge Unix controller 启动 Relay UI PID 48994；托盘初始化未报错，来源恢复成功。Clash Verge PID 14173 与 verge-mihomo PID 14201 保持运行且未被停止或修改。
