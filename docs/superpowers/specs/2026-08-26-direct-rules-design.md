# 直连规则（绕过代理）设计

## 问题

TUN 模式接管全部流量后，SSH 走代理出口经常连不通，`git push` 到 `git@github.com:...` 会失败。当前的绕法是手动关代理、推完再开回来。

## 目标

在分流规则页提供一个用户可编辑的直连规则列表，让指定端口和域名后缀的流量绕过代理直连，不必再手动开关代理。

## 非目标

- 不处理 HTTPS 协议的 git 远端（本来就正常）。
- 不做进程名匹配（`PROCESS-NAME`）。端口足以覆盖当前场景。
- 不做 IP 段匹配（`IP-CIDR`）。

## 数据模型

`manis-profile` 新增一个 `Rule` 变体：

```rust
Rule::DstPort { port: u16, policy: PolicyRef }
```

域名后缀复用现有的 `Rule::DomainSuffix`，两个内核都已有渲染分支，无需新增。

UI 层用户条目表示为：

```rust
enum DirectRule {
    Port(u16),
    DomainSuffix(String),
}
```

编译进 profile 时全部映射为 `policy: PolicyRef::Direct`。

## 内核渲染

| 条目 | Mihomo | sing-box |
|---|---|---|
| `Port(22)` | `DST-PORT,22,DIRECT` | `{ "port": [22], "action": "route", "outbound": "direct" }` |
| `DomainSuffix("github.com")` | `DOMAIN-SUFFIX,github.com,DIRECT` | `{ "domain_suffix": ["github.com"], "action": "route", "outbound": "direct" }` |

## 规则顺序

直连规则插在 `rules` 最前，优先于一切：

```
[直连规则…] → GEOIP,CN,DIRECT → [QX 订阅规则…] → MATCH,Proxy
```

## 生效范围

只在路由模式为「规则」时生效。这不需要额外代码：Mihomo 和 sing-box 的 `mode: global` 本来就跳过 `rules`，直连模式下流量本就不走代理。「全局」保持字面意义上的全局。

## 持久化

私有文件，权限 `0600`，原子写入，跟随现有 QX 规则源的存储模式。首次创建时预置一条 `22`；用户删除后不会自动恢复。

## 界面

分流规则页顶部新增「直连规则」卡片：

- 列出当前条目，每条可删除。
- 一个输入框加条目，复用现有单行输入组件。
- 输入按形状判别：纯数字视为端口，其余视为域名后缀。
- 校验：端口范围 `1..=65535`；域名非空、无空白、无协议前缀和路径。
- 重复条目不重复添加。

## 测试

- profile 层：`DstPort` 的两内核渲染；直连规则确实排在 `GEOIP` 之前；`validate()` 仍要求以唯一的 `MATCH` 结尾。
- 解析层：端口与域名的判别、端口越界（0 和 65536）、空输入、带协议前缀的输入被拒。
- 持久化层：写入权限 `0600`、往返读取、首次预置 22、删除后不恢复。
- UI 层：增删条目、持久化生效。
- 实机：用真实 Mihomo `check` 验证生成的配置可用。

## 已知边界

- `DST-PORT,22` 会让**所有** SSH 直连，包括到境外 VPS 的连接。这与手动关代理时的行为一致，是预期结果。
- 若 GitHub 的 SSH 改走 443 端口（`ssh.github.com:443`），端口 22 那条不再匹配，需要靠域名后缀条目兜底。
