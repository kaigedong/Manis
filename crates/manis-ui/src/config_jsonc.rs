//! JSONC view of the stored configuration.
//!
//! The on-disk `config.toml` stays the source of truth: one entry per key, each holding an internal
//! `manis-*-vN` record. That layout is lossless but unreadable, so the editor and the export render
//! the same data as one flat JSONC document instead.
//!
//! Three rules keep the conversion safe:
//! - every entity keeps its stable `id`, so entry names and policy members rebuild exactly;
//! - member sources are shown by name and resolved back through `id` on import;
//! - anything unrecognised is carried through verbatim in `state`, and `format` keeps the record
//!   kind. Cached rule bodies and `last-success` timestamps are not exported: they are regenerated.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::str::FromStr;

use toml_edit::{DocumentMut, Item};

use super::config_backup::BackupError;

pub(super) const JSONC_FORMAT: &str = "manis-config-v1";

const SUBSCRIPTION_KEY: &str = "subscription.url";
const ROUTING_MODE_KEY: &str = "routing.mode";
const LANGUAGE_KEY: &str = "language.preference";

const SUBSCRIPTION_FORMAT: &str = "manis-subscription-source-v3";
const GROUP_FORMAT: &str = "manis-policy-group-v1";
const RULE_SOURCE_FORMAT: &str = "manis-qx-rule-source-v3";

/// One stored entry: its internal format line plus ordered key/value fields.
struct Entry {
    name: String,
    format: String,
    fields: Vec<(String, Vec<String>)>,
}

impl Entry {
    fn field(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(name, _)| name == key)
            .and_then(|(_, values)| values.first())
            .map(String::as_str)
    }

    fn all(&self, key: &str) -> Vec<&[String]> {
        self.fields
            .iter()
            .filter(|(name, _)| name == key)
            .map(|(_, values)| values.as_slice())
            .collect()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Group,
    RuleSource,
    Node,
    Subscription,
    State,
}

fn kind_of(name: &str) -> Kind {
    match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("policy" | "group") => Kind::Group,
        Some("qxrules") => Kind::RuleSource,
        Some("vless") => Kind::Node,
        _ if name == SUBSCRIPTION_KEY => Kind::Subscription,
        _ => Kind::State,
    }
}

fn is_hex_field(key: &str) -> bool {
    matches!(key, "name" | "filter" | "url" | "target" | "proxy-dns")
}

fn decode_field(value: &str) -> String {
    match hex_decode(value) {
        Some(bytes) => String::from_utf8(bytes).unwrap_or_else(|_| value.to_owned()),
        None => value.to_owned(),
    }
}

fn encode_field(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.is_empty() || !value.len().is_multiple_of(2) {
        return None;
    }
    let raw = value.as_bytes();
    let mut bytes = Vec::with_capacity(raw.len() / 2);
    let mut index = 0;
    while index < raw.len() {
        let high = (raw[index] as char).to_digit(16)?;
        let low = (raw[index + 1] as char).to_digit(16)?;
        bytes.push(u8::try_from(high * 16 + low).ok()?);
        index += 2;
    }
    Some(bytes)
}

fn parse_entries(source: &str) -> Result<Vec<Entry>, BackupError> {
    let document = DocumentMut::from_str(source).map_err(|_| BackupError::InvalidFormat)?;
    let table = document
        .get("configuration")
        .and_then(Item::as_table)
        .ok_or(BackupError::InvalidFormat)?;
    let mut entries = Vec::new();
    for (name, item) in table {
        let contents = item
            .as_value()
            .and_then(|value| value.as_str())
            .ok_or(BackupError::InvalidFormat)?;
        entries.push(parse_entry(name, contents));
    }
    Ok(entries)
}

fn parse_entry(name: &str, contents: &str) -> Entry {
    let mut lines = contents.lines().filter(|line| !line.is_empty());
    let format = lines.next().unwrap_or_default().to_owned();
    let mut fields = Vec::new();
    for line in lines {
        let mut parts = line.split('\t');
        let Some(key) = parts.next() else { continue };
        fields.push((key.to_owned(), parts.map(str::to_owned).collect()));
    }
    Entry {
        name: name.to_owned(),
        format,
        fields,
    }
}

