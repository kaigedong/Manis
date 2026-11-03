package dev.manis.app.ui

import android.content.Context
import android.net.Uri
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.manis.app.core.ControllerApi
import dev.manis.app.core.CoreController
import dev.manis.app.core.GroupInfo
import dev.manis.app.core.NodeInfo
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.Workspace
import dev.manis.app.data.subscriptionCachePath
import java.io.File
import kotlinx.coroutines.launch

@Composable
fun ProxiesScreen(onMessage: (String) -> Unit, onNavigate: (Tab) -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snapshot = RuntimeState.snapshot
    val active = RuntimeState.active
    val config = Workspace.config.value
    var expanded by remember { mutableStateOf(emptySet<String>()) }
    var measured by remember { mutableStateOf<Map<String, Int>>(emptyMap()) }
    var testing by remember { mutableStateOf<String?>(null) }

    fun refresh() {
        scope.launch {
            ControllerApi.configure(CoreController.secret)
            runCatching { ControllerApi.snapshot() }.onSuccess { RuntimeState.snapshot = it }
        }
    }

    fun select(group: String, name: String) {
        scope.launch {
            ControllerApi.configure(CoreController.secret)
            runCatching { ControllerApi.select(group, name) }
                .onSuccess {
                    onMessage("已切换到 $name")
                    runCatching { ControllerApi.snapshot() }.onSuccess { RuntimeState.snapshot = it }
                }
                .onFailure { onMessage(it.message ?: "切换失败") }
        }
    }

    fun pickFastest(group: GroupInfo) {
        scope.launch {
            testing = group.name
            ControllerApi.configure(CoreController.secret)
            runCatching { ControllerApi.testGroup(group.name) }
                .onSuccess { delays ->
                    measured = measured + delays
                    val best = delays.minByOrNull { it.value }
                    if (best == null) {
                        onMessage("所有节点都不可用")
                    } else {
                        runCatching { ControllerApi.select(group.name, best.key) }
                            .onSuccess {
                                RuntimeState.unreachable = false
                                onMessage("已选择 ${best.key}（${best.value}ms）")
                                runCatching { ControllerApi.snapshot() }
                                    .onSuccess { RuntimeState.snapshot = it }
                            }
                            .onFailure { onMessage(it.message ?: "切换失败") }
                    }
                }
                .onFailure { onMessage(it.message ?: "测速失败") }
            testing = null
        }
    }

    fun test(group: GroupInfo) {
        scope.launch {
            testing = group.name
            ControllerApi.configure(CoreController.secret)
            runCatching { ControllerApi.testGroup(group.name) }
                .onSuccess {
                    measured = measured + it
                    onMessage("${displayProxyName(group.name) ?: group.name} 测速完成")
                }
                .onFailure { onMessage(it.message ?: "测速失败") }
            testing = null
        }
    }

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        ScreenTitle(
            "节点",
            if (active) {
                "按订阅来源列出节点，点选立即切换"
            } else {
                "未连接时显示已保存的节点，连接后可切换和测速"
            },
        )
        ApplyBar()

        if (!active) {
            OfflineNodes(context, config, onNavigate)
            return@Column
        }

        val groups = snapshot?.groups.orEmpty()
        if (groups.isEmpty()) {
            Text("正在读取节点…", color = MaterialTheme.colorScheme.onSurfaceVariant)
            TextButton(onClick = { refresh() }) { Text("重试") }
            return@Column
        }

        val global = groups.firstOrNull { it.name == GLOBAL_GROUP }
        val policyGroups = groups.filter { group ->
            group.name != GLOBAL_GROUP &&
                group.name != "GLOBAL" &&
                !group.name.startsWith("__MANIS_")
        }
        val knownNodes = snapshot?.nodes.orEmpty()
        val globalNodes = global?.all.orEmpty().toSet()
        val current = global?.now

        // The global selector contains every node. Split it back into the saved sources so a
        // subscription and a manual node do not look like one merged list.
        val offlineSources = remember(config) { buildOfflineSources(context, config) }
        val liveSources = offlineSources.mapNotNull { source ->
            val nodes = source.nodes.filter { it in globalNodes }
            if (nodes.isEmpty()) null else source.copy(nodes = nodes, enabled = true)
        }
        val assigned = liveSources.flatMap { it.nodes }.toSet()
        val groupNames = groups.map { it.name }.toSet()
        val leftovers = globalNodes.filter { name -> name !in assigned && name !in groupNames }

        if (global != null) {
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                "全局",
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.SemiBold,
                            )
                            Text(
                                "所有订阅和手动节点 · ${global.all.size} 个",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        TextButton(
                            onClick = { pickFastest(global) },
                            enabled = testing == null && global.all.isNotEmpty(),
                        ) { Text("选最快") }
                        TextButton(
                            onClick = { test(global) },
                            enabled = testing == null && global.all.isNotEmpty(),
                        ) { Text(if (testing == global.name) "测速中…" else "测速") }
                    }
                    Text(
                        "当前：${displayProxyName(current) ?: "未选择"}",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
            }
        }

        if (liveSources.isEmpty() && leftovers.isEmpty()) {
            Text(
                "内核没有返回节点，检查订阅是否启用，或到订阅页刷新后再试。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            Text("节点来源", style = MaterialTheme.typography.titleMedium)
            for (source in liveSources) {
                LiveSourceCard(
                    source = source,
                    nodes = knownNodes,
                    current = current,
                    measured = measured,
                    onSelect = { name -> select(GLOBAL_GROUP, name) },
                )
            }
            if (leftovers.isNotEmpty()) {
                LiveSourceCard(
                    source = OfflineSource(
                        name = "其他节点",
                        detail = "未归入已保存来源",
                        nodes = leftovers,
                        enabled = true,
                    ),
                    nodes = knownNodes,
                    current = current,
                    measured = measured,
                    onSelect = { name -> select(GLOBAL_GROUP, name) },
                )
            }
        }

        if (policyGroups.isNotEmpty()) {
            Text("策略组", style = MaterialTheme.typography.titleMedium)
        }
        for (group in policyGroups) {
            val open = group.name in expanded
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Row(
                        Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                displayProxyName(group.name) ?: group.name,
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.SemiBold,
                            )
                            Text(
                                "${typeLabel(group.type)} · ${group.all.size} 个节点",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        if (group.type == "Selector") {
                            TextButton(
                                onClick = { pickFastest(group) },
                                enabled = testing == null && group.all.isNotEmpty(),
                            ) {
                                Text("选最快")
                            }
                        }
                        TextButton(
                            onClick = { test(group) },
                            enabled = testing == null && group.all.isNotEmpty(),
                        ) {
                            Text(if (testing == group.name) "测速中…" else "测速")
                        }
                        TextButton(onClick = {
                            expanded = if (open) expanded - group.name else expanded + group.name
                        }) {
                            Text(if (open) "收起" else "展开")
                        }
                    }

                    if (group.now != null) {
                        Text(
                            "当前：${displayProxyName(group.now)}",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.primary,
                        )
                    }

                    if (open) {
                        HorizontalDivider()
                        for (name in group.all) {
                            val delay = measured[name] ?: snapshot?.nodes?.get(name)?.delay
                            Row(
                                Modifier
                                    .fillMaxWidth()
                                    .clickable { select(group.name, name) }
                                    .padding(vertical = 10.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    displayProxyName(name) ?: name,
                                    Modifier.weight(1f),
                                    style = MaterialTheme.typography.bodyMedium,
                                )
                                if (name == group.now) {
                                    Text(
                                        "当前",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.primary,
                                    )
                                }
                                Text(
                                    delayText(delay),
                                    Modifier.padding(start = 12.dp),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                }
            }
        }

        TextButton(onClick = { refresh() }) { Text("刷新") }
    }
}

private data class OfflineSource(
    val name: String,
    val detail: String,
    val nodes: List<String>,
    val enabled: Boolean,
)

/**
 * Shows what is already saved while the core is stopped: subscription cards, cached proxy names
 * and manual nodes. This page used to be empty until a VPN connection existed, which made a
 * configured account look like it had no nodes at all.
 */
@Composable
private fun OfflineNodes(
    context: Context,
    config: MobileConfig,
    onNavigate: (Tab) -> Unit,
) {
    val sources = remember(config) { buildOfflineSources(context, config) }
    val total = sources.sumOf { it.nodes.size }

    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("尚未连接", style = MaterialTheme.typography.titleMedium)
            Text(
                if (total > 0) {
                    "下面是已保存的 $total 个节点，连接后会显示完整策略组、实时延迟并可切换。"
                } else {
                    "下面是已保存的订阅和手动节点；先刷新订阅，连接前也能看到缓存的节点名。"
                },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }

    if (sources.isEmpty()) {
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("还没有订阅或手动节点", style = MaterialTheme.typography.titleMedium)
                Text(
                    "到「订阅」页添加 Clash 订阅或导入 VLESS 链接后，这里会列出节点。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                TextButton(onClick = { onNavigate(Tab.Subscriptions) }) { Text("去订阅页添加") }
            }
        }
        return
    }

    for (source in sources) {
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        source.name,
                        Modifier.weight(1f),
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        if (!source.enabled) "已停用" else "${source.nodes.size} 个节点",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Text(
                    source.detail,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (source.nodes.isEmpty()) {
                    Text(
                        if (!source.enabled) {
                            "已停用；启用后连接或刷新即可加载节点。"
                        } else {
                            "本地还没有节点缓存；到「订阅」页点「刷新」下载后，不连接也能在这里预览。"
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                } else {
                    HorizontalDivider()
                    for (node in source.nodes) {
                        Text(
                            node,
                            Modifier.padding(vertical = 6.dp),
                            style = MaterialTheme.typography.bodyMedium,
                        )
                    }
                }
            }
        }
    }

    TextButton(onClick = { onNavigate(Tab.Subscriptions) }) { Text("管理订阅和节点") }
}

@Composable
private fun LiveSourceCard(
    source: OfflineSource,
    nodes: Map<String, NodeInfo>,
    current: String?,
    measured: Map<String, Int>,
    onSelect: (String) -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    source.name,
                    Modifier.weight(1f),
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    "${source.nodes.size} 个节点",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Text(
                source.detail,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            HorizontalDivider()
            for (name in source.nodes) {
                val delay = measured[name] ?: nodes[name]?.delay
                Row(
                    Modifier
                        .fillMaxWidth()
                        .clickable { onSelect(name) }
                        .padding(vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        displayProxyName(name) ?: name,
                        Modifier.weight(1f),
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    if (name == current) {
                        Text(
                            "当前",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.primary,
                        )
                    }
                    Text(
                        delayText(delay),
                        Modifier.padding(start = 12.dp),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

private fun buildOfflineSources(context: Context, config: MobileConfig): List<OfflineSource> {
    val sources = mutableListOf<OfflineSource>()
    config.subscriptions.forEachIndexed { index, subscription ->
        sources += OfflineSource(
            name = subscription.name.ifBlank { "订阅 ${index + 1}" },
            detail = "订阅 · ${Uri.parse(subscription.url).host ?: "地址已隐藏"}",
            nodes = if (subscription.enabled) {
                cachedProxyNames(context, subscription.url)
            } else {
                emptyList()
            },
            enabled = subscription.enabled,
        )
    }
    if (config.nodes.isNotEmpty()) {
        sources += OfflineSource(
            name = "手动节点",
            detail = "分享链接导入",
            nodes = config.nodes.map { node ->
                if (node.enabled) node.displayName else "${node.displayName}（已停用）"
            },
            enabled = true,
        )
    }
    return sources
}

private fun cachedProxyNames(context: Context, url: String): List<String> {
    val relative = subscriptionCachePath(url).removePrefix("./")
    val file = File(context.filesDir, "mihomo/$relative")
    if (!file.isFile) return emptyList()
    return runCatching { parseProxyNames(file.readText()) }.getOrDefault(emptyList())
}

/**
 * Reads the top-level `proxies:` names from a cached subscription document.
 *
 * This is deliberately a small YAML projection rather than a full parser: it only needs display
 * names for the offline preview, and the core remains the authority for the real configuration.
 */
internal fun parseProxyNames(document: String): List<String> {
    val names = mutableListOf<String>()
    var inProxies = false
    var itemIndent: Int? = null
    var currentName: String? = null

    fun flush() {
        currentName?.let(names::add)
        currentName = null
    }

    for (line in document.lineSequence()) {
        if (line.isBlank()) continue
        val trimmed = line.trimStart()
        if (trimmed.startsWith("#")) continue
        val indent = line.length - trimmed.length
        if (!inProxies) {
            if (indent == 0 && trimmed.startsWith("proxies:")) inProxies = true
            continue
        }
        if (indent == 0) break
        if (trimmed.startsWith("-")) {
            val item = itemIndent
            if (item == null) {
                itemIndent = indent
                flush()
                currentName = parseProxyName(trimmed.removePrefix("-").trimStart())
            } else if (indent == item) {
                flush()
                currentName = parseProxyName(trimmed.removePrefix("-").trimStart())
            }
            // A deeper "-" belongs to a nested list (ws-opts, headers, ...), not another proxy.
            continue
        }
        val item = itemIndent ?: continue
        if (currentName == null && indent > item) {
            currentName = parseProxyName(trimmed)
        }
    }
    flush()
    return names
}

private val NAME_KEY = Regex("""(?:^|[{,]\s*)(?:"name"|name)\s*:\s*""")

private fun parseProxyName(line: String): String? {
    val match = NAME_KEY.find(line) ?: return null
    return parseYamlScalar(
        line.substring(match.range.last + 1),
        inline = line.trimStart().startsWith("{"),
    )
}

private fun parseYamlScalar(raw: String, inline: Boolean): String? {
    val value = raw.trimStart()
    if (value.isEmpty()) return null
    return when (value[0]) {
        '"' -> {
            val text = StringBuilder()
            var escaped = false
            for (index in 1 until value.length) {
                val character = value[index]
                when {
                    escaped -> {
                        text.append(character)
                        escaped = false
                    }

                    character == '\\' -> escaped = true
                    character == '"' -> return text.toString().ifBlank { null }
                    else -> text.append(character)
                }
            }
            text.toString().ifBlank { null }
        }

        '\'' -> {
            val text = StringBuilder()
            var index = 1
            while (index < value.length) {
                val character = value[index]
                if (character == '\'' && index + 1 < value.length && value[index + 1] == '\'') {
                    text.append('\'')
                    index += 2
                } else if (character == '\'') {
                    return text.toString().ifBlank { null }
                } else {
                    text.append(character)
                    index += 1
                }
            }
            text.toString().ifBlank { null }
        }

        else -> {
            val scalar = if (inline) {
                value.substringBefore(',').substringBefore('}')
            } else {
                value
            }
            scalar.substringBefore(" #").trim().ifBlank { null }
        }
    }
}

internal const val GLOBAL_GROUP = "__MANIS_GLOBAL__"

private fun typeLabel(type: String): String = when (type) {
    "Selector" -> "手动选择"
    "URLTest" -> "自动测速"
    "Fallback" -> "故障转移"
    "LoadBalance" -> "负载均衡"
    else -> type
}

internal fun delayText(delay: Int?): String = when {
    delay == null -> "—"
    delay >= 1_000 -> "%.1fs".format(delay / 1_000f)
    else -> "${delay}ms"
}
