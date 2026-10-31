# Manis for Android (experimental)

Kotlin/Compose 客户端，内置受管的 Mihomo 内核，通过 `VpnService` 接管整机流量。工作区模型和配置渲染
完全复用桌面版的 Rust 实现，手机和桌面生成的 `config.yaml` 语义一致。

## 现在能做什么

| 页面 | 能力                                                                                                             |
| ---- | ---------------------------------------------------------------------------------------------------------------- |
| 连接 | 一键建立/断开 VPN；显示当前出口节点；规则 / 全局 / 直连模式即时切换；配置摘要和解析提示                          |
| 节点 | 读取内核里的策略组（全局组 + 用户组）；展开后按延迟显示节点；点选立即切换；单组测速                              |
| 订阅 | 多订阅增删与启停；VLESS 分享链接导入；策略组（手动选择 / 自动测速）的新建与删除；「应用」后热重载内核            |
| 规则 | Quantumult X 风格的 `HOST` / `HOST-SUFFIX` / `HOST-KEYWORD` 规则编辑，策略可指向 `DIRECT`、`REJECT` 或任意策略组 |
| 设置 | 内核版本、配置摘要、ABI、数据目录；重新载入；查看内核日志；许可证说明                                            |

与桌面版的差异（有意保留）：

- 订阅预览、多个 QX 规则来源、出口 IP 探测、流量曲线尚未移植。
- 系统代理开关不适用；Android 的「始终开启 VPN」由系统设置接管，因此没有登录自启动。
- 策略组的成员选择按订阅/节点勾选，暂不支持按名称正则过滤之外的桌面高级策略配置。

## 工作方式

```text
Compose UI ──workspace.json──▶ Rust (crates/manis-android)
                                  │  MobileConfig → Profile（与桌面同一套类型）
                                  ▼
                            config.yaml (tun.file-descriptor: 3)
                                  │
VpnService ──TUN fd──▶ 打包的 libmihomo.so ──▶ 127.0.0.1:9090（每次启动随机 secret）
```

- UI 把工作区写成一份 JSON（订阅、VLESS 节点、策略组、QX 规则、模式），Rust 侧编译成 `Profile`
  并渲染 YAML，因此规则补全、策略映射、全局兜底规则只有一处实现。
- `VpnService` 持有 TUN 的 `ParcelFileDescriptor`，内核每次启动把同一个 fd 复制到子进程的 fd 3。
  改配置只重启内核，VPN 接口不断开。
- 控制器只监听 `127.0.0.1`，`secret` 每次启动随机生成，仅保存在进程内存中。
- 代理服务器域名的解析方式可在「设置 → 维护 → 节点域名解析」切换：默认使用加密 DNS（DoH，
  和桌面版一致），避免运营商或公共 DNS 对机场域名返回被封地址而让所有节点超时；
  「本机 DNS」改用当前网络的系统解析结果（由 `ConnectivityManager` 传给内核），适合机场用
  分地区 CDN、加密 DNS 返回的地址在你这里连不上的情况。
- 连接后若当前节点探测失败，应用会自动测速并把全局组切到最快可用节点；也可以在节点页手动
  「选最快」。换网络（Wi-Fi ↔ 移动数据）会重新探测。
- 如果整组都不可达，连接页会直接显示「所有节点都连不上」并说明是链路超时，而不是留下一个
  看似已连接、实际没有流量的状态。
- 订阅页的「刷新」让内核立刻重新下载订阅（`PUT /providers/proxies`），只更新节点列表，
  不重启内核、不断开 VPN；修改订阅/节点/规则本身仍然要点「应用」。
- 节点入口是轮换 IP 的机场，只有机场自己的 DoH 才解析出可用地址。可在「设置 → 维护 →
  节点域名解析 → 自定义 DoH」里填机场给的地址；订阅文档里带 `proxy-server-nameserver` 时应用
  会自动读取并优先使用（手动填写的会覆盖自动值）。
- `addDisallowedApplication(packageName)` 让 Manis 自己的请求（订阅拉取、控制器）绕开 TUN，
  避免自环。

## 构建

依赖：JDK 21、Android SDK 35、NDK 27、Rust 目标 `aarch64-linux-android`、`cargo-ndk`、Go 1.27、
Gradle 8.9。

```bash
android/build-apk.sh arm64-v8a     # 产物 android/app/build/outputs/apk/debug/app-debug.apk
android/build-core.sh arm64-v8a    # 只重建内核
```

`android/build-core.sh` 会按 `patches/mihomo-skip-android-package-manager.patch` 编译上游
`v1.19.33`：官方 Android 内核在启动 TUN 时无条件读取 `/data/system/packages.xml`，普通应用无权
访问会直接失败；补丁让包管理器查询失败时回退为空的包名集合。`arm64-v8a`/`armeabi-v7a` 使用
`CGO_ENABLED=0` 内部链接，`x86_64`/`x86` 需要 NDK clang 外部链接。

CI（`.github/workflows/android.yml`）构建 `arm64-v8a` debug APK 并上传 artifact
`manis-android-arm64-debug`。Package workflow（`.github/workflows/package.yml`）也会在每次
发布时构建同一个 APK，命名为 `Manis-<版本>-android-arm64-debug.apk`、附带 `.sha256`，并作为
`latest` 滚动 Pre-release 和 `build-<版本>` Release 的资产上传。设置
`MANIS_PACKAGE_VERSION` 和 `MANIS_ANDROID_VERSION_CODE` 可分别覆盖 versionName 与单调递增的
versionCode；未设置时保持 `0.1.0-android` / `1`。仓库提交了 `android/debug.keystore`（仅调试
用），本地和 CI 产物的签名一致，可以直接覆盖安装。

## 验证状态

模拟器（API 35，x86_64）已通过：

- 首次授权、前台服务、`tun0(fd=3)`、gVisor 栈、fake-IP DNS；
- 订阅拉取失败时明确显示「订阅没有节点」，不再假装已连接；
- 规则渲染为 `DOMAIN-SUFFIX` 并追加 `MATCH,__MANIS_GLOBAL__`，内核日志出现
  `match Match using __MANIS_GLOBAL__`；
- 新增订阅 → 应用 → 配置里出现两个 provider，内核重启但 TUN 不断；
- 模式切换通过 `PATCH /configs` 即时生效；
- 两个 VLESS 节点之间切换，`__MANIS_GLOBAL__` 的 `now` 随之改变。

真机（HarmonyOS / Android 17，arm64）已验证安装、VPN、内核启动；真实订阅链路的节点展示和选路
仍待用户手机复测。

排查「桌面能用、手机所有节点都超时」：先看「设置 → 查看内核日志」（现在可以一键复制），
再对比节点域名在手机上的实际解析结果。若加密 DNS 返回的地址在你这里连不上，切到「本机 DNS」。
