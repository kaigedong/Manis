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
