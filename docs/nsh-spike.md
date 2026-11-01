# NSH Android spike — 节点页

这是一个**实验分支**，不进入 `main`。目标不是交付新节点页，而是回答三个问题：

1. NSH 能不能嵌进现有 Manis Android 壳（保留 Kotlin `VpnService`）？
2. 节点页这种“搜索 + 分组 + 长列表 + 点选”的 UI，在 NSH 上是否可用、性能如何？
3. Rust root 能不能嵌进现有 Activity，而不是只能用 TSX？

## 结论

- **TSX 嵌入可用**。`NshView` + `nsh.aar` 能放进现有 Activity，参数、刷新、点选都能通过 host channel 走通。
- **Rust root 也能嵌**，但不在官方文档里：`NshView.runRustApp("nsh")` 存在；把 AAR 里的 `libnsh.so` 换成 Rust app 编出来的 `libnsh.so` 后，Kotlin Activity 能直接跑 Rust NSH 页面。**这是手工替换 APK 内 native lib 的未支持路径**，需要单独的打包步骤。
- **不建议现在全量迁移**。NSH 0.3.0 很新，Android 嵌入的 Rust 路径没有正式文档；APK 体积 +25 MB；列表回收有反直觉的响应式陷阱；最低 API 28。

## 环境

- NSH `0.3.0 (76fa8a7)`
- Android emulator API 35，x86_64；`manis` AVD
- Manis `0.1.107-nsh`（spike 本地版本）
- AAR：`~/.nsh/embed/android/nsh.aar`（37 MB，含 arm64-v8a / armeabi-v7a / x86_64 的 `libnsh.so`）

## 做了什么

### TSX 节点页

`android/nsh-ui/App.tsx`：

- 从 `host.params` 接收 `sources`，从真实 `Workspace` 的订阅缓存和手动节点生成
- `List` + `estimatedItemSize`，500 节点压力源
- 搜索框（`Input`）、来源分组、当前节点、延迟
- 「刷新」通过 `host.call("nodes.refresh")` 回到 Kotlin 取最新数据
- 点节点通过 `host.call("nodes.select")` 回到 Kotlin（spike 只记录日志）

嵌入入口：`android/app/src/main/java/dev/manis/app/ui/NshRootPage.kt`（单根 NSH 页面；spike 阶段曾从 `NshNodePage.kt` 开始）。
Settings → 诊断里加了实验入口（仅 spike 分支）。

### Rust root 验证

> Rust root 验证只在 `spike/nsh-node-page` 分支保留；生产迁移分支使用 TSX 嵌入。原始实验步骤：

`android/nsh-rust` 是 `nsh init --template rust` 的最小计数器：

1. `nsh package android/nsh-rust --target android` 生成独立 APK
2. 取出 `lib/x86_64/libnsh.so`
3. 替换 host APK 内的同名库，重新 `zipalign` + `apksigner`
4. Kotlin Activity 调 `nsh.runRustApp("nsh")`

结果：Rust 页面正常显示、按钮可用。说明“Kotlin 外壳 + Rust NSH UI”技术上可行，但需要自定义打包，不能直接依赖官方 `nsh.aar` 的默认内容。

## 测量

| 项目 | 结果 |
|---|---|
| TSX 冷启动（`am start -W`） | 238–298 ms |
| Rust 冷启动（最小页面） | 70 ms |
| 500 行列表滚动 | 185 帧，janky 4（2.16%）；90th 19 ms，99th 34 ms |
| PSS：Compose 节点页基线 | 72 MB |
| PSS：TSX 节点页 | 84 MB |
| PSS：Rust 最小页 | 85 MB |
| arm64 APK（含 Manis + Mihomo + NSH） | 59 MB |
| 对照：0.1.106 arm64 发布 APK | 33.6 MB |
| 增量 | 约 +25 MB |
| `libnsh.so` arm64 | 32.3 MB（AAR，未 strip）；手工 strip 后约 24 MB |
| 最低 API | 28（当前 Manis 是 26） |

> APK 体积是最大的产品代价。按 ABI 分包、strip `libnsh.so`、只保留需要的架构可以缓解，但无法消除。

## 踩到的坑

1. **列表回收不能拷贝行值**
   第一版在行函数顶部 `const node = row().node`，滚动后出现乱序/串行（004、012、013…）。
   NSH 的 `List` 按 cell 回收，必须每次在 JSX 里读 `row()`，并用 `<If>` 在 header/node 两种结构之间切换。修好后顺序正确。

2. **`Input.onChange` 文档与类型不一致**
   文档示例是 `onChange={({ value }) => ...}`，实际类型是 `(text: string) => void`；按文档写会把 `undefined` 写进 signal，触发 `cannot read property 'trim' of undefined`。

3. **`runRustApp` 没有文档**
   AAR 的 `NshView` 有 `runRustApp(String)`，但 Android 文档只写了 `loadAssetBundle` / `loadBundleFile` / `runPackage`。Rust root 嵌入需要自己解决 `libnsh.so` 同名冲突。

4. **无障碍需要显式补**
   `onPress` 的 View 默认可被辅助技术激活，但节点行需要 `accessible` + `accessibilityLabel` 才能让 TalkBack 一次读出“节点名、当前、延迟”；否则只读到散落的文本。

5. **最低 API 28**
   `nsh.aar` 要求 API 28；spike 把 `minSdk` 从 26 改成 28。正式采用要么抬到 28，要么保留 Compose fallback。

## 判断

NSH 的方向是对的，但当前不适合“一次性全量迁移”：

- TSX 嵌入是官方路径，但 UI 要重写；Rust 嵌入是可用但未支持的路径，需要打包工程改造。
- 桌面 GPUI 页面也不能直接复用；NSH 的 GPUI backend 仍需重写页面。
- 0.3.0 在 spike 期间就暴露了文档/类型不一致和未文档化 API，正式产品需要锁版本并准备兜底。

## 建议的下一步

1. 先做共享 presentation 层（`NodeSourceView`、`PolicyGroupView`、显示名映射），不管最后用 Compose 还是 NSH，都先消除“两端语义分叉”。
2. 如果继续 NSH，优先做 **Rust NSH 节点页**而不是 TSX：它和桌面 Rust/GPUI 更接近，启动也更快；但必须先解决 `libnsh.so` 的正式打包方式（NSH 上游支持或自定义 Gradle 任务）。
3. 在解决打包前，不要把 NSH 放进 `main` 或发布通道。

## 复现

```sh
# TSX bundle
nsh build android/nsh-ui/App.tsx --release --out android/app/src/main/assets/nsh

# AAR（不提交）
cp ~/.nsh/embed/android/nsh.aar android/app/libs/nsh.aar

# host
cd android
ANDROID_HOME=$HOME/Android/Sdk ./gradlew -PmanisAbi=x86_64 \
  -PmanisVersionName=0.1.107-nsh -PmanisVersionCode=107 assembleDebug
```

安装后打开「节点」Tab。
