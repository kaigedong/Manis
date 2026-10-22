//! Detects a Tailscale exit node on Linux.
//!
//! A Tailscale exit node needs the IPv4 default route. Manis TUN also takes the default route,
//! through its own policy table. Only one of them can win, and which one wins depends on Tailscale's
//! version: current releases keep their routes in table 52 behind a `lookup 52` rule, while older
//! releases install them in the main table. Either way the other side silently stops working, so
//! the conflict is detected across every routing table instead of assuming one layout.

use std::io;
use std::process::Command;

const IP_CANDIDATES: [&str; 4] = ["/usr/sbin/ip", "/sbin/ip", "/usr/bin/ip", "/bin/ip"];
const TAILSCALE_INTERFACE_PREFIX: &str = "tailscale";

/// Returns the Tailscale interface that owns an IPv4 default route in any routing table.
pub(crate) fn exit_node_interface() -> io::Result<Option<String>> {
    let Some(binary) = ip_binary() else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "iproute2 is not installed",
        ));
    };
    let output = Command::new(binary)
        .args(["-4", "route", "show", "table", "all"])
        .env_clear()
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "ip route exited with {:?}",
            output.status.code()
        )));
    }
    Ok(parse_exit_node(&String::from_utf8_lossy(&output.stdout)))
}

fn ip_binary() -> Option<&'static str> {
    IP_CANDIDATES
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file())
}

fn parse_exit_node(routes: &str) -> Option<String> {
    routes.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let destination = fields.next()?;
        if destination != "default" && destination != "0.0.0.0/0" {
            return None;
        }
        let mut interface = None;
        while let Some(field) = fields.next() {
            if field == "dev" {
                interface = fields.next();
                break;
            }
        }
        interface
            .filter(|name| name.starts_with(TAILSCALE_INTERFACE_PREFIX))
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::parse_exit_node;

    #[test]
    fn detects_the_current_tailscale_table_52_default() {
        let routes = "\
default via 192.168.1.1 dev enp42s0 proto dhcp src 192.168.1.6 metric 100
default via 198.18.0.2 dev Meta table 2022
default dev tailscale0 table 52 scope link
100.113.48.88 dev tailscale0 table 52 scope link
";
        assert_eq!(parse_exit_node(routes).as_deref(), Some("tailscale0"));
    }

    #[test]
    fn detects_a_legacy_main_table_default() {
        let routes = "\
default dev tailscale0 scope link
default via 192.168.1.1 dev enp42s0 proto dhcp metric 100
";
        assert_eq!(parse_exit_node(routes).as_deref(), Some("tailscale0"));
    }

    #[test]
    fn accepts_the_expanded_default_prefix() {
        let routes = "0.0.0.0/0 dev tailscale0 table 52 scope link\n";
        assert_eq!(parse_exit_node(routes).as_deref(), Some("tailscale0"));
    }

    #[test]
    fn ignores_a_tailnet_without_an_exit_node() {
        let routes = "\
default via 192.168.1.1 dev enp42s0 proto dhcp src 192.168.1.6 metric 100
default via 198.18.0.2 dev Meta table 2022
100.100.100.100 dev tailscale0 table 52 scope link
100.113.48.88 dev tailscale0 table 52 scope link
10.147.20.0/24 dev ztwfukspke proto kernel scope link src 10.147.20.145
";
        assert_eq!(parse_exit_node(routes), None);
    }

    #[test]
    fn ignores_other_overlay_defaults() {
        let routes = "default dev ztwfukspke table 52 scope link\n";
        assert_eq!(parse_exit_node(routes), None);
    }

    #[test]
    fn tolerates_empty_and_truncated_output() {
        assert_eq!(parse_exit_node(""), None);
        assert_eq!(parse_exit_node("default dev\n"), None);
        assert_eq!(parse_exit_node("default via 192.168.1.1\n"), None);
    }
}
