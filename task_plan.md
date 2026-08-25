# GPUI 策略代理客户端 UI 设计

## 目标
建立一套现代、统一、可在 Windows/macOS/Linux 复用的桌面设计系统，并制作可交互的响应式原型。视觉以 Fluent 2 的桌面效率为基础，吸收 Material 3 的自适应布局与克制的 Expressive 动效，最终可直接翻译为 GPUI tokens 和组件。

## 阶段
- [complete] 1. 安装并核验设计技能，建立产品设计上下文
- [complete] 2. 生成并收敛设计系统、信息架构和窗口尺寸规则
- [complete] 3. 制作策略组核心页面的交互原型
- [complete] 4. 宽/中/窄窗口与明暗主题视觉验证
- [complete] 5. 整理 GPUI 实现规范与设计系统文档

## 原生实现阶段

- [complete] 1. 初始化 Git 仓库并提交设计基线
- [complete] 2. 建立 Rust workspace 与可测试的策略状态模型
- [complete] 3. 固定官方 GPUI 依赖并建立三平台启动配置
- [complete] 4. 实现宽/中/窄三档原生策略工作台
- [complete] 5. 用 GPUI 离屏渲染验证浅色、深色与紧凑交互流
- [complete] 6. 完成独立视觉终审、全量 Rust 检查与版本提交

## Mihomo 只读集成阶段

- [complete] 1. 核验官方控制器端点、鉴权方式与字段漂移风险
- [complete] 2. 以测试驱动建立 std-only 只读 HTTP 客户端和容错 JSON 模型
- [complete] 3. 将控制器快照映射为拥有所有权的 `PolicyCatalog`
- [complete] 4. 在 GPUI 后台任务中连接/刷新，保持演示、连接、成功、失败状态
- [complete] 5. 分离本地预测与 `/connections` 已观察证据，并生成真实连接快照
- [complete] 6. 完成安全复核、文档、全量验证和 Git 提交

## 真实订阅兼容验证阶段

- [complete] 1. 通过现有 Clash Verge Mihomo 控制器确认真实 API 可用
- [complete] 2. 以失败测试建立 macOS/Linux Unix socket 只读传输
- [complete] 3. 修复真实 `/proxies` 字段漂移、默认组排序和缺失值语义
- [complete] 4. 用默认忽略的 live smoke test 验证真实目录映射
- [complete] 5. 将 live screenshot 限制到系统临时目录并完成视觉复核
- [complete] 6. 完成全量验证、安全复核和 Git 提交

## Relay 托管 Mihomo 进程阶段

- [complete] 1. 核验官方发行、许可证、启动参数和跨平台进程边界
- [complete] 2. 以测试驱动建立独立 engine 配置、数据目录和生命周期状态
- [complete] 3. 实现不接管外部进程的安全 spawn/health/stop 管理器
- [complete] 4. 用隔离假进程验证启动、就绪、失败和清理路径
- [complete] 5. 接入 GPUI 的托管/外部控制器状态，但暂不下载二进制或订阅
- [complete] 6. 完成安全复核、全量验证、文档和 Git 提交

## 订阅与 QX 风格配置编译阶段

- [completed] 1. 核验 Mihomo `proxy-providers`、策略组、规则与最小运行配置契约
- [completed] 2. 以 TDD 建立不泄露订阅密钥的 Relay profile 领域模型
- [completed] 3. 实现确定性、安全转义的 Mihomo YAML 编译器
- [completed] 4. 实现 `0700` runtime 与 `0600` 原子配置写入
- [completed] 5. 将显式订阅开发模式接入托管 engine，保留现有配置与外部模式
- [completed] 6. 用 fixture/本机 Mihomo 校验、安全复核、全量验证和 Git 提交

## 应用内配置工作区阶段

- [completed] 1. 映射现有 GPUI 导航、状态、响应式布局与截图交互边界
- [completed] 2. 以 TDD 建立不持有订阅密钥的配置工作区 view model
- [completed] 3. 实现订阅源、策略组和有序规则的宽/中/窄原生界面
- [completed] 4. 接入安全运行状态与 profile 预览，避免伪造持久化能力
- [completed] 5. 生成原生截图并按 Visual Verdict 完成视觉修正
- [completed] 6. 完成独立终审、全量验证、敏感信息扫描与 Git 提交

## 订阅源反馈与调试日志修复

- [completed] 1. 复现并定位默认订阅源重复点击无可见反馈
- [completed] 2. 以 TDD 建立订阅源安全诊断展开/收起状态
- [completed] 3. 实现可点击订阅卡片与明确的诊断详情
- [completed] 4. 加入统一脱敏 UI/连接事件日志与运行说明
- [completed] 5. 完成原生交互截图、视觉门禁、全量验证和 Git 提交

## 应用内订阅输入修复

- [completed] 1. 用 trace 证明点击事件已收到，并定位真正缺失的是输入控件
- [completed] 2. 以 TDD 定义 HTTPS 校验、QX 结构预览与脱敏错误契约
- [completed] 3. 实现支持键盘、粘贴、选择和 IME 的原生 GPUI 单行输入
- [completed] 4. 将诊断折叠区替换为始终可见的输入、校验、清除和结果反馈
- [completed] 5. 生成宽屏/紧凑成功状态截图并通过 Visual Verdict
- [completed] 6. 完成全量验证、安全扫描、Git 提交和应用重启

## 多来源、代理模式与运行状态工作区