/// Rewrites the stored configuration as one JSONC document.
///
/// Import already accepts the document this produces, so the editor and an exported file use the
/// same text.
pub(super) fn to_jsonc(source: &str) -> Result<String, BackupError> {
    let entries = parse_entries(source)?;

    let mut group_names = BTreeMap::new();
    let mut subscription_names = BTreeMap::new();
    for entry in &entries {
        let (Some(id), Some(name)) = (entry.field("id"), entry.field("name")) else {
            continue;
        };
        match kind_of(&entry.name) {
            Kind::Group => {
                group_names.insert(id.to_owned(), decode_field(name));
            }
            Kind::Subscription => {
                subscription_names.insert(id.to_owned(), decode_field(name));
            }
            _ => {}
        }
    }

    let mut subscription = String::new();
    let mut groups = String::new();
    let mut rule_sources = String::new();
    let mut nodes = String::new();
    let mut others = String::new();
    let mut routing_mode: Option<String> = None;
    let mut language: Option<String> = None;

    for entry in &entries {
        if entry.name == ROUTING_MODE_KEY {
            routing_mode = Some(entry.format.clone());
            continue;
        }
        if entry.name == LANGUAGE_KEY {
            language = Some(entry.format.clone());
            continue;
        }
        match kind_of(&entry.name) {
            Kind::Subscription => subscription = render_subscription(entry),
            Kind::Group => push(
                &mut groups,
                &render_group(entry, &subscription_names, &group_names),
            ),
            Kind::RuleSource => push(&mut rule_sources, &render_rule_source(entry)),
            Kind::Node => push(&mut nodes, &render_record(entry, 4)),
            Kind::State => {
                let _ = write!(
                    others,
                    "\n    {}: {},",
                    quoted(&entry.name),
                    render_record(entry, 6)
                );
            }
        }
    }

    let mut out = String::new();
    out.push_str("{\n");
    let _ = writeln!(out, "  \"format\": {},", quoted(JSONC_FORMAT));
    if let Some(mode) = routing_mode {
        let _ = writeln!(out, "  \"routing\": {{ \"mode\": {} }},", quoted(&mode));
    }
    if let Some(language) = language {
        let _ = writeln!(out, "  \"language\": {},", quoted(&language));
    }
    if !subscription.is_empty() {
        let _ = writeln!(out, "  \"subscription\": {subscription},");
    }
    let _ = writeln!(out, "  \"groups\": [{groups}],");
    let _ = writeln!(out, "  \"ruleSources\": [{rule_sources}],");
    let _ = writeln!(out, "  \"nodes\": [{nodes}],");
    out.push_str("  \"state\": {");
    out.push_str(&others);
    out.push_str("\n  }\n}\n");
    Ok(out)
}

fn push(target: &mut String, rendered: &str) {
    if !target.is_empty() {
        target.push(',');
    }
    target.push_str(rendered);
}

fn item_line(out: &mut String, indent: usize, key: &str, value: &str) {
    let pad = " ".repeat(indent);
    let _ = writeln!(out, "{pad}{}: {value},", quoted(key));
}

fn render_subscription(entry: &Entry) -> String {
    let mut out = String::from("{\n");
    let mut known = Vec::new();
    if let Some(id) = entry.field("id") {
        item_line(&mut out, 4, "id", &quoted(id));
        known.push("id");
    }
    if let Some(name) = entry.field("name") {
        item_line(&mut out, 4, "name", &quoted(&decode_field(name)));
        known.push("name");
    }
    if let Some(enabled) = entry.field("enabled") {
        item_line(&mut out, 4, "enabled", enabled);
        known.push("enabled");
    }
    if let Some(url) = entry.field("url") {
        item_line(&mut out, 4, "url", &quoted(&decode_field(url)));
        known.push("url");
    }
    if let Some(refresh) = entry.field("refresh") {
        item_line(&mut out, 4, "refresh", &quoted(refresh));
        known.push("refresh");
    }
    let dns: Vec<String> = entry
        .all("proxy-dns")
        .into_iter()
        .filter_map(|values| values.first())
        .map(|value| quoted(&decode_field(value)))
        .collect();
    if !dns.is_empty() {
        let _ = writeln!(out, "    \"proxyDns\": [{}],", dns.join(", "));
        known.push("proxy-dns");
    }
    known.push("last-success");
    render_extras(&mut out, entry, &known, 4);
    out.push_str("  }");
    out
}

