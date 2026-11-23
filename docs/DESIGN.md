---
name: Manis
description: Quiet native network workspace with clear reading and action hierarchy
colors:
  surface-base-light: "#fafbfc"
  surface-low-light: "#f0f3f5"
  surface-high-light: "#e9eef2"
  surface-overlay-light: "#ffffff"
  surface-chrome-light: "#e9eef2"
  text-primary-light: "#0d0d0d"
  text-secondary-light: "#5d5d5d"
  text-tertiary-light: "#666666"
  outline-subtle-light: "#dde3e8"
  outline-strong-light: "#78828c"
  action-primary-light: "#176c62"
  action-primary-hover-light: "#125d54"
  action-primary-active-light: "#0e4e46"
  action-on-primary-light: "#ffffff"
  action-soft-light: "#e3f0ed"
  button-bg-light: "#edf1f4"
  button-hover-light: "#e2e8ed"
  button-active-light: "#d5dfe6"
  focus-ring-light: "#2563eb"
  route-trace-light: "#923b0f"
  route-soft-light: "#fff5f0"
  status-success-light: "#00692a"
  status-warning-light: "#875500"
  status-error-light: "#b42318"
  surface-base-dark: "#15191e"
  surface-low-dark: "#1c2229"
  surface-high-dark: "#242c35"
  surface-overlay-dark: "#242c35"
  surface-chrome-dark: "#1c2229"
  text-primary-dark: "#f3f3f3"
  text-secondary-dark: "#b9b9b9"
  text-tertiary-dark: "#ababab"
  outline-subtle-dark: "#35404c"
  outline-strong-dark: "#858f9b"
  action-primary-dark: "#79d7c6"
  action-primary-hover-dark: "#95e3d4"
  action-primary-active-dark: "#afece0"
  action-on-primary-dark: "#0d0d0d"
  action-soft-dark: "#203b35"
  button-bg-dark: "#29333e"
  button-hover-dark: "#354251"
  button-active-dark: "#415165"
  focus-ring-dark: "#8cb4ff"
  route-trace-dark: "#ffb790"
  route-soft-dark: "#362820"
  status-success-dark: "#73d499"
  status-warning-dark: "#f0c36a"
  status-error-dark: "#ff9a96"
  danger-fill: "#ba2623"
  danger-hover: "#911e1b"
  danger-active: "#6e1615"
  danger-label: "#ffffff"
typography:
  page-title:
    fontFamily: "system-ui, sans-serif"
    fontSize: "24px"
    fontWeight: 600
    lineHeight: "32px"
  section-title:
    fontFamily: "system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 600
    lineHeight: "22px"
  body:
    fontFamily: "system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: "20px"
  label:
    fontFamily: "system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: "16px"
  metadata:
    fontFamily: "system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: "16px"
  data:
    fontFamily: "system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: "18px"
  button-label:
    fontFamily: "system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 500
    lineHeight: "14px"
  button-primary-label:
    fontFamily: "system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: "14px"
rounded:
  control: "8px"
  row: "8px"
  pane: "12px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
components:
  button-primary:
    backgroundColor: "{colors.action-primary-light}"
    textColor: "{colors.action-on-primary-light}"
    rounded: "{rounded.control}"
    height: "38px"
    padding: "0 12px"
    typography: "{typography.button-primary-label}"
  button-secondary:
    backgroundColor: "{colors.button-bg-light}"
    textColor: "{colors.text-primary-light}"
    rounded: "{rounded.control}"
    height: "38px"
    padding: "0 12px"
    typography: "{typography.button-label}"
  button-danger:
    backgroundColor: "{colors.danger-fill}"
    textColor: "{colors.danger-label}"
    rounded: "{rounded.control}"
    height: "38px"
    padding: "0 12px"
    typography: "{typography.button-label}"
---
# Design System: Manis

## Overview

**Creative North Star: "Signal Patch Bay"**

Manis 是安静的原生网络工作台。清晰的标题、说明和当前内容形成阅读顺序，青绿色标出选中状态与提交动作；浅色使用轻微冷色调，深色使用石板色表面。保留紧凑的运行列表，让用户检查主窗口、独立设置或编辑，再通过拖动调整规则优先级。

品牌延续 [PRODUCT.md](PRODUCT.md) 已确认的 Manis 名称与青绿、铜色语义。`assets/brand/manis-mark.svg` 用于小尺寸标识；高细节 `manis-shanshui.svg` 留给大尺寸文档和发布物料。小于 64px 的场景使用抽象标识；macOS 菜单栏仍使用单色透明模板。

本文合并现有方向与实际 GPUI 实现。规范 token 来自 `crates/manis-ui/src/theme.rs`；共享控件来自 `components.rs`。前置信息中的 CSS 字体名称是原生系统字体的可移植表达，GPUI 和系统提供实际字体。原生窗口行为与截图范围见 [interface-design.md](docs/interface-design.md)。

