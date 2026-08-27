# Manis UI → GPUI 实现规范

状态：设计基线 v0.1；产品与仓库名已确认为 `Manis`。

## 1. 体验原则

主对象不是“VPN 是否连接”，而是可追踪的路由链：

`请求/连接 → 首条命中规则 → 策略组 → 最终节点`

- 普通用户先看到“为什么这样走”，需要时再展开 Mihomo 术语。
- 策略组切换节点是一等操作；YAML 只作为高级逃生口。
- 宽窗口同时展示上下文，窄窗口一次完成一个任务，不压缩三栏。
- 预测结果与运行事实必须明确区分，不能把界面推演冒充 Mihomo 已观察结果。

## 2. 窗口尺寸类

GPUI 根据内容区逻辑宽度计算，不使用操作系统名称分支布局。

| Size class | 内容宽度 | 主导航 | 策略列表 | 策略详情 | 路由解释 |
|---|---:|---|---|---|---|
| `Wide` | `>= 1280` | 220 px 文本侧栏 | 326 px | 弹性 | 340 px 常驻 |
| `Medium` | `900–1279` | 66 px 图标轨 | 292 px | 弹性 | 340 px side sheet |
| `Compact` | `640–899` | 56 px 图标轨 | 单独页面 | 单独页面 | 340 px side sheet |

- 建议最小窗口：`640 × 560` 逻辑像素。
- `Compact` 中选择策略后进入详情；详情头部必须提供可聚焦的返回按钮。
- 宽度变化只改变布局状态，不重新创建领域状态；选中的策略、节点、筛选词和滚动位置应保留。
- 平台只影响原生标题栏、菜单、快捷键标签、托盘和系统权限提示。

## 3. 设计 Tokens

所有组件只能依赖语义 token，不直接读取十六进制色值。

### 颜色

| Token | Light | Dark | 用途 |
|---|---|---|---|
| `surface.base` | `#F4F7F5` | `#0E1715` | 应用内容底色 |
| `surface.low` | `#EDF2EF` | `#111D1A` | 导航、解释面板 |
| `surface.high` | `#FFFFFF` | `#172521` | 编辑/列表主面 |
| `surface.chrome` | `#E7EEEA` | `#13211E` | 应用栏、状态栏 |
| `text.primary` | `#152321` | `#E3EEEA` | 主文本 |
| `text.secondary` | `#5F6E69` | `#A4B4AE` | 描述文本 |
| `text.tertiary` | `#84918D` | `#7D8E88` | 非关键元数据 |
| `outline.subtle` | `#CBD6D2` | `#2B3D37` | 分隔线 |
| `outline.strong` | `#9FAFA9` | `#435851` | 输入框、插口边界 |
| `action.primary` | `#176C62` | `#79D7C6` | 主操作、选中态 |
| `action.on_primary` | `#FFFFFF` | `#082A24` | 主操作上的内容 |
| `route.trace` | `#D46642` | `#F39B75` | 当前规则/路由轨迹 |
| `status.success` | `#24795F` | `#79D7B0` | 可用、低延迟 |
| `status.warning` | `#A96620` | `#EFB96E` | 退化、较慢 |
| `status.error` | `#B54F49` | `#EF8C84` | 失败、不可用 |
| `focus.ring` | `#0B6FBB` | `#82C8FF` | 键盘焦点 |

`route.trace` 只表达“当前被解释的路径”，不得用于普通装饰或成功状态。

### 尺寸

```text
space: 4, 8, 12, 16, 24, 32
radius.control: 8
radius.row: 8
radius.pane: 12
radius.window: 18 (仅浏览器设计稿；生产使用平台窗口形状)
icon.small: 16
icon.standard: 18
control.compact: 34
control.standard: 38
pointer_target.minimum: 32
touch_capable_target.minimum: 44
```

### 字体

- UI：平台系统字体；Windows `Segoe UI Variable`，macOS `SF Pro`，Linux 优先 `Noto Sans`/桌面环境 UI 字体。
- 中文：系统 CJK fallback，禁止为一个平台打包后导致另两个平台字重跳变。
- 数据：仅域名规则、地址、速率、版本号使用系统等宽字体。
- 所有延迟、速率和序号使用 tabular numerals。

### 动效