fn render_group(
    entry: &Entry,
    subscription_names: &BTreeMap<String, String>,
    group_names: &BTreeMap<String, String>,
) -> String {
    let mut out = String::from("\n    {\n");
    let mut known = Vec::new();
    if let Some(id) = entry.field("id") {
        item_line(&mut out, 6, "id", &quoted(id));
        known.push("id");
    }
    if let Some(name) = entry.field("name") {
        item_line(&mut out, 6, "name", &quoted(&decode_field(name)));
        known.push("name");
    }
    if let Some(icon) = entry.field("icon")
        && icon != "none"
    {
        item_line(&mut out, 6, "icon", &quoted(icon));
    }
    known.push("icon");
    if let Some(strategy) = entry.field("strategy") {
        item_line(&mut out, 6, "strategy", &quoted(strategy));
        known.push("strategy");
    }
    if let Some(interval) = entry.field("interval") {
        item_line(&mut out, 6, "interval", &number_or_string(interval));
        known.push("interval");
    }
    if let Some(tolerance) = entry.field("tolerance-ms") {
        item_line(&mut out, 6, "toleranceMs", &number_or_string(tolerance));
        known.push("tolerance-ms");
    }
    if let Some(matcher) = entry.field("matcher") {
        if matcher != "explicit" {
            item_line(&mut out, 6, "matcher", &quoted(matcher));
        }
        known.push("matcher");
    }
    if let Some(filter) = entry.field("filter") {
        if !filter.is_empty() {
            item_line(&mut out, 6, "filter", &quoted(&decode_field(filter)));
        }
        known.push("filter");
    }

    let members: Vec<(String, String)> = entry
        .all("member")
        .into_iter()
        .map(|values| {
            let from = values.first().map(String::as_str).unwrap_or_default();
            let node = values.get(1).map(String::as_str).unwrap_or_default();
            (
                resolve_source(&decode_field(from), subscription_names, group_names),
                decode_field(node),
            )
        })
        .collect();
    if !members.is_empty() {
        out.push_str("      \"members\": [\n");
        for (from, node) in &members {
            let _ = writeln!(
                out,
                "        {{ \"from\": {}, \"node\": {} }},",
                quoted(from),
                quoted(node)
            );
        }
        out.push_str("      ],\n");
        known.push("member");
    }
    render_extras(&mut out, entry, &known, 6);
    out.push_str("    }");
    out
}

fn render_rule_source(entry: &Entry) -> String {
    let mut out = String::from("\n    {\n");
    let mut known = Vec::new();
    if let Some(id) = entry.field("id") {
        item_line(&mut out, 6, "id", &quoted(id));
        known.push("id");
    }
    if let Some(name) = entry.field("name") {
        item_line(&mut out, 6, "name", &quoted(&decode_field(name)));
        known.push("name");
    }
    if let Some(url) = entry.field("url") {
        item_line(&mut out, 6, "url", &quoted(&decode_field(url)));
        known.push("url");
    }
    if let Some(target) = entry.field("target") {
        item_line(&mut out, 6, "target", &quoted(&decode_field(target)));
        known.push("target");
    }
    if let Some(enabled) = entry.field("enabled") {
        let enabled = enabled == "1" || enabled == "true";
        item_line(&mut out, 6, "enabled", &enabled.to_string());
        known.push("enabled");
    }
    if let Some(refresh) = entry.field("refresh") {
        item_line(&mut out, 6, "refresh", &quoted(refresh));
        known.push("refresh");
    }
    if entry.field("content").is_some() {
        out.push_str("      \"content\": \"<re-downloaded on import>\",\n");
    }
    known.push("content");
    known.push("last-success");
    render_extras(&mut out, entry, &known, 6);
    out.push_str("    }");
    out
}

