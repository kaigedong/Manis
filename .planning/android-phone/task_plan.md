# Android 手机版实施计划

## 目标
让 Manis 能在 Android ARM64 手机上启动，使用 GPUI Android 后端，并通过 Android VPN Service 将系统流量交给 Mihomo；最终用真机核验连接、断开和基础 UI/输入。

## 当前阶段
阶段 5：APK 已成功构建并在 Android 35 ARM64 模拟器安装；模拟器窗口仍黑屏，等待真实 Android GPU 设备诊断首屏与 VPN 生命周期。

## 阶段

### 阶段 1：GPUI Android 宿主与 APK
- [x] 选择活跃的开源 GPUI Android 移植并创建个人 fork
- [x] 接入 Manis Android NativeActivity 宿主与 ARM64 Gradle 工程
- [x] 对当前 GPUI API 适配并完成 Android 交叉编译与 APK 构建
- [x] 记录尚未实现的 VPN、真机和输入法限制
- **状态：** complete（尚无真机运行证据）

### 阶段 2：Android VPN 与 Mihomo 技术路径
- [x] 确认可维护的 Android Mihomo 运行方式及许可证边界
- [x] 确认 VpnService TUN FD 到 Mihomo 的桥接机制和 protect(socket) 路径
- [x] 设计并实现 Rust 核心、Android Service 与 UI 的生命周期/API
- **状态：** complete

### 阶段 3：Mihomo Android 运行时
- [x] 支持 Android arm64 Mihomo 官方稳定版下载、摘要校验、版本验证和私有目录安装
- [x] 让 Manis runtime 能配置、启动、监测和停止 Android 核心
- [ ] 保持控制器认证、数据目录权限和更新来源校验
- **状态：** complete（交叉编译和 APK 通过；Android 真机仍需验收）

### 阶段 4：VpnService 与连接控制
- [x] 加入正确声明的 VpnService 和前台通知生命周期
- [x] 在用户授权后建立 TUN、排除 Manis 自身 UID，并以 fd 3 传给 Mihomo
- [x] 将连接/断开和系统撤销回调接入 Manis runtime
- [x] 确保启动失败会关闭服务；系统撤销时重启 Mihomo 回到本地代理模式
- **状态：** complete（设备侧生命周期仍需验收）

### 阶段 5：手机交互与真机验收
- [ ] 调整窄屏布局与触摸交互
- [ ] 核验软键盘及中文输入
- [ ] 构建签名调试 APK 并在 ARM64 真机安装
- [ ] 观察授权、联网、选节点、断开及撤销后的行为
- **状态：** pending

## 已知限制与错误
| 项目 | 状态/处理 |
|------|-----------|
| 当前设备列表没有 Android 真机 | 可继续实现与打包；最终运行验证需设备连接 |
| Android 10+ 对 targetSdk >=29 禁止执行 app-private 下载核心 | 当前开发 APK 按自用侧载设为 targetSdk 28，保留摘要校验和应用内更新；Google Play 构建必须改为当前 targetSdk 并把核心随 APK 打包 |
| Manis runtime 当前由 EngineManager 启动外部进程 | TUN FD 继承与 Android Service 生命周期尚无接口 |
| Android 35 ARM64 模拟器显示黑屏 | Activity/JNI 能启动；host Vulkan 会 device lost，SwiftShader Vulkan 会在 GPUI renderer 初始化阶段长时间占满 CPU；尚未证明真实手机也受影响，需用手机 GPU 调试 |

## 遇到的错误
| 错误 | 次数 | 处理 |
|------|------|------|
| shallow clone Mihomo upstream failed with HTTP/2 framing error | 1 | 不重复相同 clone；使用上游 GitHub issue、官方 TUN docs 和官方 release artifact evidence 核对所需协议 |
| Gradle 9.4.1 与 Android Gradle Plugin 8.7.3 组合导致增量 APK 有 76 MB 未引用数据 | 1 | Android 官方兼容表要求 Gradle 8.9；wrapper 固定为 8.9，build 脚本 clean 后 assemble |
