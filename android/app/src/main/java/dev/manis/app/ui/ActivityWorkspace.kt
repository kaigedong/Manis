package dev.manis.app.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.manis.app.core.ConnectionInfo
import dev.manis.app.core.ConnectionsSnapshot
import dev.manis.app.core.ControllerApi
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.RuleEntry
import dev.manis.app.data.Workspace
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** The route segments one live connection proves, matching the desktop Activity workspace. */
internal data class ObservedRoute(
    val rule: String,
    val group: String?,
    val exit: String?,
    val latency: Int?,
    val direct: Boolean,
    val rejected: Boolean,
)

private data class ActivityConnectionView(
    val connection: ConnectionInfo,
    val route: ObservedRoute,
)

/**
 * The activity workspace: the live connections of the running kernel, each one drawn as its
 * observed route — the rule that matched, the rule/policy group that took it (hiding internal
 * runtime groups), and the exit it reached. Tapping a card opens the rule sheet prefilled with
 * the sniffed host (or destination IP) and prepends the saved rule so it matches before FINAL.
 */
@Composable
fun ActivityWorkspace(onMessage: (String) -> Unit) {
    val colors = LocalManisColors.current
    var snapshot by remember { mutableStateOf<ConnectionsSnapshot?>(null) }
    var loadError by remember { mutableStateOf<String?>(null) }
    var refreshing by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }
    var draft by remember { mutableStateOf<RuleEditor?>(null) }
    val config = Workspace.config.value
    val runtimeSnapshot = RuntimeState.snapshot
    val policies = remember(config.groups) { policyOptions(config) }
    val scope = rememberCoroutineScope()

    LaunchedEffect(Unit) {
        while (true) {
            if (RuntimeState.active) {
                runCatching { ControllerApi.connections() }
                    .onSuccess {
                        snapshot = it
                        loadError = null
                    }
                    .onFailure { error ->
                        loadError = error.message ?: "读取连接失败"
                    }
            } else {
                snapshot = null
                loadError = null
            }
            delay(2_000)
        }
    }

    val allConnections = snapshot?.connections ?: emptyList()
    val visibleViews = remember(allConnections, query, config.ruleGroups, config.ruleEntries, runtimeSnapshot) {
        val newestFirst = allConnections.sortedByDescending { it.start }
        newestFirst.mapNotNull { connection ->
            val ruleGroup = matchingRuleGroup(connection, config)
            val route = observedRoute(
                connection = connection,
                ruleGroup = ruleGroup,
                nodeDelay = { exit -> runtimeSnapshot?.nodes?.get(exit)?.delay },
            )
            if (matchesActivityQuery(connection, route, query)) {
                ActivityConnectionView(connection, route)
            } else {
                null
            }
        }
    }

    val state = draft
    if (state != null) {
        RuleEditorPane(
            initial = state.entry,
            editingIndex = null,
            existing = config.ruleEntries,
            policies = policies,
            groups = config.ruleGroups,
            onCancel = { draft = null },
            onDelete = { draft = null },
            onConfirm = { entry ->
                // Prepend so rules created from live activity match before any FINAL fallback,
                // mirroring desktop Activity behavior.
                Workspace.update { it.copy(ruleEntries = listOf(entry) + it.ruleEntries) }
                draft = null
                onMessage("已添加规则，点「应用」后生效")
            },
        )
    }

    Column(Modifier.fillMaxSize().background(colors.surfaceBase)) {
        ManisPageHeader(title = "网络活动")
        Column(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = ManisDimens.spaceLg),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Row(
                Modifier.fillMaxWidth().height(ManisDimens.controlStandard),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
            ) {
                Box(Modifier.weight(1f)) {
                    ManisSearchField(
                        value = query,
                        onValueChange = { query = it },
                        placeholder = "搜索域名、规则、分组或出口",
                    )
                }
                ManisSectionAction(if (refreshing) "刷新中…" else "刷新") {
                    if (!RuntimeState.active) {
                        onMessage("先连接 VPN")
                    } else {
                        refreshing = true
                        scope.launch {
                            runCatching { ControllerApi.connections() }
                                .onSuccess {
                                    snapshot = it
                                    loadError = null
                                    onMessage("已刷新连接")
                                }
                                .onFailure { error ->
                                    loadError = error.message ?: "读取连接失败"
                                    onMessage("读取连接失败")
                                }
                            refreshing = false
                        }
                    }
                }
            }
            // One compact mono line: how many connections, and what they have moved.
            Text(
                summaryLine(
                    shown = visibleViews.size,
                    total = allConnections.size,
                    download = visibleViews.sumOf { it.connection.download },
                    upload = visibleViews.sumOf { it.connection.upload },
                    pending = snapshot == null && loadError == null && RuntimeState.active,
                    idle = !RuntimeState.active && query.isBlank(),
                ),
                style = ManisType.data.copy(fontSize = 11.5.sp),
                color = colors.textTertiary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(
                start = ManisDimens.spaceLg,
                end = ManisDimens.spaceLg,
                top = ManisDimens.spaceSm,
                bottom = ManisDimens.spaceXl,
            ),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            if (!RuntimeState.active) {
                item(key = "inactive") {
                    ManisCard {
                        Text("暂无活动连接", style = ManisType.label, color = colors.textPrimary)
                        ManisMeta("连接 VPN 后，这里会显示每条连接的域名、规则与流量。")
                    }
                }
            } else if (loadError != null && allConnections.isEmpty()) {
                item(key = "load-error") {
                    ManisCard {
                        Text("读取连接失败", style = ManisType.label, color = colors.statusError)
                        ManisMeta(loadError.orEmpty())
                    }
                }
            } else if (allConnections.isEmpty()) {
                item(key = "empty") {
                    ManisCard {
                        ManisMeta("还没有连接；有流量经过内核时会出现在这里。")
                    }
                }
            } else if (visibleViews.isEmpty()) {
                item(key = "no-match") {
                    ManisCard {
                        Text("没有匹配的连接", style = ManisType.label, color = colors.textPrimary)
                        ManisMeta("试试搜索目标域名、IP、规则名称、规则组或出口节点。")
                    }
                }
            } else {
                itemsIndexed(
                    items = visibleViews.take(200),
                    key = { index, view ->
                        view.connection.id.ifBlank { "conn-$index" }
                    },
                ) { _, view ->
                    ConnectionCard(
                        view = view,
                        onClick = { draft = ruleDraft(view.connection, policies) },
                    )
                }
            }
        }
    }
}

