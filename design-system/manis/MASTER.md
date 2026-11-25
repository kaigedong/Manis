# Manis UI 2.0 — Master Design System

> **Creative North Star: "Obsidian & Alabaster — Swiss Precision Routing"（墨石与霜白 · 瑞士精密路由仪表）**
>
> 全新设计的统一跨端 UX 设计系统，彻底告别旧版“Signal Patch Bay（冷青绿 + 铜色 + 灰蓝石板）”与移动端“薄荷绿 Material 3”的割裂语言。桌面端（Rust GPUI）与移动端（Android Jetpack Compose）共享同一套色彩体系、排版秩序、路由语义文法与组件解剖学，同时严格尊重各自平台的交互人体工学。

---

## 1. 为什么彻底重构（旧版核心痛点诊断）

| 维度 | 旧版问题 | 新版 2.0 解决方案 |
| --- | --- | --- |
| **双端割裂** | 桌面为冷蓝灰 + 深青绿 + 铜色；Android 为薄荷绿 Material 3 + 冰蓝路由色，两套圆角、字阶、结构不统一 | 双端 100% 同源 Token：`tokens.json` 单一真源直译到 `theme.rs` 与 `Theme.kt` |
| **核心价值隐身** | `规则 → 策略组 → 出口节点` 淹没在普通灰表与单行纯文本中，缺乏视觉识别度 | 引入专属 **Route Truth Chain（路由真值链路）** 视觉组件与皇家紫专属语义色 |
| **控制重心散乱** | 桌面右上角代理开关与路由模式等权并列、无状态焦点；移动端底部 140dp 固定连接栏叠加 72dp 导航栏吃掉 210dp+ 视野 | 桌面升级为 **顶部指挥栏 + 左侧状态遥测轨**；移动端升级为 **顶部 Bento 连接驾驶舱 + 滚动吸顶迷你条 + 64dp 极简底栏** |
| **数据密度失衡** | 节点表格像旧后台列表，流量/到期信息被塞进假节点行；设置目录留白失衡 | 策略组升级为 **Bento 交互卡片网格**，订阅配额进度条内嵌源头卡片，延迟/流量/IP 全部采用 **等宽表格数字（Tabular Mono）** |

---

## 2. 视觉语言与色彩系统（WCAG AA/AAA 全校验）

### 2.1 设计基调
- **浅色模式 — 暖霜纸感（Alabaster Paper）**：采用微暖建筑石膏白底（`#F7F6F3`）承托纯白卡片（`#FFFFFF`），以 1px 暖石细边（`#E5E2DC`）与明度差建立纯粹层次，消除冷灰医疗感。
- **深色模式 — 曜石暗房（Obsidian Studio）**：采用纯中性曜石黑（`#0D0D0F`）与精密分层炭灰（`#141417` / `#1C1C21` / `#25252C`），无蓝泥感、无绿偏色，在 OLED 移动端与桌面暗屏上同样通透锐利。
- **不透明工作区原则（Opaque Workspace Rule）**：所有主内容、卡片、弹窗与编辑器均为 100% 不透明实体表面，不依赖壁纸模糊，完全兼容 GPUI `WindowBackgroundAppearance::Opaque`。

### 2.2 语义色板对照表

| 语义角色 | Light Hex | Dark Hex | 对比度保障 | 唯一用途 |
| --- | --- | --- | --- | --- |
| `surface.canvas` | `#F7F6F3` | `#0D0D0F` | — | 页面主底色 |
| `surface.chrome` | `#EFECE6` | `#141417` | — | 导航轨、顶部指挥栏、移动端底栏 |
| `surface.low` | `#F1EFEA` | `#17171B` | — | 分段控件槽位、输入框底、浅层信息条 |
| `surface.elevated` | `#FFFFFF` | `#1C1C21` | — | 策略组卡、来源卡、规则组容器、设置卡 |
| `surface.high` | `#E6E3DC` | `#25252C` | — | 列表悬停、按下反馈、次级表面抬升 |
| `text.primary` | `#141413` | `#F4F4F6` | ≥ 15.8:1 | 标题、节点名、规则主体、主数值 |
| `text.secondary` | `#4E4D49` | `#B4B4BE` | ≥ 7.4:1 | 说明文案、策略标签、协议类型 |
| `text.tertiary` | `#5E5C57` | `#9A9AA6` | ≥ 5.5:1 | 元数据、时间戳、空态提示（全表面 ≥ 4.5:1） |
| `accent.primary` | `#2B4EFF` | `#6B8AFF` | ≥ 5.6:1 (配 `onPrimary`) | **电钴靛蓝**：主要 CTA、选中高亮、插入线、键盘焦点 |
| `accent.soft` | `#EEF2FF` | `#1B2342` | — | 选中行底色、激活胶囊底色 |
| `route.trace` | `#6D28D9` | `#C4B5FD` | ≥ 7.2:1 (配 `route.soft`) | **皇家紫（路由专属色）**：仅用于 `规则 → 策略组 → 节点` 链路与目标胶囊 |
| `status.success` | `#0E7A4B` | `#4ADE80` | ≥ 5.2:1 | **通路翠绿**：代理已连通、低延迟 `<120ms` |
| `status.warning` | `#9A5504` | `#FBBF24` | ≥ 5.5:1 | **暖珀琥珀**：中延迟 `120–350ms`、待应用变更、警告 |
| `status.error` | `#C81E1E` | `#F87171` | ≥ 5.6:1 | **朱砂红**：高延迟 `>350ms`、超时、错误、危险操作 |

