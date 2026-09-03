# 统一手动分流规则与旧直连规则迁移

## 问题

旧版在分流规则页单独维护“直连规则”。其中 `22 → DIRECT` 会让所有 SSH 连接直连，无法表达“只让 GitHub 的 22 端口直连”，并且与后来加入的手动分流规则重复。

## 目标

- 所有用户规则统一进入“手动规则”组。
- 一条规则可以包含多个必须同时命中的条件。
- 策略可以是 `DIRECT`、`REJECT` 或用户策略组，不再把端口规则固定为直连。
- 支持域名或 IP 与目标端口组合。
- 自动迁移旧 `direct-rules.state`，不丢失端口 22 等既有行为。

## 数据模型

`ManualRule` 保存一个非空条件列表和一个目标策略。单条件继续编译为普通 `Rule`；多条件编译为：

```rust
Rule::All {
    conditions: Vec<RuleCondition>,
    policy: PolicyRef,
}
```

当前界面允许配置两个 AND 条件，存储格式最多接受四个，为后续扩展保留空间。

## 内核渲染

例如“GitHub 域名后缀 + 目标端口 22 → DIRECT”：

```text
Mihomo: AND,((DOMAIN-SUFFIX,github.com),(DST-PORT,22)),DIRECT
```

IP 单地址使用 `IP-CIDR`：IPv4 为 `/32`，IPv6 为 `/128`。

## 规则顺序

手动规则整体位于规则订阅之前，内部保持用户看到的顺序：

```text
[手动规则…] → [规则订阅…] → GEOIP,CN,DIRECT → MATCH,__MANIS_GLOBAL__
```

## 持久化与迁移

- 新格式为 `manis.manual-routing-rules.v2`。
- 文件记录旧直连规则已经迁移，避免用户删除迁移项后又被重新导入。
- `Port(22)` 迁移为 `DST-PORT 22 → DIRECT`。
- `DomainSuffix(github.com)` 迁移为 `DOMAIN-SUFFIX github.com → DIRECT`。
- 旧文件只保留只读解析能力，不再参与运行时编译，也不再有独立 UI。
- 文件继续使用私有权限和原子替换。

## 界面

- 页面通过“添加规则”打开一个专用弹窗，不在规则列表中内嵌编辑器。
- 第一条件始终存在；“添加‘并且’条件”显示第二行。
- 每行独立选择类型并填写参数。
- 命中后的策略在条件之后选择，以强调“条件 → 策略”的关系。
- 生效规则列表把组合条件显示在同一行，中间明确标记 `AND`。

## 测试

- Mihomo 精确渲染域名/IP + 端口组合。
- 单条件和多条件的参数校验、重复条件与端口边界。
- v1 手动规则和旧直连规则迁移到 v2。
- 删除迁移后的端口规则不会在下次启动时恢复。
- 宽屏与紧凑布局视觉快照。

## 边界

- 单独的 `DST-PORT 22 → DIRECT` 仍表示所有 SSH 直连；若只想放行 GitHub，应组合 `DOMAIN-SUFFIX github.com`。
- `ssh` 配置中的本地别名不会成为网络目的域名，应匹配其实际 `HostName`。
