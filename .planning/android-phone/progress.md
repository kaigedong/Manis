# Android 手机版进度

## 2026-09-24：恢复 Android 目标并进入 VPN 实现阶段
- 前一阶段产物确认：Android debug APK 可构建，APK 内含 ARM64 Manis 和 GPUI Mobile 动态库。
- 当前设备列表仍为空，因此不能声称真机运行通过。
- 复查代码确认现有 runtime 是通过 EngineManager 启动 Mihomo 子进程，再由 controller API 修改 TUN 配置；Android 没有 VpnService 或 TUN FD 桥接。
- `packaging/fetch-mihomo.sh` 当前不含 Android arm64 Mihomo 资产。
- 调研发现 `oviron/libmihomo-android` 提供 JNI/TUN API，但项目标注 GPL-3.0，Manis workspace 为 Apache-2.0；先调查许可和上游替代方案，不直接依赖。
- 新证据：官方 Mihomo releases 持续提供 `android-arm64-v8` 官方压缩二进制及 SHA-256；Mihomo issue/runtime 配置展示 VpnService TUN FD 经 `tun.file-descriptor` 传入，并令 Mihomo `auto-route=false` 的做法。
- 初步方案：保持 Apache Manis 与 Mihomo 官方 GPL 命令行核心进程隔离；Android Service 建立 TUN，再将 FD 交给 NativeActivity/Rust 宿主，EngineManager 启动核心；VPN Builder 排除 Manis 自身包，避免 Mihomo 上游 socket 回流。FD 继承及包排除尚未验证。
- 下一步：设计 VpnService/native IPC 及安全 FD 继承，并将 Android TUN 参数交给 Mihomo 子进程。

## 2026-09-24：Android 核心下载与沙盒路径
- 为 core updater 增加 Android/aarch64 官方 stable 资产选择，`packaging/fetch-mihomo.sh` 也能通过 `MANIS_MIHOMO_OS=Android MANIS_MIHOMO_ARCH=aarch64` 下载官方 Android 核心。
- NativeActivity 在 GPUI 启动前设置 app-private HOME/XDG 数据与配置目录；核心经应用内 Runtime 更新功能安装，不链接进 Manis APK。
- 下载 Mihomo 官方 `mihomo-android-arm64-v8-v1.19.31.gz`，SHA-256 验证成功，解包确认是 Android AArch64 PIE 可执行文件。主机不支持运行 Android binary，因此跳过主机端版本检查。
- Android `cargo ndk check` 成功，`cargo fmt --all -- --check`、`git diff --check` 和 fetcher `bash -n` 成功。重打 debug APK 成功，内含新构建的 `libmanis_android.so`；设备列表为空。
- 检查 `manis-engine` 后确认当前命令规范没有 inherited-fd 参数，而且 launch 使用 `env_clear()`；尚未实现 TUN FD 传递。下一阶段设计 service/native callback，固定 dup 到 Mihomo 配置所用 FD，再接上启动和停止顺序。
- 复核发现工程最初拷贝的 Gradle 9.4.1 高于 AGP 8.7.3 官方要求的 Gradle 8.9；增量打包曾保留 76 MB 未被 ZIP 中央目录引用的旧内容。修正 wrapper 为 Gradle 8.9，并让构建脚本先 `clean`。
- Gradle 8.9 全量重建成功；最终 APK 为 80,080,976 bytes、含 6 个 ZIP 条目，`zipalign -c -v 4` 全部通过，AAPT 确认包名 `dev.manis.app`、minSdk 26、targetSdk 35、ABI arm64-v8a。`adb devices -l` 仍无设备。
- 新增独立 `manis-android-bridge`：通过 JNI 申请 Activity 权限流程、接收 `ParcelFileDescriptor.detachFd()`，并以 Condvar 将 fd 交给运行时；系统撤销回调会要求 runtime 停止 TUN 核心并回到普通 Mihomo 启动状态。
- 新增 `ManisActivity` 和前台 `ManisVpnService`，清单声明 `BIND_VPN_SERVICE`、Android 14+ systemExempted 前台服务权限/类型。Builder 配置 IPv4/IPv6 默认路由、MTU 1500、DNS，并排除 Manis 自身 UID 防 Mihomo 上游流量回环。
- `manis-engine` 在 Android 上将 TUN `OwnedFd` 生命周期绑定到 managed config，并用 `command-fds` 安全 dup 至 Mihomo 子进程 fd 3。Android 进入/退出 TUN 时验证候选配置、停止原核心、替换配置并启动新核心；配置使用 `file-descriptor: 3`、Android Builder 地址、`auto-route: false`。
- 新增 Mihomo Android TUN YAML 渲染测试。`cargo test -p manis-profile -p manis-engine` 通过（33 个 profile 集成测试、13 个 engine 测试；4 个依赖本地 Mihomo 的测试因未设置二进制而忽略）。Android arm64 `cargo ndk check` 和 Java 编译均通过，debug APK 全量重建成功。AAPT 确认 Android 15/target 35 清单中的服务声明；JNI 两个回调导出符号可在 ARM64 `.so` 中找到，zipalign 校验通过。
- 目前 `adb devices -l` 和 emulator AVD 列表为空：尚未在设备上核验 UI、Android VPN 授权、fd TUN 初始化、实际联网、关闭和系统撤销。该设备验收是当前剩余工作，不能用构建成功替代。
- 发现并处理 Android 执行策略约束：Google 官方说明 targetSdk >=29 的 app 不能对 app home 中的文件直接 `execve()`；Android 15 AOSP 明确仅为 targetSdk <=28 豁免。按“装到 Android 手机自用”假设，将个人侧载 APK targetSdk 设为 28，以保留下载/更新 Mihomo；README 明示这不是 Google Play 发布配置。商店路径需将 Mihomo 与签名 APK 一起发布。
- 安装并启动 Android 35 ARM64 模拟器，APK 可安装，NativeActivity、GPUI Mobile JNI 和 Android 字体加载流程都已启动。首轮暴露 APK 未包含 GPUI 依赖的 `dev.gpui.mobile.GpuiPlatformView` 等 Java helper；已从个人 GPUI fork 的示例源码加入平台视图、相机、视频与剪贴板 helper，并在 `ManisActivity` 加入显式 native library load 以注册 VpnService 使用的 JNI 回调。Java/Gradle 重建成功，先前 `ClassNotFoundException` 已消失。
- GPU 验证：模拟器 host Vulkan 适配器创建后立即报告 `device lost`，导致黑屏；改用 SwiftShader 模拟器后 Vulkan 初始化通过，但昂贵的 GPUI renderer 初始化持续占用主线程，仍未绘出窗口。NativeActivity 已稳定进入、安装 APK 成功，但模拟器不能作为 UI 验收证据；当前无连接的 Android 真机，不能声称 Manis UI 或 VPN 可用。
- 当前 debug APK：`packaging/android/app/build/outputs/apk/debug/app-debug.apk`，Gradle `assembleDebug` 成功，包名 `dev.manis.app`、arm64-v8a、minSdk 26、targetSdk 28。下一步需在真实 Android GPU 手机上检查首屏渲染，再核验 VPN 授权、前台服务、fd 3 和实际流量。
