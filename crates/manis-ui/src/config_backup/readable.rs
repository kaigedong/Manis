use std::str::FromStr;

use toml_edit::{DocumentMut, Item, value};

use super::BackupError;

const READABLE_FORMAT: &str = "manis-readable-v2";
const LEGACY_READABLE_FORMAT: &str = "manis-readable-v1";

pub(super) fn to_readable_source(source: &str) -> Result<String, BackupError> {
    let mut document = DocumentMut::from_str(source).map_err(|_| BackupError::InvalidFormat)?;
    let table = document
        .get_mut("configuration")
        .and_then(Item::as_table_mut)
        .ok_or(BackupError::InvalidFormat)?;

    for (name, item) in table.iter_mut() {
        let Some(contents) = item.as_value().and_then(|item| item.as_str()) else {
            return Err(BackupError::InvalidFormat);
        };
        let readable = readable_entry(name.get(), contents);
        let decor = item.as_value().map(|item| item.decor().clone());
        let mut replacement = value(readable);
        if let (Some(decor), Some(value)) = (decor, replacement.as_value_mut()) {
            *value.decor_mut() = decor;
        }
        *item = replacement;
    }

    document.insert("format", value(READABLE_FORMAT));
    Ok(document.to_string())
}

pub(super) fn to_storage_source(source: &str) -> Result<String, BackupError> {
    to_storage_source_with_download(source, |url| {
        crate::rule_source::download_qx_rule_document(url)
            .map_err(|_| BackupError::RemoteSourceUnavailable)
    })
}

fn to_storage_source_with_download(
    source: &str,
    download: impl Fn(&str) -> Result<String, BackupError>,
) -> Result<String, BackupError> {
    let mut document = DocumentMut::from_str(source).map_err(|_| BackupError::InvalidFormat)?;
    let format = document
        .get("format")
        .and_then(Item::as_value)
        .and_then(|item| item.as_str());
    match format {
        Some(READABLE_FORMAT | LEGACY_READABLE_FORMAT) => {}
        Some(_) => return Err(BackupError::InvalidFormat),
        None if document.get("format").is_some() => return Err(BackupError::InvalidFormat),
        None => return Ok(source.to_owned()),
    }

    let table = document
        .get_mut("configuration")
        .and_then(Item::as_table_mut)
        .ok_or(BackupError::InvalidFormat)?;
    for (name, item) in table.iter_mut() {
        let Some(contents) = item.as_value().and_then(|item| item.as_str()) else {
            return Err(BackupError::InvalidFormat);
        };
        let storage = storage_entry(name.get(), contents, &download)?;
        let decor = item.as_value().map(|item| item.decor().clone());
        let mut replacement = value(storage);
        if let (Some(decor), Some(value)) = (decor, replacement.as_value_mut()) {
            *value.decor_mut() = decor;
        }
        *item = replacement;
    }
    document.remove("format");
    Ok(document.to_string())
}

fn readable_entry(name: &str, contents: &str) -> String {
    let heading = entry_heading(name);
    let had_trailing_newline = contents.ends_with('\n');
    let lines: Vec<_> = contents.lines().collect();
    if lines.len() == 1 && !lines[0].contains('\t') {
        let mut readable = format!("# {heading}\nvalue: {}", lines[0]);
        if had_trailing_newline {
            readable.push('\n');
        }
        return readable;
    }

    let has_version = lines.first().is_some_and(|line| is_storage_version(line));
    let mut readable = format!("# {heading}\n");
    if !has_version {
        for line in lines {
            readable.push_str("workspace: ");
            readable.push_str(line);
            readable.push('\n');
        }
        let mut readable = readable.trim_end_matches('\n').to_owned();
        if had_trailing_newline {
            readable.push('\n');
        }
        return readable;
    }

    readable.push_str("format: ");
    readable.push_str(lines[0]);
    for line in &lines[1..] {
        let fields: Vec<_> = line.split('\t').collect();
        let Some(key) = fields.first().copied() else {
            continue;
        };
        match key {
            "member" | "global" | "policy" if fields.len() == 3 => {
                readable.push('\n');
                readable.push_str(key);
                readable.push_str(": ");
                readable.push_str(&decode_field(fields[1]));
                readable.push_str(" -> ");
                readable.push_str(&decode_field(fields[2]));
            }
            "rule" | "final" if fields.len() >= 2 => {
                readable.push('\n');
                readable.push_str(key);
                readable.push_str(": ");
                readable.push_str(&fields[1..].join(" | "));
            }
            "content" if fields.len() == 2 && is_qx_rule_source(name) => {}
            "content" if fields.len() == 2 => {
                readable.push('\n');
                readable.push_str("content:");
                for content_line in decode_field(fields[1]).split('\n') {
                    readable.push('\n');
                    readable.push_str("  | ");
                    readable.push_str(content_line);
                }
            }
            _ if name == "routing-rule-group-order.state" && fields.len() == 1 => {
                readable.push('\n');
                readable.push_str("order: ");
                readable.push_str(key);
            }
            _ => {
                readable.push('\n');
                readable.push_str(key);
                readable.push(':');
                if fields.len() > 1 {
                    readable.push(' ');
                    readable.push_str(
                        &fields[1..]
                            .iter()
                            .map(|field| decode_field_for_key(key, field))
                            .collect::<Vec<_>>()
                            .join("\t"),
                    );
                }
            }
        }
    }
    if had_trailing_newline {
        readable.push('\n');
    }
    readable
}

