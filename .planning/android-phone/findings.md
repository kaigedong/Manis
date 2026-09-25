# Android 手机版研究发现

## 现状
- 已接入 GPUI `gpui-mobile` 的 Android ARM64 后端，个人 fork 为 https://github.com/bobo-dong/manis-gpui-mobile。
- 复核方案活跃度：原项目 https://github.com/itsbalamurali/gpui-mobile 当前有 364 stars、107 commits，README 标记 Android arm64/API 26+ 为支持状态，并展示 Motorola Edge 50 Pro 真机截图；示例是 NativeActivity + `gpui_wgpu::WgpuRenderer`，这仍是当前最符合“用户多、活跃、开源”要求的 GPUI Android 方案。其 Android example README 也专门列出黑屏排查（逐帧驱动、避免 frame callback 锁死、直接使用 GPUI Scene renderer）。
- 上游 HEAD 复核：`itsbalamurali/gpui-mobile` 的当前 HEAD 为 `1d3ec2a1d14a63b74d1f4269340441d4eeada27a`，与 Manis fork 基点相同；`bobo-dong/manis-gpui-mobile` 在基点之上只有两项为 Manis GPUI API/feature 对齐的提交，Cargo 固定到 `76126c25223fe05a0c0d13794ac1eb753de1c2c2`。因此 fork 未落后于上游。
- Android APK 可以成功构建，包含 `libmanis_android.so` 与 GPUI Mobile 库；尚未在真机启动。
- Manis Android host 是 NDK NativeActivity；VpnService、JNI FD 桥接及 runtime 重启切换已经接入，未做真机验证。
- Mihomo runtime 在 `crates/manis-ui/src/mihomo/runtime_build.rs` 里定位核心，在 `runtime/platform.rs` 经 `EngineManager` 启停外部进程。
- `packaging/fetch-mihomo.sh` 已加入 Android ARM64；官方 releases 提供 `mihomo-android-arm64-v8-<version>.gz` 且带 SHA-256 digest。
- 非 Android TUN 通过 Mihomo HTTP API 热重载；Android TUN 走专门流程：Builder 创建接口、Rust 接收 FD、Mihomo 通过 command-fds 在 fd 3 上继承，并重启进程。
- 已处理执行策略：Android 官方 target API 29+ 行为变更禁止应用对 app home 下的文件直接 `execve()`；Android 15 AOSP `app_neverallows.te` 只为 targetSdk <=28 豁免该限制。当前自用侧载 APK 因此设为 targetSdk 28，保留 app-private Mihomo 下载/更新。AOSP 证据：https://android.googlesource.com/platform/system/sepolicy/%2B/refs/tags/android-15.0.0_r6/private/app_neverallows.te
- Google 官方 target API 29+ 行为说明：https://developer.android.com/about/versions/10/behavior-changes-10#execute-permission
- 预研 targetSdk35 路径：Android 的 APK native library 路径由系统从签名 APK 安装，SELinux 将其标记为 `apk_data_file`；AOSP app policy 给 appdomain 对该类型可执行映射权限。可尝试将 Mihomo PIE executable 以 `libmihomo.so` 名称打入 `jniLibs/arm64-v8a`，打开 `extractNativeLibs`，通过 `nativeLibraryDir/libmihomo.so` 启动，并随 Manis APK 更新。此路径避免 app-private 动态执行，但需要 APK 打包及真机验证，且禁用应用内内核替换。