**Key Characteristics:**

- 不透明、轻微着色的原生表面。
- 内容优先，标题、说明与动作层级清楚。
- 青绿控件与选择状态，铜色保留路由语义。
- 紧凑运行列表、独立辅助窗口、可见的拖动反馈。

## Colors

前置信息同时记录浅色和深色 token；按当前主题成对使用，不把两套表面混在一个窗口中。

### Primary

青绿色 `action-primary` 用于主要提交按钮、选中内容、披露图标和拖动插入线。`action-soft` 提供轻微选中或悬停背景；主按钮标签必须使用对应的 `action-on-primary`。常规次操作使用 `button-bg`，保持提交动作可见。

### Neutral

`surface-base` 是正文底色，`surface-low` 是轻微层次，`surface-high` 强调列表标题或交互表面，`surface-chrome` 承载应用栏和状态栏，`surface-overlay` 承载编辑器标题、页脚、弹出内容。深色中部分角色共享值，但职责保持一致。

`text-primary` 承载正文，`text-secondary` 承载说明，`text-tertiary` 承载元数据。使用实际颜色 token，不用降低文字透明度制造次级标签。`outline-subtle` 用于结构分隔，`outline-strong` 用于输入控件边界，`focus-ring` 是独立的蓝色键盘焦点。

### Semantic accents

铜色 `route-trace` / `route-soft` 用于被解释的路由路径；成功、警告、错误色用于健康或操作结果。破坏性按钮使用独立的红色填充与白色标签对；不能从深色主按钮标签派生。日志级别还有蓝色 INFO、紫色 DEBUG、青色 TRACE，并保留中性正文和可读级别文字。

**The Route Accent Rule.** 铜色只表达被解释的路由，不作为普通装饰或提交动作色。

**The Readable State Rule.** 状态、选中与焦点都有文字或图形辅助；普通小文字以 4.5:1、交互边界与键盘焦点以 3:1 为测试目标。

## Typography

UI 沿用 GPUI 的平台系统字体与 CJK fallback。等宽字体用于需要逐字检查的数据和日志；不要把整页说明文字做成代码编辑器。标题与正文遵循前置 token 的角色关系：页面标题、区域标题、正文、标签、元数据、数据。

共享页面标题为 semibold，说明为 normal；标签与数据为 medium。共享按钮沿用 gpui-component 的 14px 小号标签，主按钮 semibold，其余 medium。当前编辑器标题有局部 17px 样式，不能将所有辅助窗口标题误标为 24px 页面标题。

原文提出的 Windows `Segoe UI Variable`、macOS `SF Pro`、Linux `Noto Sans` 显式优先级与全部数字 tabular numerals 尚未作为全局字体规则落地；它们是设计意向，不能作为验证过的当前行为。

## Layout

### Main workspace

`WindowSizeClass` 以内容宽度划分：Wide 为至少 1280px，Medium 为 900–1279px，Compact 为低于 900px。主窗口最小为 640 × 560 逻辑像素。主导航分别为 220px、66px、56px；Wide 显示标签，中窄窗口使用图标轨。

Nodes 当前使用一个页面滚动容器：来源与节点列表在前，策略组在后，以轻微分隔线区分；展开内容保持自然高度并推动后续内容。主要工作区由稳定的应用栏、导航、内容区和底部状态栏组成，操作密度随可用空间调整。标题动作先换行，避免把说明挤成窄列。

原文提出的固定 326/292px 策略列表、340px 常驻解释器或侧滑解释器、compact 列表/详情单页切换是早期布局意向；当前 Nodes 渲染不采用这套多栏编排。保留其“窄窗为真实工作模式”的原则，不据此生成未实现的窗格。

### Native utility windows

设置窗口默认为 980 × 760，代理来源、规则来源、手动规则与策略组编辑器默认为 720 × 660；这些辅助窗口最小均为 480 × 480 逻辑像素。每种窗口对同一个应用模型仅保留一份；重新打开激活现有窗口并保留草稿，主窗口仍可用。

辅助窗口拥有自己的焦点、滚动和关闭行为。长编辑表单使用固定标题与页脚、可滚动正文；正文缩小时页脚主操作仍可见。设置目录在宽窗口为侧栏，紧凑窗口为三列两行；内容滚动与目录当前项同步，底部反馈保持可见。

间距使用前置的 4/8/12/16/24px 尺度。共享编辑标题采用横向 20px、纵向 16px 内边距；页脚采用横向 20px、纵向 12px 内边距。原文的 32px 间距、44px 触控最小目标、18px 设计稿窗口圆角不是当前通用原生 token。

## Elevation & Depth

