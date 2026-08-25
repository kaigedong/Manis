# GPUI 策略代理客户端 UI 设计

## 目标
建立一套现代、统一、可在 Windows/macOS/Linux 复用的桌面设计系统，并制作可交互的响应式原型。视觉以 Fluent 2 的桌面效率为基础，吸收 Material 3 的自适应布局与克制的 Expressive 动效，最终可直接翻译为 GPUI tokens 和组件。

## 阶段
- [complete] 1. 安装并核验设计技能，建立产品设计上下文
- [complete] 2. 生成并收敛设计系统、信息架构和窗口尺寸规则
- [complete] 3. 制作策略组核心页面的交互原型
- [complete] 4. 宽/中/窄窗口与明暗主题视觉验证
- [complete] 5. 整理 GPUI 实现规范与设计系统文档

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