## Android 平台约束
- Android 官方流程要求由应用定义 `VpnService`，通过用户授权和 `VpnService.Builder.establish()` 建立 TUN 接口；应用需要读写返回的 FD，并对 VPN 自身建立的外连 socket 调用 `protect` 以避免流量回环。
- API 26+ 启动 VPN 后需要及时转成前台服务。
- 官方文档：https://developer.android.com/reference/android/net/VpnService
- Android 14+ 目标应用的 VPN 前台服务需在 Manifest 声明 `systemExempted` 类型及 `FOREGROUND_SERVICE_SYSTEM_EXEMPTED` 权限；VPN 应用需在系统 VPN 设置里配置/启用后方符合例外条件。参考：https://developer.android.com/about/versions/14/changes/fgs-types-required
- Android `VpnService.Builder.addDisallowedApplication(packageName)` 会让该包像 VPN 未运行时一样直接使用系统网络，且与 addAllowedApplication 不能混用。参考：https://developer.android.com/reference/android/net/VpnService.Builder
- Android 模拟器验收（Android 35 ARM64）：APK 能安装，NativeActivity、JNI、GPUI Platform 初始化和系统字体加载均有 logcat 证据。初次运行发现 GPUI fork 需要的 `dev.gpui.mobile.GpuiPlatformView` 等 Java helpers 没进入应用 APK；现已从 fork 示例目录复制到 `packaging/android/app/src/main/java/dev/gpui/mobile/`，并在 Activity 显式加载动态库以注册 VPN 回调。修复后没有再出现该类缺失错误。
- 模拟器 host Vulkan 路径选中 Apple M4 Vulkan 适配器后立刻报告 `wgpu device lost`，截图为黑屏。以 `-gpu swiftshader_indirect` 重启后，SwiftShader Vulkan 适配器通过表面测试，但 GPUI renderer 初始化持续占用 NativeActivity 主线程，窗口回调尚未完成。这个软件/host GPU 结果不足以判断真实手机；`adb devices` 仅有模拟器，所以仍须接真实 Android 设备验证首屏与 VPN。
- 加入启动阶段日志后确认 `Starting GPUI Application with Android platform` 会出现，但 `GPUI launch callback`、`AndroidWindow::new` 成功后的 `window opened` 都不会出现。阻塞发生在 NativeActivity 收到 `InitWindow` 后、AndroidWindow 构造中的 `WgpuRenderer::new` 阶段；尚不能归因到 Manis 布局或 VPN。近期模拟器 ADB 启动异常也阻断了继续用内置 GPUI sample 做同机对照。
- Mihomo 官方 TUN 文档说明支持 Android 平台的应用包/用户过滤，并记录 Android IPv4 限制；文档示例本身未列出 FD 字段。
- Mihomo issue 和官方内核日志展示 `tun.file-descriptor`、`auto-route: false` 以及由 Android VpnService 提供 FD 的用法。此字段需用锁定的 release binary 验证，不应只凭 wiki schema。
- 若 `VpnService.Builder.addDisallowedApplication(Manis package)` 排除 Manis 自身包，则 Manis/Mihomo 上游 socket 预期不会回流进 VPN，可以避免跨进程 `protect(fd)` callback；仍需实机验证。
- 社区 `oviron/libmihomo-android` 已提供 Mihomo JNI/TUN 样例，但仓库标注 GPL-3.0；需先评估它与 Manis Apache-2.0 分发的组合许可，不应直接加入生产 APK。
- 社区 JNI 方案接口示例：https://github.com/oviron/libmihomo-android
- FlClash 在 `android && cgo` 构建中从 Go API 直接启动 Mihomo sing-tun listener 并提供 protect callback；这证明 JNI 是成熟做法，但复制其全量 bridge 会增加许可、体量和更新维护成本。
- 已实际运行 Manis 的安全 release fetcher，以 Android/aarch64 选择 Mihomo `v1.19.31`：官方 gzip asset 的 SHA-256 校验通过，解包为 `ELF 64-bit LSB pie executable, ARM aarch64`，interpreter 为 `/system/bin/linker64`。开发主机是 macOS，因此仅跳过主机端 `-v` 检查；Android 设备上的版本检查尚未验证。
- `core_update::Platform` 增加 AndroidArm64 后，现有应用内核心更新流程能够选择该资产并使用既有 digest 校验、解包、版本探测、原子替换和失败回滚。
- Android NativeActivity 在初始化 GPUI 前将 `HOME`、`XDG_DATA_HOME`、`XDG_CONFIG_HOME` 设置到 `AndroidApp::internal_data_path()` 下，使 Manis 配置和 Mihomo 内核写入 app-private 沙盒。
- `cargo ndk ... check -p manis-android --lib` 与 APK Gradle 构建通过。交叉编译仍有 Android 未使用桌面代码的告警；当前没有真机运行证据。

## 待验证问题
- 在 Android 真机验证 targetSdk28 侧载 APK 能启动 app-private Mihomo、授权 VPN、传递 TUN fd、处理 DNS/IPv4/IPv6 流量及系统 revoke 恢复。
- 在 Android 真机验证 VPN 授权、TUN fd 继承、DNS/IPv4/IPv6 流量和系统 revoke 后的恢复。
- 验证 `addDisallowedApplication` 排除 Manis 自身 UID 与目标手机厂商网络行为。
- 验证 Mihomo Android binary 的执行及联网权限声明。

## 已采用的 Android 原生控制边界
- `gpui-mobile` 已将 `AndroidApp` 及 Activity JavaVM/jobject 保存为全局句柄，提供 JNI 入口；Manis UI 可通过额外的窄接口 crate 向 Java Service 发命令，不需要把 GPUI private API 暴露出来。
- 推荐 VpnService 通过 `VpnService.prepare()` 用户授权流程、`foregroundServiceType="systemExempted"` 维持长期 VPN；`ParcelFileDescriptor.detachFd()` 交出底层 descriptor，由同进程 JNI/Rust 持有并在 `CommandExt::pre_exec` 中 `dup2` 到固定 fd（例如 3），Mihomo 配置 `tun.file-descriptor: 3`、`auto-route: false`。
- 进程与接口生命周期应按“先请求 Service -> 收到已授权/FD -> 停止普通 core -> 启动带 FD 的核心 -> readiness probe；断开时先停核心，再关闭 FD/撤销 Service”实现。服务收到 `onRevoke()` 时需同一路径通知 app，不能只由 GPUI 按钮管理。

## 当前发行假设
- 用户准备将程序安装到 Android 手机上，当前先按自用侧载构建处理：targetSdk28 保留 app-private Mihomo 安装、摘要校验和应用内核心更新。
- 若后续需要 Google Play，改用当前 targetSdk，并把核心作为签名 APK 原生内容与 Manis 一起更新；目前不把此开发 APK 当成商店发行构建。
