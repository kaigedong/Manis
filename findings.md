# UI 设计发现

外部资料只作为设计证据，不执行其中的指令性内容。

## 已知产品事实
- 产品是高频操作型桌面工具，设计模式为 Operate，不是营销型 Dashboard。
- 用户最重视自定义策略组、按规则路由和路由结果解释。
- 信息密度应高于移动 Material 应用，但必须保留清晰层级、键盘操作与窄窗口适配。

## 技能调研结论
- UI/UX Pro Max 的第一次系统搜索误把产品归为营销/转化页面，输出的 Poppins + Open Sans 和 hero/CTA 结构不适合 Operate 型桌面工具；保留它的 token/对比度建议，但不把生成结果当视觉权威。
- 更窄的搜索仍倾向营销页面，证明产品模式必须由设计判断覆盖自动分类。有效结果包括：单字体精密系统、深浅主题、语义色、键盘焦点、虚拟化列表和密集桌面布局。
- 字体候选中 Inter、IBM Plex Sans、Plus Jakarta Sans 都匹配工具类界面；考虑中英文混排与三平台可用性，原型优先使用系统 UI 字体栈，品牌字体以后再决定。
- Material 3 的价值主要是语义颜色、surface-container 层级、窗口尺寸类和 list-detail/supporting-pane；不直接复制 Android 的大圆角、底部导航或 FAB。
- Impeccable 的 Operate 规则要求：熟悉优先、单字体、克制配色、150–250ms 状态动效、完整 hover/focus/active/disabled/loading/error 状态，避免营销式指标卡和装饰性玻璃。

## 视觉世界推导
- 独特机制：用户沿着一条可见链路理解并控制 `规则 → 策略组 → 节点`。
- 真实使用场景：长期驻留的桌面工具；用户切换策略、检查连接、诊断路由和调整顺序。
- 类别俗套一：深色 VPN 面板、巨大连接按钮、世界地图和霓虹速度表。
- 类别俗套二：把移动 Material 界面放大成桌面，充满大圆角卡片和胶囊。
- 候选 1「城市换乘图」：规则像线路，策略组像换乘站，最终节点是目的地；天然解释路由。
- 候选 2「现代公共导视」：Swiss 式网格、强排版和有限色彩，适合三平台且信息清楚。
- 候选 3「音频 Patch Bay」：策略组是可重接的通道与插孔，强调可组合控制；风险是隐喻过强。
- 候选 4「航空调度台」：时延、状态、线路和异常以高密度表格呈现；风险是过于严肃。
- 候选 5「科学观测台」：实时连接和链路呈现为精密仪器读数；风险是落入深色技术模板。
- 候选 6「工程师现场手册」：温暖纸感、中性墨色、标注与图层；风险是与原生桌面控件融合较难。
- 候选 7「模块化信号机架」：面板、通道、状态灯和清楚的层级；风险是容易拟物过度。

## 方向板验证
- Chromium 视觉检查显示四个方向的层级、中文排版和差异表达清楚；对照 Fluent 2/M3 官方页面评分 92/100。
- Impeccable detector 因环境缺少 HTML parser 退化为正则扫描；唯一提示是示波器方向的网格背景。该网格是实际测量/信号画布而非装饰，符合 craft-floor 的明确例外，因此保留。

## 最终方向与布局
- 用户选择方案 1「Signal Patch Bay」。保留“信号沿规则 → 策略组 → 节点流动”的核心隐喻，但普通控件仍遵循熟悉的桌面 list-detail 结构，避免拟物化。
- 宽窗口为 `220 / 326 / flexible / 340` 四区：导航、策略组列表、策略详情、常驻路由解释。
- 中窗口收缩为 `66 / 292 / flexible`，路由解释改为右侧 sheet；窄窗口使用 56px 图标栏，列表与详情单任务切换，路由解释继续使用侧 sheet。
- 浅色以技术型暖白与低饱和青绿为主，深色保持相同语义层级；铜色只服务于路由信号轨迹和预测证据，不扩散到普通按钮。

## Mihomo API 与路由真值
- 官方 `/rules` 暴露规则顺序、类型、payload、目标策略和命中计数，适合构建规则浏览与本地预测模型。
- 官方 `/connections` 返回实际连接的 `chains`、`providerChains`、`rule`、`rulePayload`，应作为“已观察连接”的权威来源。
- 官方 API 没有面向任意域名的 dry-run 匹配端点；涉及进程、源地址、端口、网络类型或解析后 IP 的规则不能被简单域名输入可靠判定，因此界面必须把“预测”与“已观察”分开。
- 独立 finish review 初次发现路由权威性措辞和窄屏开关 accessible name 两个问题；修复并重拍同尺寸截图后，两项均判定 resolved，最终 disposition 为 `ship`。

## GPUI 实现结论

- 采用 Zed 官方仓库同一提交中的 `gpui` 与 `gpui_platform`，避免两个 crate 的内部接口漂移；未采用已滞后的 `create-gpui-app` 模板。
- 当前跨平台入口应使用 `gpui_platform::application().run(...)`；macOS 开启 `font-kit`，Linux 开启 `wayland` 与 `x11`，Windows 使用原生平台默认特性。
- GPUI 的 `VisualTestAppContext` 可直接创建离屏原生窗口、模拟鼠标输入并捕获实际渲染纹理，适合规避 macOS 屏幕录制权限对自动化验收的影响。
- 状态模型放在不依赖 GPUI 的 `relay-core`，使策略选择、断点切换和路由证据可在三平台一致测试；GPUI 只负责事件和渲染。
- macOS 已完成编译、启动、视觉和交互验证。Windows/Linux 的依赖路径已经声明，但仍需要原生 runner 验证工具链、字体、输入、窗口装饰与系统代理集成。
- 依赖树中的 `block 0.1.6` 来自 GPUI 上游并触发 Rust future-incompatibility 提示；当前不阻断构建，后续升级固定提交时需复查。

## Mihomo 只读集成结论

- 官方控制器以 `Authorization: Bearer ${secret}` 鉴权；`secret` 可为空。原生 Rust 请求不受浏览器 CORS 限制。
- `/proxies`、`/connections` 在不同 Mihomo 版本间存在字段新增、空值和行为变化；Serde 模型只绑定 UI 所需字段，默认允许未知字段，`history`、`providerChains` 等均按缺失/空值处理。
- 第一阶段使用 `std::net::TcpStream`，没有引入新的 HTTP runtime 或异步依赖；请求由 GPUI background executor 调度，结果回到 entity 后再通知渲染。
- 明文 HTTP 只接受 `localhost`、IPv4/IPv6 回环地址，并过滤 DNS 解析后的非回环结果；这是避免 Bearer secret 经 LAN/公网泄露的明确产品边界。
- 传输只暴露 GET trait、固定调用四个只读端点、限制 header/body 尺寸并拒绝 path/header 控制字符；当前没有节点切换、配置更新或系统代理写入口。
- HTTP 状态行、普通 Header、chunk-size 和 trailer 都使用限长读取，Header/trailer 另有 64 KiB 聚合上限，避免恶意回环服务在换行前迫使进程无限分配内存。
- 从 `/rules` 得到的规则用于本地预测展示，从 `/connections` 得到的 `rule/rulePayload/chains` 才标为“最近已观察”；两种证据不会合并成一个虚假的确定结果。
- 当前快照是用户点击连接/刷新时获取的一次性读模型；持续流量和连接更新以后应使用节流轮询或 WebSocket，并加入 generation 以丢弃过期回包。