/**
 * Strips internal runtime selectors (`__MANIS_GLOBAL__`, `__MANIS_EXIT_IP_PROBE__`, `GLOBAL`)
 * from Mihomo's `chains`, matching desktop `user_route_stages`.
 */
internal fun userRouteStages(chains: List<String>): List<String> =
    chains
        .map(String::trim)
        .filter { it.isNotEmpty() && !isInternalProxyGroup(it) }

/**
 * Recovers the rule group that matched this connection in the exact compilation order used by
 * `crates/manis-android/src/model.rs`: enabled rule groups first, then manual rules, then default
 * fallback for `Match`.
 */
internal fun matchingRuleGroup(connection: ConnectionInfo, config: MobileConfig): String? {
    val kind = connection.rule.trim()
    if (kind.isEmpty()) return null
    val normalizedKind = normalizeRuleKind(kind)
    val payload = connection.rulePayload.trim()

    val enabledGroups = config.ruleGroups.filter { it.enabled && it.name.isNotBlank() && it.id.isNotBlank() }
    val disabledGroupIds = config.ruleGroups.filterNot { it.enabled }.map { it.id.trim() }.toSet()
    val enabledGroupIds = enabledGroups.map { it.id.trim() }.toSet()

    for (group in enabledGroups) {
        val matched = config.ruleEntries.any { entry ->
            entry.enabled &&
                entry.groupId?.trim() == group.id.trim() &&
                !isFinalRuleKind(entry.kind) &&
                ruleEntryMatchesConnection(entry, normalizedKind, payload)
        }
        if (matched) return group.name.trim()
    }

    val matchedManual = config.ruleEntries.any { entry ->
        if (!entry.enabled || isFinalRuleKind(entry.kind)) return@any false
        val ownerId = entry.groupId?.trim().orEmpty()
        val isManual = ownerId.isEmpty() || (ownerId !in enabledGroupIds && ownerId !in disabledGroupIds)
        isManual && ruleEntryMatchesConnection(entry, normalizedKind, payload)
    }
    if (matchedManual) return "手动规则"

    if (kind.equals("Match", ignoreCase = true)) return "默认兜底"
    return null
}