```text
fast: 140 ms
standard: 220 ms
enter: cubic-bezier(.05, .7, .1, 1)
state: cubic-bezier(.2, 0, 0, 1)
exit: cubic-bezier(.3, 0, .8, .15), 160 ms
```

- 唯一品牌动效：测试/观察到路由结果后，铜色信号从规则插口依次抵达策略与节点。
- side sheet 只用位移和阴影，不动画宽度。
- Reduced Motion 下立即切换，只保留颜色/文本状态更新。

## 4. 组件边界

```text
ManisWindow
├── AppChrome
│   ├── GlobalSearch
│   ├── SystemProxyToggle
│   └── RuntimeIndicator
├── WorkspaceShell
│   ├── PrimaryNavigation
│   └── PolicyWorkspace
│       ├── PolicyGroupList
│       ├── PolicyGroupDetail
│       │   ├── PolicyHeader
│       │   ├── PolicyTabs
│       │   ├── NodeTable
│       │   └── MatchingRuleList
│       └── RouteInspector
│           ├── RouteQuery
│           ├── SignalPath
│           └── RouteEvidence
└── RuntimeStatusBar
```

### 组件职责

- `PolicyGroupList`：只渲染摘要和选择动作；不直接调用 Mihomo。
- `PolicyGroupDetail`：组合节点与规则视图；持有焦点/滚动表现，不持有领域数据副本。
- `NodeTable`：列表虚拟化、单选语义、延迟状态；提交 `SelectNode` intent。
- `RouteInspector`：展示 `RouteExplanation`；不自行推断规则。
- `SignalPath`：纯展示组件；输入是三个已解析 stage，不读取全局 store。
- `RuntimeStatusBar`：消费聚合后的运行状态，不订阅每条连接。

## 5. Rust 状态模型

以下是结构约束，不绑定某个 GPUI commit 的具体 API 名称。

```rust
struct PolicyWorkspaceState {
    size_class: WindowSizeClass,
    selected_group: Option<PolicyGroupId>,
    selected_node: Option<ProxyId>,
    filter: String,
    tab: PolicyTab,
    navigation: CompactNavigation,
    inspector: InspectorState,
    latency_test: LatencyTestState,
}

enum WindowSizeClass { Compact, Medium, Wide }
enum PolicyTab { Nodes, Rules, Settings }
enum CompactNavigation { GroupList, GroupDetail }
enum InspectorState { Closed, Open(RouteExplanationState) }

enum RouteExplanationState {
    Empty,
    Predicting { query: RouteQuery, generation: u64 },
    Predicted(PredictedRoute),
    Observed(ObservedRoute),
    Failed { query: RouteQuery, error: RouteExplainError },
}

enum LatencyTestState {
    Idle,
    Running { group: PolicyGroupId, generation: u64 },
    Complete { group: PolicyGroupId, finished_at: Instant },
    Failed { group: PolicyGroupId, error: HealthCheckError },
}
```

使用 ID 关联实体；不要把完整 `PolicyGroup`/`Proxy` 克隆进每个 view。业务枚举必须穷举匹配，新增状态时让编译器指出遗漏分支。

## 6. Intent 与异步任务

```rust
enum PolicyIntent {
    SelectGroup(PolicyGroupId),
    SelectNode { group: PolicyGroupId, proxy: ProxyId },
    ChangeTab(PolicyTab),
    FilterGroups(String),
    StartLatencyTest(PolicyGroupId),
    Explain(RouteQuery),
    OpenInspector,
    CloseInspector,
    NavigateBack,
}
```

- View 发 intent；controller/service 执行副作用；完成结果再写回 entity。
- 测速、规则解析和 API 请求不得阻塞 GPUI 主线程。
- 每次异步请求分配递增 `generation`；只接受仍与当前 generation 相同的结果，丢弃过期回包。
- 策略节点切换使用乐观 UI 时必须保留原值；Mihomo 更新失败后回滚，并给出“失败原因 + 重试”而不是泛化 toast。
- UI 层不得直接持有 `Arc<Mutex<...>>` 并在 render 中上锁；共享运行状态通过 entity/snapshot 或有界消息通道进入 UI。

## 7. 路由解释的数据可信度

Mihomo 官方 API 的能力边界：

- `/rules`：返回运行中的规则顺序、类型、payload、目标策略和命中统计。
- `/connections`：对真实活跃连接返回 `chains`、`providerChains`、`rule`、`rulePayload`；这是“已观察”解释的权威来源。
- `/dns/query`：只负责 DNS 查询，不等于路由 dry-run。
- 当前公开 API 没有“输入任意域名并返回完整匹配链”的 dry-run 端点。

