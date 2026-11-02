//! Host command catalog shared by the desktop dispatcher and the NSH manifest.

/// Every host command the NSH app may call.
pub const COMMANDS: &[&str] = &[
    "nodes.refresh",
    "nodes.select",
    "nodes.selectGroup",
    "subscriptions.toggle",
    "subscriptions.delete",
    "subscriptions.add",
    "subscriptions.addNode",
    "subscriptions.addGroup",
    "subscriptions.refresh",
    "subscriptions.apply",
    "rules.toggle",
    "rules.delete",
    "rules.save",
    "rules.apply",
    "settings.reload",
    "settings.autostart.set",
    "settings.logs",
    "settings.copyLogs",
    "settings.copyBackup",
    "settings.importBackup",
    "settings.dns.set",
    "settings.update.check",
    "settings.update.download",
    "settings.update.install",
    "settings.update.release",
    "connection.toggle",
    "connection.mode",
    "connection.reload",
    "workspace.apply",
    "workspace.discard",
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    #[test]
    fn the_manifest_grants_every_host_command() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../android/nsh-ui/nsh.json"))
                .expect("nsh.json");
        let granted: BTreeSet<_> = manifest["capabilities"]["host"]
            .as_array()
            .expect("host capabilities")
            .iter()
            .map(|value| value.as_str().expect("capability").to_owned())
            .collect();
        let expected: BTreeSet<_> = super::COMMANDS
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        assert_eq!(granted, expected);
    }
}