表面以色调、亮度和细边界建立层次，正文不依赖壁纸。所有原生窗口使用 `WindowBackgroundAppearance::Opaque`；主窗口 macOS 集成标题栏的透明只面向不透明应用 chrome。辅助窗口使用普通原生标题栏。没有真实玻璃或 backdrop blur。

阴影仅在组件库的弹层等场景提供必要层次；Manis 没有自定义通用阴影尺度。弹出表面必须遮住下方文字。布局容器可透明继承不透明父表面，模态遮罩允许混合。

**The Opaque Workspace Rule.** 正文、列表、输入框、编辑器与弹出内容使用不透明主题表面。

原文的 140/220ms 动效、传播式铜色信号和 reduced-motion 切换策略尚未作为统一 GPUI 动效系统实现，不作为当前动效 token。

## Shapes

控件与行使用轻微圆角，面板使用更宽松圆角；规范值见前置 `rounded`。结构分隔使用细边，输入框边界更明确。徽标通过表面与文字表达状态，不叠加多余描边。

共享文本按钮高 34px 或 38px，图标按钮与拖动把手为 32px 方形目标。把手保留透明 2px 边界空间，在键盘焦点时改为蓝色边框，避免聚焦时跳动。六点把手图标来自明确嵌入的 `GripVertical` 资产。

## Components

### Buttons

主要保存或导入动作使用 `ActionRole::Primary` 对应的青绿 `ButtonVariant::Primary`；次操作使用 subdued 冷灰 `Secondary`，删除使用独立 `Danger`。所有共享按钮经 `action_button` / `style_action_button`，水平内边距为 12px，悬停、按下、加载和禁用状态由同步后的组件主题提供。添加 AND 条件属于次操作；编辑页脚保留一个主要提交动作。

### Inputs / Fields

输入表面不透明，标签与说明有明确层级，使用强边界和蓝色焦点。来源、规则和策略组编辑器在窗口建立焦点范围后，将初始焦点置于其首个输入框。无需按页面定义第二套输入调色板。

### Navigation

当前入口为节点、分流规则、网络活动、日志和设置。导航的悬停、选中、键盘焦点分别可辨；设置目录提供六个章节，支持鼠标与 Enter/Space。主窗口生产路径从设置入口打开独立窗口；旧截图 fixture 仍保留内联设置以维持矩阵。

### Cards / Lists / Badges

列表标题与正文通过表面和轻微分隔表达结构。策略组强调当前出口与延迟，来源保留来源信息。状态徽标使用中性低层表面或路由轻色表面，始终写出状态。列表视图表达选择意图；领域数据、内核调用和回滚由应用模型负责。

### Rule priority drag

规则组优先级用六点把手拖动，鼠标呈原生 open-hand；拖动时显示浮动组名，并在目标上半或下半出现青绿前/后插入线。接近列表边缘持续滚动，Escape 取消，聚焦把手后的 Alt+Up/Down 提供键盘替代。上下箭头排序按钮已移除。放下后沿用保存和应用事务，失败恢复此前顺序。

### Editor lifecycle / Dialogs

Cmd/Ctrl+, 打开设置，Cmd/Ctrl+W 关闭辅助窗口，Escape 关闭编辑器；活动的导入、策略保存和设置配置传输期间拒绝对应取消或关闭。取消编辑清除草稿。短确认与配置替换预览留在发起窗口的模态层；配置替换等待编辑器关闭，手动规则草稿打开时阻止主列表删除以保护索引。托盘只把主窗口作为目标。

原文的全局搜索、Cmd/Ctrl+1…6、`/` 策略筛选、通用方向键选择、解释器快捷键、compact 返回键和测速快捷键均是交互预期；当前源码未注册这些全局快捷键。不要把这张早期预期表当成已交付的键盘支持。

### Routing truth

保留 `规则 → 策略组 → 最终节点` 的解释语义。客户端本地规则预测必须写明预测，缺少上下文时需要实际连接；真实已观察链路来自内核支持的 `/connections` 数据。铜色路径或徽标不能把预测包装为已观察。详见 [PRODUCT.md](PRODUCT.md)。

## Do's and Don'ts

### Do:

- **Do** 从 `theme.rs` 和共享组件复用颜色、字号、控件状态。
- **Do** 保持不透明正文、稳定编辑标题与页脚、可滚动长表单。
- **Do** 保持青绿主要动作与冷灰次操作的清晰层级。
- **Do** 为拖动提供插入线、浮动组名、边缘滚动与键盘替代。
- **Do** 用合成数据检查浅色、深色、紧凑窗口与可见键盘焦点。

### Don't:

- **Don't** 用透明玻璃正文或桌面背景承担可读性。
- **Don't** 把铜色路由语义用于普通动作或装饰。
- **Don't** 将本地预测标成内核已观察结果。
- **Don't** 将旧布局、动效和快捷键意向写成已实现事实。
- **Don't** 把离屏 GPUI 内容截图称为真实 OS 标题栏或合成器验证。