> **严格色彩纪律**：
> 1. **路由专属规则**：皇家紫（`route.trace`）只表达路由链路与命中结果，绝不用于普通按钮或装饰。
> 2. **非纯色依赖规则**：所有状态（连通/暂停、延迟分级、日志级别）必须同时具备 **图标/圆点 + 文字标签**，确保色觉障碍用户无损识别。
> 3. **危险按钮独立对**：破坏性操作始终使用 `#C81E1E` 填充 + `#FFFFFF` 文字，不从深色主按钮反色派生。

---

## 3. 排版与精密数据体系（Typography & Tabular Data）

- **界面字体（Sans）**：`Inter`（系统回退：`SF Pro Text` / `PingFang SC` / `Noto Sans SC` / `Roboto`）。标题收紧字距（`-0.02em`），正文保持舒适行高（`1.5`）。
- **技术数据字体（Mono + Tabular Nums）**：`JetBrains Mono`（系统回退：`SF Mono` / `Roboto Mono`），**强制开启等宽数字（`tabular-nums`）**，覆盖所有延迟（`38 ms`）、速率/流量（`↓ 14.2 MB`）、出口 IP、规则序号（`01`）、时间戳与日志等级，杜绝实时刷新时数字抖动。

| 角色 | 桌面端 (GPUI) | 移动端 (Compose) | 字重 | 用途 |
| --- | --- | --- | --- | --- |
| `Display Metric` | 24px / 30px | 26px / 32px | 600 | 连接状态主数值、核心统计 |
| `Page Title` | 22px / 30px | 24px / 32px | 600 | 工作区页面标题 |
| `Section Title` | 15px / 22px | 16px / 22px | 600 | 分区标题（策略组、订阅来源、设置分组） |
| `Body` | 13.5px / 20px | 14.5px / 21px | 400 | 列表主体、说明文本、表单输入 |
| `Label` | 13px / 18px | 14px / 20px | 500 | 导航项、按钮、徽标、卡片主行 |
| `Mono Data` | 12px / 18px | 13px / 18px | 500 | 延迟、流量、IP、域名、规则表达式 |
| `Meta` | 11.5px / 16px | 12px / 16px | 400 | 次级元数据、时间、提示语 |

---

## 4. 空间网格、圆角与动效节奏

- **4px 基础网格**：`4 / 8 / 12 / 16 / 24 / 32px` 严格阶梯，杜绝任意像素值。
- **统一圆角阶梯**：
  - 徽标 / 协议标签 / 区域码：`6px`
  - 按钮 / 输入框 / 列表选中行：`10px`
  - 卡片 / 面板 / 规则组容器：`14px`
  - 弹窗 / 移动端底部抽屉：`20px`
  - 状态胶囊 / 开关 / 分段滑块：`9999px`（Full Pill）
- **图标规范（Zero Emoji）**：
  - 全应用**严禁使用 Emoji**（包括国旗 Emoji 与状态 Emoji）。
  - 全部使用统一的 **1.75px 线宽矢量 SVG 图标**（Lucide 几何风格）；节点地区统一采用 crisp 2 字母 ISO 等宽胶囊（`HK`、`SG`、`JP`、`US`、`TW`）。
  - 品牌标识沿用官方 `assets/brand/manis-mark.svg`，置于 `28px`（桌面）/`32px`（移动）精致圆角方框中，不加多余白底。
- **克制动效（尊重 `prefers-reduced-motion`）**：
  - 微交互（悬停/按下/开关）：`140ms ease-out`
  - 状态切换（分段滑块、卡片展开、工作区淡入）：`220ms cubic-bezier(0.16, 1, 0.3, 1)`
  - 抽屉/弹窗进场 `280ms`，退场 `180ms`（退场快于进场）。

---

## 5. 统一信息架构（5 个核心工作区）

双端使用完全一致的工作区心智模型与中文文案：

| 顺序 | 工作区 | 图标 | 核心职责 | 桌面承载 | 移动承载 |
| --- | --- | --- | --- | --- | --- |
| 1 | **节点** | Globe / Nodes | 代理开关与模式、策略组切换、订阅节点与测速 | 主内容区（策略组 Bento 网格 + 订阅源卡片） | 顶部 Hero 连接驾驶舱 + 策略组 + 订阅源 |
| 2 | **分流规则** | Layers / Split | 优先级管道（规则组 → 手动规则 → 兜底）、拖拽排序 | 优先级管道列表 + 原生拖动/键盘排序 + 独立编辑窗 | 优先级管道 + 即时拖动把手 + 底部表单抽屉 |
| 3 | **网络活动** | Activity | 实时连接与 **路由真值链路**、搜索、一键建规则 | 实时表格 + 3 段路由链路胶囊 + 右键/点击建规则 | 精简连接卡 + 3 段路由链路 + 点击弹出建规则抽屉 |
| 4 | **日志** | Terminal | 内核与界面日志、等级过滤胶囊、搜索、一键复制 | 等宽日志流 + 级别筛选胶囊 + 顶部搜索 | 等宽日志流 + 级别筛选胶囊 + 搜索与复制 |
| 5 | **设置** | Sliders | 通用、运行内核、代理来源、规则来源、高级与备份、关于 | 侧栏入口 / `Cmd+,` 独立原生设置窗口（6 章节目录） | 第 5 底部页签（统一 6 章节分组卡片 + 顶部快速目录） |