因此产品提供两种明确状态：

1. `预测路径`：客户端使用与配置编译器共享的结构化规则模型进行本地评估；必须显示“预测”，并列出无法仅凭域名确定的条件，例如进程、源地址、端口、网络类型和 DNS 后的 IP 规则。
2. `已观察连接`：从 Mihomo `/connections` 获取实际 `rule/rulePayload/chains`；显示“已观察”和连接时间，可作为最终事实。

不得把预测路径标成 “Mihomo runtime 已确认”。如果本地评估无法确定，结果应是 `需要实际连接`，而不是猜一个节点。

## 8. 可访问性与键盘

| 操作 | 快捷键 |
|---|---|
| 全局搜索 | `Cmd/Ctrl + K` |
| 切换导航区域 | `Cmd/Ctrl + 1…6` |
| 聚焦策略筛选 | `/`（文本输入未聚焦时） |
| 上/下选择策略或节点 | `↑ / ↓` |
| 确认节点 | `Enter` |
| 打开/关闭路由解释 | `Cmd/Ctrl + I` / `Esc` |
| 返回 compact 列表 | `Alt + ←`，macOS 同时支持 `Cmd + [` |
| 开始测速 | `Cmd/Ctrl + Shift + T` |

- 焦点顺序必须跟视觉顺序一致。
- 选中态同时用 radio/文本和表面色表达，不能只依赖颜色。
- side sheet 打开后，首次焦点进入标题或查询输入；关闭后返回触发按钮。
- 实时流量不逐帧通知读屏器；状态栏只在运行状态、配置应用结果和错误发生时公告完整句子。
- 延迟数字变化不主动抢焦点。

## 9. 平台适配

### Windows

- 使用原生窗口控制区；菜单和快捷键显示 `Ctrl`。
- 系统代理/TUN 权限失败时指向 Windows 设置或服务状态。

### macOS

- 尊重左上角窗口控制区和原生菜单栏；快捷键显示 `⌘`。
- 不在内容层重画 traffic lights。

### Linux

- 客户端装饰遵循桌面环境；不要假设托盘一定存在。
- 字体、缩放和 GTK/KDE 高对比主题需要单独验证。

共享内容区保持同一 token、层级与交互语义；平台差异不能分叉成三套产品。

## 10. 性能预算

- 策略组数量通常较小，可普通渲染；连接、日志和大型规则列表超过 50 行必须虚拟化。
- `/connections` WebSocket 更新先在后台合并，再以 100–250 ms 节流快照更新 UI。
- 状态栏速率最多每秒刷新 4 次；屏幕不可见时降频。
- 规则筛选在 80–120 ms 后执行；短列表可即时执行。
- 渲染函数只做派生展示，不进行 DNS、JSON 解析或锁等待。

## 11. 建议目录

```text
crates/manis-ui/src/
├── app_shell/
│   ├── mod.rs
│   ├── chrome.rs
│   ├── navigation.rs
│   └── status_bar.rs
├── design_system/
│   ├── color.rs
│   ├── metrics.rs
│   ├── typography.rs
│   ├── motion.rs
│   └── components/
├── policies/
│   ├── mod.rs
│   ├── model.rs
│   ├── controller.rs
│   ├── group_list.rs
│   ├── group_detail.rs
│   ├── node_table.rs
│   └── route_inspector.rs
└── platform/
    ├── mod.rs
    ├── windows.rs
    ├── macos.rs
    └── linux.rs
```

领域模型、Mihomo client 和配置编译器不放进 `manis-ui`；UI 通过窄 trait/command 接口使用它们。

## 12. 第一阶段验收

- 宽/中/窄三种尺寸切换时状态不丢失、无横向溢出。
- light/dark/high-contrast 下正文对比度不低于 4.5:1，边界和焦点不低于 3:1。
- 全流程可只用键盘完成：选策略 → 选节点 → 打开解释 → 返回。
- 节点更新失败会回滚，并显示可恢复错误。
- 预测路径和已观察连接的标签、数据来源和置信度不可混淆。
- Reduced Motion 下不播放信号脉冲和 side-sheet 滑动。
- 规则/连接长列表在目标三平台上保持 60 fps 滚动。