/// Renders an entry without a dedicated shape: `format` plus every field as stored.
fn render_record(entry: &Entry, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let inner = " ".repeat(indent + 2);
    let mut out = String::from("{\n");
    let _ = writeln!(out, "{inner}\"format\": {},", quoted(&entry.format));
    for (key, values) in &entry.fields {
        let rendered: Vec<String> = values
            .iter()
            .map(|value| quoted(&decode_field_if(key, value)))
            .collect();
        let value = if rendered.len() == 1 {
            rendered[0].clone()
        } else {
            format!("[{}]", rendered.join(", "))
        };
        let _ = writeln!(out, "{inner}{}: {value},", quoted(key));
    }
    if entry.fields.is_empty() {
        // A single-line entry stores its whole value in `format`; keep the object valid.
        out.pop();
        out.pop();
        out.push('\n');
    }
    out.push_str(&pad);
    out.push('}');
    out
}

fn render_extras(out: &mut String, entry: &Entry, known: &[&str], indent: usize) {
    for (key, values) in &entry.fields {
        if known.contains(&key.as_str()) {
            continue;
        }
        let rendered: Vec<String> = values
            .iter()
            .map(|value| quoted(&decode_field_if(key, value)))
            .collect();
        let value = if rendered.len() == 1 {
            rendered[0].clone()
        } else {
            format!("[{}]", rendered.join(", "))
        };
        item_line(out, indent, key, &value);
    }
}

fn decode_field_if(key: &str, value: &str) -> String {
    if is_hex_field(key) {
        decode_field(value)
    } else {
        value.to_owned()
    }
}

fn resolve_source(
    from: &str,
    subscription_names: &BTreeMap<String, String>,
    group_names: &BTreeMap<String, String>,
) -> String {
    if let Some(id) = from.strip_prefix("subscription:") {
        return subscription_names
            .get(id)
            .cloned()
            .unwrap_or_else(|| from.to_owned());
    }
    if let Some(id) = from.strip_prefix("policy:") {
        return group_names
            .get(id)
            .cloned()
            .unwrap_or_else(|| from.to_owned());
    }
    from.to_owned()
}

fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            character if (character as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", character as u32);
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

fn number_or_string(value: &str) -> String {
    if value.parse::<i64>().is_ok() {
        value.to_owned()
    } else {
        quoted(value)
    }
}

/// Turns JSONC back into the stored configuration document.
pub(super) fn from_jsonc(text: &str) -> Result<String, BackupError> {
    from_jsonc_with_download(text, |url| {
        crate::rule_source::download_qx_rule_document(url)
            .map_err(|_| BackupError::RemoteSourceUnavailable)
    })
}

fn from_jsonc_with_download(
    text: &str,
    download: impl Fn(&str) -> Result<String, BackupError>,
) -> Result<String, BackupError> {
    let stripped = strip_jsonc(text)?;
    let document: serde_json::Value =
        serde_json::from_str(&stripped).map_err(|_| BackupError::InvalidFormat)?;
    let root = document.as_object().ok_or(BackupError::InvalidFormat)?;
    if root.get("format").and_then(serde_json::Value::as_str) != Some(JSONC_FORMAT) {
        return Err(BackupError::InvalidFormat);
    }

    let mut subscription_ids = BTreeMap::new();
    let mut group_ids = BTreeMap::new();
    if let Some(subscription) = root.get("subscription").and_then(|value| value.as_object()) {
        collect_ids(subscription, &mut subscription_ids);
    }
    for group in array(root, "groups") {
        collect_ids(&group, &mut group_ids);
    }

    let mut entries = Vec::new();
    if let Some(mode) = root
        .get("routing")
        .and_then(|value| value.get("mode"))
        .and_then(serde_json::Value::as_str)
    {
        entries.push(Entry {
            name: ROUTING_MODE_KEY.to_owned(),
            format: mode.to_owned(),
            fields: Vec::new(),
        });
    }
    if let Some(language) = root.get("language").and_then(serde_json::Value::as_str) {
        entries.push(Entry {
            name: LANGUAGE_KEY.to_owned(),
            format: language.to_owned(),
            fields: Vec::new(),
        });
    }
    if let Some(subscription) = root.get("subscription").and_then(|value| value.as_object()) {
        entries.push(subscription_entry(subscription));
    }
    for group in array(root, "groups") {
        entries.push(group_entry(&group, &subscription_ids, &group_ids)?);
    }
    for source in array(root, "ruleSources") {
        entries.push(rule_source_entry(&source, &download)?);
    }
    for node in array(root, "nodes") {
        let id = string_field(&node, "id").ok_or(BackupError::InvalidFormat)?;
        entries.push(record_entry(format!("{id}.vless"), &node)?);
    }
    if let Some(state) = root.get("state").and_then(|value| value.as_object()) {
        for (name, value) in state {
            let object = value.as_object().ok_or(BackupError::InvalidFormat)?;
            entries.push(record_entry(name.clone(), object)?);
        }
    }

    let mut out = String::from("schema_version = 1\n\n[configuration]\n");
    for entry in &entries {
        let _ = writeln!(out, "{} = \"\"\"", quoted(&entry.name));
        out.push_str(&entry.format);
        out.push('\n');
        for (key, values) in &entry.fields {
            out.push_str(key);
            for value in values {
                out.push('\t');
                out.push_str(value);
            }
            out.push('\n');
        }
        out.push_str("\"\"\"\n");
    }
    Ok(out)
}

fn array(
    root: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Vec<serde_json::Map<String, serde_json::Value>> {
    root.get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_object().cloned())
                .collect()
        })
        .unwrap_or_default()
}

fn collect_ids(
    object: &serde_json::Map<String, serde_json::Value>,
    into: &mut BTreeMap<String, String>,
) {
    if let (Some(name), Some(id)) = (string_field(object, "name"), string_field(object, "id")) {
        into.insert(name, id);
    }
}

fn subscription_entry(object: &serde_json::Map<String, serde_json::Value>) -> Entry {
    let mut fields = Vec::new();
    if let Some(id) = string_field(object, "id") {
        fields.push(("id".to_owned(), vec![id]));
    }
    if let Some(name) = string_field(object, "name") {
        fields.push(("name".to_owned(), vec![encode_field(&name)]));
    }
    if let Some(enabled) = object.get("enabled").and_then(serde_json::Value::as_bool) {
        fields.push(("enabled".to_owned(), vec![enabled.to_string()]));
    }
    if let Some(url) = string_field(object, "url") {
        fields.push(("url".to_owned(), vec![encode_field(&url)]));
    }
    if let Some(refresh) = string_field(object, "refresh") {
        fields.push(("refresh".to_owned(), vec![refresh]));
    }
    if let Some(servers) = object.get("proxyDns").and_then(|value| value.as_array()) {
        for server in servers {
            if let Some(server) = server.as_str() {
                fields.push(("proxy-dns".to_owned(), vec![encode_field(server)]));
            }
        }
    }
    Entry {
        name: SUBSCRIPTION_KEY.to_owned(),
        format: SUBSCRIPTION_FORMAT.to_owned(),
        fields,
    }
}