fn storage_entry(
    name: &str,
    contents: &str,
    download: &impl Fn(&str) -> Result<String, BackupError>,
) -> Result<String, BackupError> {
    let had_trailing_newline = contents.ends_with('\n');
    let lines: Vec<_> = contents.lines().collect();
    let mut storage = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim_end();
        index += 1;
        if line.is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("value:") {
            let mut result = value.trim_start().to_owned();
            if had_trailing_newline {
                result.push('\n');
            }
            return Ok(result);
        }
        if let Some(value) = line.strip_prefix("format:") {
            storage.push(value.trim_start().to_owned());
            continue;
        }
        if let Some(value) = line.strip_prefix("workspace:") {
            storage.push(value.trim_start().to_owned());
            continue;
        }
        if let Some(value) = line.strip_prefix("order:") {
            storage.push(value.trim_start().to_owned());
            continue;
        }
        if let Some(value) = line.strip_prefix("content:") {
            let mut content = Vec::new();
            while index < lines.len() {
                let continuation = lines[index];
                let Some(value) = continuation
                    .strip_prefix("  | ")
                    .or_else(|| (continuation == "  |").then_some(""))
                else {
                    break;
                };
                content.push(value);
                index += 1;
            }
            if !value.trim().is_empty() && content.is_empty() {
                content.push(value.trim_start());
            }
            storage.push(format!("content\t{}", hex::encode(content.join("\n"))));
            continue;
        }

        let Some((key, value)) = line.split_once(':') else {
            return Err(BackupError::InvalidFormat);
        };
        let value = value.trim_start();
        match key {
            "member" | "global" | "policy" => {
                let Some((left, right)) = value.split_once(" -> ") else {
                    return Err(BackupError::InvalidFormat);
                };
                storage.push(format!(
                    "{key}\t{}\t{}",
                    hex::encode(left),
                    hex::encode(right)
                ));
            }
            "rule" | "final" => {
                let fields: Vec<_> = value.split(" | ").collect();
                if fields.is_empty() {
                    return Err(BackupError::InvalidFormat);
                }
                storage.push(format!("{key}\t{}", fields.join("\t")));
            }
            _ => {
                storage.push(format!(
                    "{key}\t{}",
                    if needs_hex_encoding(key) {
                        hex::encode(value)
                    } else {
                        value.to_owned()
                    }
                ));
            }
        }
    }

    if storage.is_empty() {
        return Err(BackupError::InvalidFormat);
    }
    fetch_missing_rule_content(name, &mut storage, download)?;
    let mut result = storage.join("\n");
    if had_trailing_newline {
        result.push('\n');
    }
    Ok(result)
}

fn fetch_missing_rule_content(
    name: &str,
    storage: &mut Vec<String>,
    download: &impl Fn(&str) -> Result<String, BackupError>,
) -> Result<(), BackupError> {
    if !is_qx_rule_source(name) || storage.iter().any(|line| line.starts_with("content\t")) {
        return Ok(());
    }
    let url = storage
        .iter()
        .find_map(|line| line.strip_prefix("url\t"))
        .and_then(|encoded| hex::decode(encoded).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .ok_or(BackupError::InvalidFormat)?;
    let content = download(&url)?;
    let index = storage
        .iter()
        .position(|line| line.starts_with("enabled\t"))
        .unwrap_or(storage.len());
    storage.insert(index, format!("content\t{}", hex::encode(content)));
    Ok(())
}

fn entry_heading(name: &str) -> &'static str {
    match name {
        "subscription.url" => "订阅来源",
        "node-selection.state" => "节点选择",
        "manual-routing-rules.state" => "手动路由规则",
        "routing-rule-group-order.state" => "规则来源顺序",
        "routing.mode" => "路由模式",
        "language.preference" => "语言偏好",
        "workspace.state" => "工作区状态",
        name if has_extension(name, "policy") || has_extension(name, "group") => "策略组",
        name if has_extension(name, "qxrules") => "规则来源",
        name if has_extension(name, "vless") => "保存的节点",
        _ => "配置项",
    }
}