private fun normalizeRuleKind(value: String): String =
    value.filter(Char::isLetterOrDigit).lowercase()

private fun isFinalRuleKind(kind: String): Boolean {
    val trimmed = kind.trim()
    return trimmed.equals("FINAL", ignoreCase = true) || trimmed.equals("MATCH", ignoreCase = true)
}

private fun ruleEntryMatchesConnection(
    entry: RuleEntry,
    normalizedConnectionKind: String,
    connectionPayload: String,
): Boolean {
    if (normalizeRuleKind(entry.kind) != normalizedConnectionKind) return false
    val expected = entry.value.trim()
    if (expected.isEmpty() || connectionPayload.isEmpty()) return false
    return expected.equals(connectionPayload, ignoreCase = true)
}

/**
 * Formats the displayed target (`host:port`, `[ipv6]:port`, or fallback), mirroring desktop
 * `connection_target`.
 */
internal fun connectionTarget(connection: ConnectionInfo): String {
    val host = connection.targetHost
    val port = connection.destinationPort.trim()
    return when {
        host.isNotEmpty() && port.isNotEmpty() && host.contains(':') && !host.startsWith('[') ->
            "[$host]:$port"
        host.isNotEmpty() && port.isNotEmpty() -> "$host:$port"
        host.isNotEmpty() -> host
        port.isNotEmpty() -> "未知目标 · 端口 $port"
        else -> "未知目标"
    }
}

/** Extracts the process badge label if Mihomo reported a process name or path. */
internal fun connectionProcess(connection: ConnectionInfo): String? {
    val process = connection.process.trim()
    if (process.isNotEmpty()) return process
    val path = connection.processPath.trim()
    if (path.isNotEmpty()) return path.substringAfterLast('/').ifEmpty { path }
    return null
}

/**
 * Searches user-visible connection fields, rule payload, matched group, and user route stages,
 * never matching internal runtime group names like `__MANIS_GLOBAL__`.
 */
internal fun matchesActivityQuery(
    connection: ConnectionInfo,
    route: ObservedRoute,
    query: String,
): Boolean {
    val needle = query.trim()
    if (needle.isEmpty()) return true
    val fields = buildList {
        add(connection.sniffHost)
        add(connection.host)
        add(connection.destinationIp)
        add(connection.remoteDestination)
        add(connection.destinationPort)
        add(connection.process)
        add(connection.processPath)
        add(connection.network)
        add(connection.rule)
        add(connection.rulePayload)
        add(route.rule)
        if (!route.group.isNullOrBlank()) add(route.group)
        addAll(userRouteStages(connection.chains))
        if (route.direct) add("直连")
    }
    return fields.any { it.contains(needle, ignoreCase = true) }
}

/**
 * The rule a tap on a connection creates, mirroring desktop `rule_draft_from_connection`:
 * prefer sniffed host, then host, then destination IP; use `DOMAIN-SUFFIX` for hostnames and
 * single-address CIDR for IPs, and prefill the observed policy when selectable.
 */
internal fun ruleDraft(connection: ConnectionInfo, policies: List<String>): RuleEditor? {
    val preferredHost = listOf(connection.sniffHost, connection.host)
        .map(String::trim)
        .firstOrNull { it.isNotEmpty() }
    val address = connection.destinationIp.trim()
    val policy = preferredPolicyForConnection(connection, policies)

    val entry = when {
        preferredHost != null -> ipOrSuffixEntry(preferredHost, policy)
        address.isNotEmpty() && (isIpv4(address) || isIpv6(address)) -> ipOrSuffixEntry(address, policy)
        else -> null
    }
    return entry?.let { RuleEditor(null, it) }
}

private fun preferredPolicyForConnection(connection: ConnectionInfo, policies: List<String>): String {
    for (stage in connection.chains) {
        val trimmed = stage.trim()
        if (trimmed in policies) return trimmed
    }
    return if ("DIRECT" in policies) "DIRECT" else policies.firstOrNull() ?: "DIRECT"
}