---

## 6. 核心签名组件（Signature Components）

### 6.1 Route Truth Chain（路由真值链路组件）
Manis 的标志性视觉语法，清晰区分 **内核实测链路（Observed）** 与 **本地预测（Predicted）**：
```
[ ● 命中: DOMAIN-SUFFIX openai.com ] ──▶ [ 策略组: AI 自动选择 ] ──▶ [ 出口: SG-02 · 54 ms ]
```
- 规则段：皇家紫轻底（`route.soft`）+ 皇家紫文字（`route.trace`）
- 策略组段：中性抬升表面 + 细边框
- 出口节点段：通路翠绿圆点 + 等宽延迟数值；直连显示中性 `DIRECT`，拒绝显示警示红 `REJECT`

### 6.2 Segmented Control（精密分段切换器）
- 用于 **路由模式（`直连` / `规则` / `全局`）** 与桌面端 **代理模式（`已暂停` / `系统代理` / `TUN 代理`）**。
- 槽位底色 `surface.low` + `1px border.subtle`；选中项使用 `surface.elevated` 实体滑块 + 极轻投影（浅色）或 `surface.high`（深色）+ 加粗文字与状态色点，一眼可辨当前激活项。

### 6.3 Latency & Region Pill（延迟与地区胶囊）
- 地区：`HK` / `SG` / `JP` / `US` 等宽 2 字符方标（`surface.low` + `text.secondary`）
- 延迟：等宽数字 + 3 级信号柱（`<120ms` 翠绿 / `120–350ms` 琥珀 / `>350ms` 朱砂红 / 超时 `—` 中性）

### 6.4 Rule Priority Pipeline（规则优先级管道）
- 每组左侧配有 **2 位等宽优先级序号（`01`, `02`, `03`）** 与 **六点拖动手柄（Grip）**。
- 垂直引导细线将规则组与末尾 **默认兜底（Fallback）** 节点串联，让新手 1 秒看懂“从上到下依次匹配”。
- 拖拽时显示电钴蓝 2px 插入线 + 浮动标题胶囊；键盘聚焦把手显示 2px 电钴蓝焦点框，支持 `Alt + ↑/↓`。

---

## 7. 双端工程落地映射（1:1 Implementation Map）

- **桌面端 Rust GPUI** (`crates/manis-ui/src/`):
  - `theme.rs`：将 `Theme::light()` / `Theme::dark()` 直接替换为 `tokens.json` 色值，更新 `Radius` (`10/14px`)、`TextRole`（引入 `MonoData`）、同步 `project_component_palette`，保持并升级 `all_normal_text_roles_meet_aa_on_every_content_surface` 自动单测！
  - `components.rs`：新增 `route_chain_badge()`、`segmented_control()`、`latency_pill()`、`region_code_badge()`、`usage_meter()`。
  - `app/policy_workspace/chrome.rs`：升级为 **左轨导航 + 底部实时遥测卡 + 顶部整合指挥栏**。
  - `app/nodes/view.rs`：策略组升级为自适应 2 列 Bento 卡片，订阅源卡片头部集成配额进度条。
  - `app/activity.rs` & `app/configuration/manual_rules/`：落地 Route Truth Chain 与规则优先级管道视觉。
- **移动端 Android Jetpack Compose** (`android/app/src/main/java/dev/manis/app/ui/`):
  - `Theme.kt`：彻底移除薄荷绿 M3 调色板，替换为 `tokens.json` 的 Alabaster & Obsidian 精确色值与 `ManisDimens`（触控最小 `48.dp`）。
  - `Shell.kt`：升级为 `64.dp` 极简底部导航栏（严格 safe-area 适配）+ 桌面同源图标集。
  - `NodesWorkspace.kt`：移除 `140.dp` 底部固定条，改为顶部 **Hero 连接驾驶舱 Bento 卡** + 滚动吸顶迷你状态条，释放 80dp+ 列表可视空间。
  - `RulesWorkspace.kt` / `ActivityWorkspace.kt` / `LogsWorkspace.kt` / `ConfigurationWorkspace.kt`：全面对齐路由真值链路、优先级序号管道、日志级别过滤胶囊与 Bottom Sheet 编辑器。

> 详细双端页面规范见：[desktop.md](pages/desktop.md) · [mobile.md](pages/mobile.md)
> 可交互视觉原型见：[preview.html](preview.html)