fn has_extension(name: &str, extension: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, current)| current == extension)
}

fn is_qx_rule_source(name: &str) -> bool {
    has_extension(name, "qxrules")
}

fn is_storage_version(value: &str) -> bool {
    value.starts_with("manis-")
        || value.starts_with("manis.")
        || value.starts_with("relay-")
        || value.starts_with("relay.")
}

fn needs_hex_encoding(key: &str) -> bool {
    matches!(key, "name" | "filter" | "url" | "target" | "proxy-dns")
}

fn decode_field_for_key(key: &str, value: &str) -> String {
    if needs_hex_encoding(key) {
        decode_field(value)
    } else {
        value.to_owned()
    }
}

fn decode_field(value: &str) -> String {
    if value.is_empty() || !value.len().is_multiple_of(2) {
        return value.to_owned();
    }
    let Ok(bytes) = hex::decode(value) else {
        return value.to_owned();
    };
    let Ok(decoded) = String::from_utf8(bytes) else {
        return value.to_owned();
    };
    if decoded
        .chars()
        .all(|character| !character.is_control() || matches!(character, '\t' | '\n' | '\r'))
    {
        decoded
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_source_decodes_common_entries_and_round_trips() {
        let source = r#"schema_version = 1
[configuration]
"subscription.url" = """
manis-subscription-source-v3
id	subscription:legacy
name	e9a699e6b8af
enabled	true
url	68747470733a2f2f6578616d706c652e696e76616c6964
refresh	1h
last-success	123
proxy-dns	68747470733a2f2f3139322e302e322e31
"""
"policy-abc123.policy" = """
manis-policy-group-v1
id	policy-abc123
name	5553
icon	none
strategy	manual
interval	60
tolerance-ms	150
matcher	explicit
filter__empty
member	737562736372697074696f6e3a6c6567616379	484b203033
"""
"routing.mode" = "rule"
"#
        .replace("filter__empty\nmember", "filter\t\nmember");

        let readable = to_readable_source(&source).expect("readable source");
        assert!(readable.contains("name: 香港"));
        assert!(readable.contains("url: https://example.invalid"));
        assert!(readable.contains("member: subscription:legacy -> HK 03"));
        assert!(!readable.contains("e9a699e6b8af"));

        let restored = to_storage_source(&readable).expect("storage source");
        let original = DocumentMut::from_str(&source).expect("original TOML");
        let restored = DocumentMut::from_str(&restored).expect("restored TOML");
        for name in ["subscription.url", "policy-abc123.policy", "routing.mode"] {
            let before = original
                .get("configuration")
                .and_then(Item::as_table)
                .and_then(|table| table.get(name))
                .and_then(Item::as_value)
                .and_then(|item| item.as_str());
            let after = restored
                .get("configuration")
                .and_then(Item::as_table)
                .and_then(|table| table.get(name))
                .and_then(Item::as_value)
                .and_then(|item| item.as_str());
            assert_eq!(after, before, "entry {name}");
        }
    }

    #[test]
    fn readable_source_omits_remote_rule_content_and_downloads_on_import() {
        let source = r#"schema_version = 1
[configuration]
"qx-rule-abc123.qxrules" = """
manis-qx-rule-source-v3
id	qx-rule-abc123
url	68747470733a2f2f6578616d706c652e696e76616c6964
name	e8a784e58899e6ba902031
target	5553
content	444f4d41494e2d5355464649582c6578616d706c652e696e76616c6964
enabled	1
refresh	manual
last-success	123
"""
"#;

        let readable = to_readable_source(source).expect("readable source");
        assert!(readable.contains("format = \"manis-readable-v2\""));
        assert!(readable.contains("url: https://example.invalid"));
        assert!(!readable.contains("content:"));
        let restored = to_storage_source_with_download(&readable, |url| {
            assert_eq!(url, "https://example.invalid");
            Ok("DOMAIN-SUFFIX,example.invalid".to_owned())
        })
        .expect("storage source");
        let original = DocumentMut::from_str(source).expect("original TOML");
        let restored = DocumentMut::from_str(&restored).expect("restored TOML");
        let before = original
            .get("configuration")
            .and_then(Item::as_table)
            .and_then(|table| table.get("qx-rule-abc123.qxrules"))
            .and_then(Item::as_value)
            .and_then(|item| item.as_str());
        let after = restored
            .get("configuration")
            .and_then(Item::as_table)
            .and_then(|table| table.get("qx-rule-abc123.qxrules"))
            .and_then(Item::as_value)
            .and_then(|item| item.as_str());
        assert_eq!(after, before);
    }

    #[test]
    fn legacy_readable_source_preserves_blank_and_trailing_space_rule_lines() {
        let readable = r#"schema_version = 1
format = "manis-readable-v1"
[configuration]
"qx-rule-abc123.qxrules" = """
# 规则来源
format: manis-qx-rule-source-v3
id: qx-rule-abc123
url: https://example.invalid
target: US
content:
  | DOMAIN-SUFFIX,example.invalid
  | BLANK
  | DOMAIN,other.invalid,DIRECT TRAILING
enabled: 1
refresh: manual
last-success: 123
"""
"#
        .replace("  | BLANK", "  | ")
        .replace("DIRECT TRAILING", "DIRECT  ");
        let trimmed_blank_line = readable.replace("  | \n", "  |\n");
        for readable in [&readable, &trimmed_blank_line] {
            let storage = to_storage_source_with_download(readable, |_| {
                panic!("legacy export contains its own rule content")
            })
            .expect("legacy storage source");
            let document = DocumentMut::from_str(&storage).expect("storage TOML");
            let entry = document["configuration"]["qx-rule-abc123.qxrules"]
                .as_str()
                .expect("rule source");
            let encoded = entry
                .lines()
                .find_map(|line| line.strip_prefix("content\t"))
                .expect("content field");
            assert_eq!(
                String::from_utf8(hex::decode(encoded).expect("hex")).expect("text"),
                "DOMAIN-SUFFIX,example.invalid\n\nDOMAIN,other.invalid,DIRECT  "
            );
        }
    }

    #[test]
    fn compact_rule_source_reports_download_failure() {
        let source = r#"schema_version = 1
[configuration]
"qx-rule-abc123.qxrules" = """
manis-qx-rule-source-v3
id\tqx-rule-abc123
url\t68747470733a2f2f6578616d706c652e696e76616c6964
target\t5553
content\t444f4d41494e2d5355464649582c6578616d706c652e696e76616c6964
enabled\t1
"""
"#
        .replace("\\t", "\t");
        let readable = to_readable_source(&source).expect("readable source");
        assert!(matches!(
            to_storage_source_with_download(&readable, |_| Err(
                BackupError::RemoteSourceUnavailable
            )),
            Err(BackupError::RemoteSourceUnavailable)
        ));
    }

    #[test]
    fn readable_source_round_trips_order_and_workspace_entries() {
        let source = r#"schema_version = 1
[configuration]
"routing-rule-group-order.state" = """
manis-routing-rule-group-order-v1
manual
qx-rule-abc123
"""
"workspace.state" = """
routing-manual-rules
routing-rule-source:qx-rule-abc123
saved
"""
"#;

        let readable = to_readable_source(source).expect("readable source");
        assert!(readable.contains("order: manual"));
        assert!(readable.contains("workspace: saved"));
        let restored = to_storage_source(&readable).expect("storage source");
        let original = DocumentMut::from_str(source).expect("original TOML");
        let restored = DocumentMut::from_str(&restored).expect("restored TOML");
        for name in ["routing-rule-group-order.state", "workspace.state"] {
            let before = original
                .get("configuration")
                .and_then(Item::as_table)
                .and_then(|table| table.get(name))
                .and_then(Item::as_value)
                .and_then(|item| item.as_str());
            let after = restored
                .get("configuration")
                .and_then(Item::as_table)
                .and_then(|table| table.get(name))
                .and_then(Item::as_value)
                .and_then(|item| item.as_str());
            assert_eq!(after, before, "entry {name}");
        }
    }

    #[test]
    fn single_line_value_preserves_trailing_newline() {
        let source =
            "schema_version = 1\n[configuration]\n\"language.preference\" = \"system\\n\"\n";
        let readable = to_readable_source(source).expect("readable source");
        let restored = to_storage_source(&readable).expect("storage source");
        let restored = DocumentMut::from_str(&restored).expect("storage TOML");
        assert_eq!(
            restored["configuration"]["language.preference"].as_str(),
            Some("system\n")
        );
    }
}