fn group_entry(
    object: &serde_json::Map<String, serde_json::Value>,
    subscription_ids: &BTreeMap<String, String>,
    group_ids: &BTreeMap<String, String>,
) -> Result<Entry, BackupError> {
    let id = string_field(object, "id").ok_or(BackupError::InvalidFormat)?;
    let mut fields = vec![("id".to_owned(), vec![id.clone()])];
    fields.push((
        "name".to_owned(),
        vec![encode_field(
            &string_field(object, "name").unwrap_or_default(),
        )],
    ));
    if let Some(icon) = string_field(object, "icon") {
        fields.push(("icon".to_owned(), vec![icon]));
    } else {
        fields.push(("icon".to_owned(), vec!["none".to_owned()]));
    }
    if let Some(strategy) = string_field(object, "strategy") {
        fields.push(("strategy".to_owned(), vec![strategy]));
    }
    if let Some(interval) = scalar_text(object.get("interval")) {
        fields.push(("interval".to_owned(), vec![interval]));
    }
    if let Some(tolerance) = scalar_text(object.get("toleranceMs")) {
        fields.push(("tolerance-ms".to_owned(), vec![tolerance]));
    }
    fields.push((
        "matcher".to_owned(),
        vec![string_field(object, "matcher").unwrap_or_else(|| "explicit".to_owned())],
    ));
    fields.push((
        "filter".to_owned(),
        vec![encode_field(
            &string_field(object, "filter").unwrap_or_default(),
        )],
    ));
    if let Some(members) = object.get("members").and_then(|value| value.as_array()) {
        for member in members {
            let member = member.as_object().ok_or(BackupError::InvalidFormat)?;
            let from = string_field(member, "from").ok_or(BackupError::InvalidFormat)?;
            let node = string_field(member, "node").ok_or(BackupError::InvalidFormat)?;
            fields.push((
                "member".to_owned(),
                vec![
                    encode_field(&resolve_back(&from, subscription_ids, group_ids)?),
                    encode_field(&node),
                ],
            ));
        }
    }
    Ok(Entry {
        name: format!("{id}.policy"),
        format: GROUP_FORMAT.to_owned(),
        fields,
    })
}

fn resolve_back(
    from: &str,
    subscription_ids: &BTreeMap<String, String>,
    group_ids: &BTreeMap<String, String>,
) -> Result<String, BackupError> {
    match from {
        "builtin" | "saved" => Ok(from.to_owned()),
        name => {
            if fake_prefix(name).is_some() {
                return Ok(name.to_owned());
            }
            if let Some(id) = subscription_ids.get(name) {
                return Ok(format!("subscription:{id}"));
            }
            if let Some(id) = group_ids.get(name) {
                return Ok(format!("policy:{id}"));
            }
            Err(BackupError::InvalidConfiguration)
        }
    }
}

/// A source the writer could not resolve is kept verbatim, so re-import stays stable.
fn fake_prefix(from: &str) -> Option<&str> {
    from.strip_prefix("subscription:")
        .or_else(|| from.strip_prefix("policy:"))
}

fn rule_source_entry(
    object: &serde_json::Map<String, serde_json::Value>,
    download: &impl Fn(&str) -> Result<String, BackupError>,
) -> Result<Entry, BackupError> {
    let id = string_field(object, "id").ok_or(BackupError::InvalidFormat)?;
    let mut fields = vec![("id".to_owned(), vec![id.clone()])];
    let url = string_field(object, "url");
    if let Some(url) = &url {
        fields.push(("url".to_owned(), vec![encode_field(url)]));
    }
    if let Some(name) = string_field(object, "name") {
        fields.push(("name".to_owned(), vec![encode_field(&name)]));
    }
    if let Some(target) = string_field(object, "target") {
        fields.push(("target".to_owned(), vec![encode_field(&target)]));
    }
    if let Some(enabled) = object.get("enabled").and_then(serde_json::Value::as_bool) {
        fields.push(("enabled".to_owned(), vec![u8::from(enabled).to_string()]));
    }
    if let Some(refresh) = string_field(object, "refresh") {
        fields.push(("refresh".to_owned(), vec![refresh]));
    }
    if let Some(url) = url {
        let content = download(&url)?;
        fields.push(("content".to_owned(), vec![encode_field(&content)]));
    }
    Ok(Entry {
        name: format!("{id}.qxrules"),
        format: RULE_SOURCE_FORMAT.to_owned(),
        fields,
    })
}

