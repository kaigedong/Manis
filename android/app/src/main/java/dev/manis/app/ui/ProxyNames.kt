package dev.manis.app.ui

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