private fun ipOrSuffixEntry(value: String, policy: String): RuleEntry = when {
    isIpv6(value) -> RuleEntry(kind = "IP6-CIDR", value = "$value/128", policy = policy)
    isIpv4(value) -> RuleEntry(kind = "IP-CIDR", value = "$value/32", policy = policy)
    else -> RuleEntry(kind = "DOMAIN-SUFFIX", value = value, policy = policy)
}

private val IPV4_PATTERN = Regex("^\\d{1,3}(\\.\\d{1,3}){3}$")

private fun isIpv4(value: String): Boolean = IPV4_PATTERN.matches(value)

private fun isIpv6(value: String): Boolean = value.count { it == ':' } >= 2

/** Rule, optional group, exit, and tone read from what the core reported for this connection. */
internal fun observedRoute(
    connection: ConnectionInfo,
    ruleGroup: String?,
    nodeDelay: (String) -> Int?,
): ObservedRoute {
    val ruleKind = connection.rule.trim()
    val rulePayload = connection.rulePayload.trim()
    val rule = when {
        ruleKind.isNotEmpty() && rulePayload.isNotEmpty() -> "$ruleKind $rulePayload"
        ruleKind.isNotEmpty() -> ruleKind
        else -> "未匹配规则"
    }
    val stages = userRouteStages(connection.chains)
    val exit = stages.firstOrNull()
    val group = ruleGroup?.trim()?.takeIf { it.isNotEmpty() } ?: stages.getOrNull(1)
    val direct = exit == "DIRECT"
    val rejected = exit == "REJECT"
    val latency = if (exit == null || direct || rejected) null else nodeDelay(exit)
    return ObservedRoute(
        rule = rule,
        group = group,
        exit = exit,
        latency = latency,
        direct = direct,
        rejected = rejected,
    )
}

/** One live connection: what asked, where it went, and the chain that proves it. */
@Composable
private fun ConnectionCard(view: ActivityConnectionView, onClick: () -> Unit) {
    val colors = LocalManisColors.current
    val connection = view.connection
    val route = view.route
    val process = connectionProcess(connection)
    Surface(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(ManisDimens.radiusCard),
        color = colors.surfaceElevated,
        border = BorderStroke(1.dp, colors.outlineSubtle),
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = ManisDimens.spaceLg, vertical = ManisDimens.spaceMd),
            verticalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
        ) {
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ManisDimens.spaceSm),
            ) {
                Text(
                    connectionTarget(connection),
                    modifier = Modifier.weight(1f),
                    style = ManisType.data,
                    color = colors.textPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (process != null) {
                    ManisPill(process, ManisTone.Neutral)
                }
                ManisPill(connection.network.ifBlank { "tcp" }.uppercase(), ManisTone.Neutral)
                Text(
                    "↓ ${formatBytes(connection.download)} · ↑ ${formatBytes(connection.upload)}",
                    style = ManisType.data.copy(fontSize = 11.5.sp),
                    color = colors.textSecondary,
                    maxLines = 1,
                )
            }
            ManisRouteChain(
                rule = route.rule,
                group = route.group,
                exit = route.exit,
                latency = route.latency,
                direct = route.direct,
                rejected = route.rejected,
            )
        }
    }
}

/** The one-line live summary: count, download, upload, and whether it is auto-refreshing. */
private fun summaryLine(
    shown: Int,
    total: Int,
    download: Long,
    upload: Long,
    pending: Boolean,
    idle: Boolean,
): String = when {
    idle -> "0 个连接 · ↓ 0 B · ↑ 0 B"
    pending -> "正在读取连接…"
    shown == total -> "$total 个连接 · ↓ ${formatBytes(download)} · ↑ ${formatBytes(upload)}"
    else -> "$shown/$total 个连接 · ↓ ${formatBytes(download)} · ↑ ${formatBytes(upload)}"
}

private fun formatBytes(bytes: Long): String = when {
    bytes >= 1024L * 1024L * 1024L -> "%.1f GB".format(bytes / (1024.0 * 1024.0 * 1024.0))
    bytes >= 1024L * 1024L -> "%.1f MB".format(bytes / (1024.0 * 1024.0))
    bytes >= 1024L -> "%.1f KB".format(bytes / 1024.0)
    else -> "$bytes B"
}