- [completed] 1. 将节点移动到一级导航首位并支持多订阅来源分组
- [completed] 2. 持久保存单个 VLESS 节点与分组折叠状态
- [completed] 3. 实现关闭、系统 HTTP/SOCKS、TUN 三态代理控制
- [completed] 4. 增加网络活动与安全事件日志工作区
- [completed] 5. 完成视觉门禁、安全复核、全量验证、提交和重启

## 可运行 VLESS 与实时运行遥测

- [completed] 1. 映射现有 profile/engine/controller 边界与 Mihomo 实时流契约
- [completed] 2. 以 TDD 扩展 VLESS 领域模型和确定性 Mihomo 配置编译
- [completed] 3. 将已保存 VLESS 安全合并进 Relay 托管运行配置并支持重载
- [completed] 4. 实现有界、可停止的实时网络活动与内核日志流
- [completed] 5. 完成 GPUI 实时状态、错误/空状态和响应式视觉验证
- [completed] 6. 完成安全复核、全量验证、Git 提交和应用重启

## 节点库存与用户策略分组

- [completed] 1. 映射现有节点来源、持久化、策略编译与响应式界面边界
- [completed] 2. 以 TDD 建立可重命名、换图标和规则化选点的分组领域模型
- [completed] 3. 持久化用户分组并编译为 Relay 托管 Mihomo 策略组
- [completed] 4. 在节点页分离“导入的节点”与“节点分组”，实现完整编辑交互
- [completed] 5. 生成宽屏/紧凑原生截图并通过 Visual Verdict
- [in_progress] 6. 完成安全复核、全仓验证、Git 提交和应用重启

## 固定约束
- 实现框架：Rust + GPUI
- 目标平台：Windows、macOS、Linux
- 核心体验：规则 → 策略组 → 节点必须直观、可解释
- 不复制 Quantumult X 界面；不以编辑 YAML 为主要交互
- 统一品牌与组件语言，只对标题栏、菜单、快捷键等平台惯例做局部适配

## 错误记录
| 错误 | 次数 | 处理 |
|---|---:|---|
| Impeccable 决策页检测不到可用浏览器并退出 | 1 | 不重试同一路径；改用已安装的内联可交互可视化，并让选择按钮通过对话 follow-up 返回用户选择 |
| browser-use 中误用不存在的 `screenshot()` helper | 1 | 读取错误给出的接口并改用 `capture_screenshot()`；成功保存页面截图 |
| Alacritty + Codex CLI 显示 `Visualization unavailable on this device` | 1 | 确认 CLI 不支持内联可视化；建立 `codex-visualize-open` 浏览器后备与全局 AGENTS 规则 |
| 路由解释可能被误读为 Mihomo 的实时裁决 | 1 | 明确区分“本地规则预测”和来自 `/connections` 的“已观察连接”，并由独立评审复核通过 |
| macOS 未授权窗口共享导致外部 UI 自动化无法捕获 GPUI 窗口 | 1 | 改用 GPUI `VisualTestAppContext` 直接捕获真实渲染纹理，并加入坐标点击烟测 |
| 严格 Clippy 将声明式视图长度和 `RRGGBB` 色值视为问题 | 1 | 修复实际告警，仅对四个组合视图和标准色值表示做窄范围豁免 |
| 明文控制器地址可能把 Bearer secret 发往远端 | 1 | 本阶段限制为 localhost/IPv4/IPv6 回环地址，并二次验证 DNS 解析结果 |
| Bearer secret 可包含 HTTP 控制字符 | 1 | 请求构造前拒绝控制字符，并以回归测试覆盖 header injection |
| 真实 Mihomo 将 `fixed` 返回为节点名而非布尔值 | 1 | 删除 UI 未使用的脆弱字段绑定，让 Serde 按未知字段忽略并保留兼容性回归样本 |
| 首次 live screenshot 误选内置 GLOBAL 且把空 host/0 ms 当有效值 | 1 | GLOBAL 排在普通策略之后，空 host 回退目标 IP，0 ms 视为未知；第二轮视觉判定通过 |
| live screenshot 命令使用 zsh 只读变量名 `status` | 1 | 不重试该变量名；改用任务专用 `task_exit_code` 后成功 |
| 一次 `cargo test` 误传多个位置过滤参数 | 1 | Cargo 只接受一个 TESTNAME；改为运行整个 `relay-engine` 测试集并读取三项预期失败 |
| GPUI 代码映射 explorer 命中 GPT-5.3-Codex-Spark 用量上限 | 1 | 不重试同一模型；由主代理使用本地结构搜索完成映射，保留独立 designer 评审 |
| `omx explore` 已被当前 OMX 硬弃用 | 1 | 按命令自身迁移提示停止重试，改用正常 `rg`/定向读取；验证类噪声命令仍可用 sparkshell |
| GPUI 输入范式 explorer 命中 GPT-5.3-Codex-Spark 用量上限 | 1 | 不重试同一模型；直接采用项目锁定 GPUI revision 的官方 `examples/input.rs` 范式 |
| 节点分组测试首个补丁的 import 上下文与当前文件不一致 | 1 | 读取实际 import 与测试位置后，用精确上下文重新应用；首个失败未修改代码 |
| 只读 UX 子任务遗留了与领域模型冲突的半成品改动 | 1 | 中断该子任务，移除伪延迟阈值/协议规则模型，保留可复用输入改造后统一实现 |
| 首轮严格 Clippy 报告缺少 Errors 文档与声明式 UI 行数限制 | 1 | 补齐公开 Result 文档、派生 Default，并仅对声明式渲染/事务函数做局部 allow |
| `relay-ui` RED 测试撞上 profile 执行代理的中间编辑窗口 | 1 | 不重试并发编译；等待其完成独占文件后再运行，避免误判共享工作树中的暂态错误 |
