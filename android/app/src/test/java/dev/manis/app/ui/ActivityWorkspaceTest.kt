package dev.manis.app.ui

import dev.manis.app.core.ConnectionInfo
import dev.manis.app.data.MobileConfig
import dev.manis.app.data.RuleEntry
import dev.manis.app.data.RuleGroupEntry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ActivityWorkspaceTest {

    @Test
    fun userRouteStagesHidesInternalRuntimeGroups() {
        val stages = userRouteStages(
            listOf(" HK 05 ", "__MANIS_GLOBAL__", "__MANIS_EXIT_IP_PROBE__", "GLOBAL", " "),
        )
        assertEquals(listOf("HK 05"), stages)
    }

    @Test
    fun observedRouteNeverExposesManisGlobalAndCombinesRulePayload() {
        val connection = ConnectionInfo(
            id = "c1",
            host = "www.example.com",
            rule = "DomainSuffix",
            rulePayload = "example.com",
            chains = listOf("HK05", "__MANIS_GLOBAL__", "GLOBAL"),
        )
        val config = MobileConfig(
            ruleEntries = listOf(
                RuleEntry(kind = "DOMAIN-SUFFIX", value = "example.com", policy = "__MANIS_GLOBAL__"),
            ),
        )
        val group = matchingRuleGroup(connection, config)
        val route = observedRoute(connection, group) { node -> if (node == "HK05") 82 else null }

        assertEquals("DomainSuffix example.com", route.rule)
        assertEquals("手动规则", route.group)
        assertEquals("HK05", route.exit)
        assertEquals(82, route.latency)
        assertFalse(route.direct)
        assertFalse(route.rejected)
    }

    @Test
    fun observedRouteMatchesRuleGroupBeforeManualRulesAndFallback() {
        val config = MobileConfig(
            ruleGroups = listOf(
                RuleGroupEntry(id = "rg-work", name = "工作", policy = "日本节点", enabled = true),
                RuleGroupEntry(id = "rg-off", name = "停用组", policy = "DIRECT", enabled = false),
            ),
            ruleEntries = listOf(
                RuleEntry(kind = "DOMAIN-SUFFIX", value = "corp.example", groupId = "rg-work"),
                RuleEntry(kind = "DOMAIN-SUFFIX", value = "off.example", groupId = "rg-off"),
                RuleEntry(kind = "FINAL", value = "", policy = "__MANIS_GLOBAL__"),
            ),
        )

        val workConn = ConnectionInfo(
            id = "c-work",
            host = "git.corp.example",
            rule = "DomainSuffix",
            rulePayload = "corp.example",
            chains = listOf("JP-01", "日本节点", "__MANIS_GLOBAL__", "GLOBAL"),
        )
        assertEquals("工作", matchingRuleGroup(workConn, config))

        val disabledGroupConn = ConnectionInfo(
            id = "c-off",
            host = "off.example",
            rule = "DomainSuffix",
            rulePayload = "off.example",
            chains = listOf("JP-01", "__MANIS_GLOBAL__", "GLOBAL"),
        )
        assertNull(matchingRuleGroup(disabledGroupConn, config))

        val matchConn = ConnectionInfo(
            id = "c-match",
            host = "unknown.example",
            rule = "Match",
            chains = listOf("HK-01", "__MANIS_GLOBAL__", "GLOBAL"),
        )
        val matchGroup = matchingRuleGroup(matchConn, config)
        assertEquals("默认兜底", matchGroup)
        val matchRoute = observedRoute(matchConn, matchGroup) { null }
        assertEquals("Match", matchRoute.rule)
        assertEquals("默认兜底", matchRoute.group)
        assertEquals("HK-01", matchRoute.exit)
    }

    @Test
    fun observedRouteFallsBackToUserPolicyGroupOrOmitsGroupForDirect() {
        val grouped = ConnectionInfo(
            id = "c2",
            host = "api.openai.com",
            rule = "RuleSet",
            rulePayload = "ai",
            chains = listOf("SG-02", "AI 自动选择", "__MANIS_GLOBAL__", "GLOBAL"),
        )
        val groupedRoute = observedRoute(grouped, null) { null }
        assertEquals("RuleSet ai", groupedRoute.rule)
        assertEquals("AI 自动选择", groupedRoute.group)
        assertEquals("SG-02", groupedRoute.exit)

        val direct = ConnectionInfo(
            id = "c3",
            host = "www.baidu.com",
            rule = "GeoIP",
            rulePayload = "CN",
            chains = listOf("DIRECT"),
        )
        val directRoute = observedRoute(direct, null) { 10 }
        assertEquals("GeoIP CN", directRoute.rule)
        assertNull(directRoute.group)
        assertEquals("DIRECT", directRoute.exit)
        assertNull(directRoute.latency)
        assertTrue(directRoute.direct)

        val missingRoute = ConnectionInfo(
            id = "c4",
            host = "1.1.1.1",
            chains = emptyList(),
        )
        val empty = observedRoute(missingRoute, null) { null }
        assertEquals("未匹配规则", empty.rule)
        assertNull(empty.group)
        assertNull(empty.exit)
    }

    @Test
    fun connectionTargetPrefersSniffHostAndFormatsPortAndIpv6() {
        val sniffed = ConnectionInfo(
            id = "t1",
            host = "",
            sniffHost = "chat.openai.com",
            destinationIp = "104.18.32.47",
            destinationPort = "443",
        )
        assertEquals("chat.openai.com:443", connectionTarget(sniffed))

        val ipv6 = ConnectionInfo(
            id = "t2",
            host = "",
            destinationIp = "2001:db8::1",
            destinationPort = "443",
        )
        assertEquals("[2001:db8::1]:443", connectionTarget(ipv6))

        val portOnly = ConnectionInfo(
            id = "t3",
            host = "",
            destinationPort = "53",
        )
        assertEquals("未知目标 · 端口 53", connectionTarget(portOnly))
    }

    @Test
    fun activitySearchMatchesVisibleFieldsAndIgnoresInternalGroups() {
        val connection = ConnectionInfo(
            id = "s1",
            host = "",
            sniffHost = "www.example.com",
            destinationPort = "443",
            process = "Chrome",
            rule = "DomainSuffix",
            rulePayload = "example.com",
            chains = listOf("Hong Kong", "__MANIS_GLOBAL__", "GLOBAL"),
        )
        val route = observedRoute(connection, "手动规则") { null }

        assertTrue(matchesActivityQuery(connection, route, "EXAMPLE"))
        assertTrue(matchesActivityQuery(connection, route, "chrome"))
        assertTrue(matchesActivityQuery(connection, route, "domainsuffix"))
        assertTrue(matchesActivityQuery(connection, route, "hong kong"))
        assertTrue(matchesActivityQuery(connection, route, "手动规则"))
        assertFalse(matchesActivityQuery(connection, route, "__manis_global__"))
        assertFalse(matchesActivityQuery(connection, route, "GLOBAL"))
    }

    @Test
    fun ruleDraftPrefersSniffHostAndPrefillsObservedPolicy() {
        val policies = listOf("DIRECT", "REJECT", "__MANIS_GLOBAL__", "AI 自动选择")
        val sniffed = ConnectionInfo(
            id = "d1",
            host = "155.133.231.9",
            sniffHost = "cache11-lax2.steamcontent.com",
            destinationIp = "155.133.231.9",
            chains = listOf("SG-02", "AI 自动选择", "__MANIS_GLOBAL__"),
        )
        val draft = ruleDraft(sniffed, policies)
        assertNotNull(draft)
        assertEquals("DOMAIN-SUFFIX", draft!!.entry.kind)
        assertEquals("cache11-lax2.steamcontent.com", draft.entry.value)
        assertEquals("AI 自动选择", draft.entry.policy)

        val ipv4 = ConnectionInfo(
            id = "d2",
            host = "",
            destinationIp = "203.0.113.7",
            chains = listOf("DIRECT"),
        )
        val ipDraft = ruleDraft(ipv4, policies)
        assertNotNull(ipDraft)
        assertEquals("IP-CIDR", ipDraft!!.entry.kind)
        assertEquals("203.0.113.7/32", ipDraft.entry.value)
        assertEquals("DIRECT", ipDraft.entry.policy)
    }
}
