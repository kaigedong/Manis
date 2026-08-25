# Relay

Relay 是一个使用 Rust 与 GPUI 构建的跨平台代理策略工作台。它借鉴 Quantumult X 易于理解的“规则 → 策略组 → 节点”工作流，但不复制其界面，也不要求用户先学会编辑 YAML。

![Relay 原生宽屏界面](.impeccable/review/native-wide.png)

## 当前里程碑

- 原生 GPUI 窗口，可在宽、中、窄三档尺寸间自适应
- 策略组、节点与规则的可操作演示状态
- 可解释的本地路由预测链，并明确区别于 Mihomo 已观察连接
- 浅色/深色主题、系统代理演示开关与键盘可聚焦控件
- macOS 原生运行和 Metal 离屏截图已验证
- Windows、Linux 已配置对应的 GPUI 平台依赖，仍需各自原生 CI/设备验证

当前数据全部是演示数据，尚未连接 Mihomo，也不会修改系统代理设置。

## 运行

仓库通过 `rust-toolchain.toml` 固定 Rust 工具链。macOS 需要 Xcode Command Line Tools；Linux 还需要 Wayland/X11 的系统开发库。

```bash
cargo run -p relay-ui
```

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

## 代码结构

- `crates/relay-core`：与渲染框架无关的窗口尺寸、策略选择和路由证据状态
- `crates/relay-ui`：GPUI 应用、主题、演示数据与自适应视图
- `DESIGN.md`：视觉 token、组件和响应式行为
- `GPUI_IMPLEMENTATION.md`：状态边界、事件流和未来 Mihomo API 映射
- `packaging`：各平台打包元数据的起点

GPUI 与 `gpui_platform` 固定到同一个 Zed 提交，避免跟随 `main` 漂移。后续优先补齐 Mihomo 只读连接、配置导入与跨平台 CI，再开放真实写入和系统代理控制。