fn record_entry(
    name: String,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<Entry, BackupError> {
    let format = string_field(object, "format").unwrap_or_default();
    let mut fields = Vec::new();
    for (key, value) in object {
        if key == "format" {
            continue;
        }
        let values: Vec<String> = match value {
            serde_json::Value::Array(items) => items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect(),
            serde_json::Value::String(text) => vec![text.clone()],
            serde_json::Value::Bool(flag) => vec![flag.to_string()],
            serde_json::Value::Number(number) => vec![number.to_string()],
            _ => return Err(BackupError::InvalidFormat),
        };
        let encoded = if is_hex_field(key) {
            values.iter().map(|value| encode_field(value)).collect()
        } else {
            values
        };
        fields.push((key.clone(), encoded));
    }
    Ok(Entry {
        name,
        format,
        fields,
    })
}

fn string_field(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn scalar_text(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

/// Removes `//` and `/* */` comments plus trailing commas, leaving string contents untouched.
fn strip_jsonc(text: &str) -> Result<String, BackupError> {
    let characters: Vec<char> = text.chars().collect();
    let mut without_comments = String::with_capacity(text.len());
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < characters.len() {
        let character = characters[index];
        if in_string {
            without_comments.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if character == '"' {
            in_string = true;
            without_comments.push(character);
            index += 1;
            continue;
        }
        if character == '/' && characters.get(index + 1) == Some(&'/') {
            while index < characters.len() && characters[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if character == '/' && characters.get(index + 1) == Some(&'*') {
            index += 2;
            while index + 1 < characters.len()
                && !(characters[index] == '*' && characters[index + 1] == '/')
            {
                index += 1;
            }
            index = (index + 2).min(characters.len());
            continue;
        }
        without_comments.push(character);
        index += 1;
    }
    if in_string {
        return Err(BackupError::InvalidFormat);
    }

    let characters: Vec<char> = without_comments.chars().collect();
    let mut out = String::with_capacity(without_comments.len());
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in characters.iter().enumerate() {
        if in_string {
            out.push(*character);
            if escaped {
                escaped = false;
            } else if *character == '\\' {
                escaped = true;
            } else if *character == '"' {
                in_string = false;
            }
            continue;
        }
        if *character == '"' {
            in_string = true;
            out.push(*character);
            continue;
        }
        if *character == ',' {
            let next = characters[index + 1..]
                .iter()
                .find(|character| !character.is_whitespace());
            if matches!(next, Some('}' | ']')) {
                continue;
            }
        }
        out.push(*character);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(name: &str, format: &str, fields: &[(&str, &[String])]) -> String {
        let mut out = format!("\"{name}\" = \"\"\"\n{format}\n");
        for (key, values) in fields {
            out.push_str(key);
            for value in *values {
                out.push('\t');
                out.push_str(value);
            }
            out.push('\n');
        }
        out.push_str("\"\"\"\n");
        out
    }

    fn hex(value: &str) -> String {
        encode_field(value)
    }

    fn fixture() -> String {
        let mut source = String::from("schema_version = 1\n\n[configuration]\n");
        source.push_str(&block("routing.mode", "rule", &[]));
        source.push_str(&block("language.preference", "zh-Hans", &[]));
        source.push_str(&block(
            "subscription.url",
            SUBSCRIPTION_FORMAT,
            &[
                ("id", &["subscription:legacy".to_owned()]),
                ("name", &[hex("NaiU_Net")]),
                ("enabled", &["true".to_owned()]),
                ("url", &[hex("https://example.invalid/sub")]),
                ("refresh", &["1h".to_owned()]),
                ("last-success", &["1700000000".to_owned()]),
                ("proxy-dns", &[hex("https://dns.example.invalid/dns-query")]),
            ],
        ));
        source.push_str(&block(
            "group-1.policy",
            GROUP_FORMAT,
            &[
                ("id", &["group-1".to_owned()]),
                ("name", &[hex("香港")]),
                ("icon", &["none".to_owned()]),
                ("strategy", &["manual".to_owned()]),
                ("interval", &["1800".to_owned()]),
                ("tolerance-ms", &["150".to_owned()]),
                ("matcher", &["explicit".to_owned()]),
                ("filter", &[String::new()]),
                (
                    "member",
                    &[hex("subscription:subscription:legacy"), hex("HK 03")],
                ),
                (
                    "member",
                    &[hex("subscription:subscription:legacy"), hex("HK 04")],
                ),
            ],
        ));
        source.push_str(&block(
            "group-2.policy",
            GROUP_FORMAT,
            &[
                ("id", &["group-2".to_owned()]),
                ("name", &[hex("机场专线")]),
                ("icon", &["none".to_owned()]),
                ("strategy", &["manual".to_owned()]),
                ("interval", &["600".to_owned()]),
                ("tolerance-ms", &["150".to_owned()]),
                ("matcher", &["explicit".to_owned()]),
                ("filter", &[String::new()]),
                ("member", &[hex("builtin"), hex("DIRECT")]),
                ("member", &[hex("policy:group-1"), hex("香港")]),
                ("member", &[hex("saved"), hex("node-a")]),
            ],
        ));
        source.push_str(&block(
            "qx-1.qxrules",
            RULE_SOURCE_FORMAT,
            &[
                ("id", &["qx-1".to_owned()]),
                ("url", &[hex("https://rules.example.invalid/list")]),
                ("name", &[hex("规则源 1")]),
                ("target", &[hex("机场专线")]),
                ("enabled", &["1".to_owned()]),
                ("refresh", &["24h".to_owned()]),
                ("content", &[hex("HOST,x,DIRECT\nHOST,y,DIRECT")]),
            ],
        ));
        source.push_str(&block(
            "node-selection.state",
            "manis-node-selection-v1",
            &[
                ("group", &["group-1".to_owned()]),
                ("node", &[hex("HK 04")]),
            ],
        ));
        source
    }

    fn canonical(source: &str) -> Vec<String> {
        parse_entries(source)
            .expect("entries parse")
            .into_iter()
            .map(|entry| {
                let mut text = format!("{}|{}", entry.name, entry.format);
                for (key, values) in entry.fields {
                    if key == "last-success" {
                        continue;
                    }
                    text.push('|');
                    text.push_str(&key);
                    for value in values {
                        text.push('=');
                        text.push_str(&value);
                    }
                }
                text
            })
            .collect()
    }

    #[test]
    fn renders_names_and_members_instead_of_internal_ids() {
        let jsonc = to_jsonc(&fixture()).expect("to_jsonc");
        assert!(jsonc.contains("\"name\": \"香港\""), "{jsonc}");
        assert!(jsonc.contains("\"from\": \"NaiU_Net\""), "{jsonc}");
        assert!(jsonc.contains("\"node\": \"HK 04\""), "{jsonc}");
        assert!(
            jsonc.contains("\"proxyDns\": [\"https://dns.example.invalid/dns-query\"]"),
            "{jsonc}"
        );
        assert!(jsonc.contains("\"interval\": 1800"), "{jsonc}");
        assert!(jsonc.contains("\"enabled\": true"), "{jsonc}");
    }

    #[test]
    fn round_trips_the_stored_entries() {
        let source = fixture();
        let jsonc = to_jsonc(&source).expect("to_jsonc");
        let back =
            from_jsonc_with_download(&jsonc, |_url| Ok("HOST,x,DIRECT\nHOST,y,DIRECT".to_owned()))
                .expect("from_jsonc");
        assert_eq!(canonical(&source), canonical(&back), "{back}");
    }

    #[test]
    fn accepts_comments_and_trailing_commas() {
        let jsonc =
            "{\n  // note\n  \"format\": \"manis-config-v1\", /* block */\n  \"groups\": [ ],\n}\n";
        let back = from_jsonc_with_download(jsonc, |_url| Ok(String::new())).expect("from_jsonc");
        assert!(back.contains("schema_version = 1"));
    }

    #[test]
    fn rejects_a_foreign_document() {
        assert!(matches!(
            from_jsonc_with_download("{\"format\": \"other\"}", |_url| Ok(String::new())),
            Err(BackupError::InvalidFormat)
        ));
    }
}
