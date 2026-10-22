//! Detects a Tailscale exit node on Linux.
//!
//! Manis owns the default route through its TUN. A Tailscale exit node installs its own default
//! route in the main table, which Mihomo deliberately ignores (`suppress_prefixlength 0`), so the
//! exit node would appear enabled while every packet still entered the TUN. Surface the conflict
//! instead of silently routing around it.

use std::io;

/// IPv4 routes for the current network namespace. `/proc/net/route` reports the main table, which
/// is where tailscaled installs its exit-node default; the fwmark bypass table is not relevant.
const PROC_ROUTE: &str = "/proc/net/route";
const TAILSCALE_INTERFACE_PREFIX: &str = "tailscale";

/// Returns the Tailscale interface that currently owns the IPv4 default route.
pub(crate) fn exit_node_interface() -> io::Result<Option<String>> {
    let routes = std::fs::read_to_string(PROC_ROUTE)?;
    Ok(parse_exit_node(&routes))
}

fn parse_exit_node(routes: &str) -> Option<String> {
    routes.lines().skip(1).find_map(|line| {
        let mut fields = line.split_whitespace();
        let interface = fields.next()?;
        let destination = fields.next()?;
        // Gateway, Flags, RefCnt, Use, Metric.
        fields.next()?;
        fields.next()?;
        fields.next()?;
        fields.next()?;
        fields.next()?;
        let mask = fields.next()?;
        (destination == "00000000"
            && mask == "00000000"
            && interface.starts_with(TAILSCALE_INTERFACE_PREFIX))
        .then(|| interface.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::parse_exit_node;

    const HEADER: &str =
        "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n";

    #[test]
    fn detects_the_tailscale_exit_node_default() {
        let routes = format!(
            "{HEADER}tailscale0\t00000000\t00000000\t0001\t0\t0\t0\t00000000\t0\t0\t0\nenp42s0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n"
        );

        assert_eq!(parse_exit_node(&routes).as_deref(), Some("tailscale0"));
    }

    #[test]
    fn ignores_a_physical_default_and_tailscale_subnet_routes() {
        let routes = format!(
            "{HEADER}enp42s0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\ntailscale0\t0064A8C0\t00000000\t0001\t0\t0\t0\t00FFFFFF\t0\t0\t0\n"
        );

        assert_eq!(parse_exit_node(&routes), None);
    }

    #[test]
    fn ignores_other_overlay_interfaces() {
        let routes =
            format!("{HEADER}ztwfukspke\t00000000\t00000000\t0001\t0\t0\t0\t00000000\t0\t0\t0\n");

        assert_eq!(parse_exit_node(&routes), None);
    }

    #[test]
    fn tolerates_an_empty_or_truncated_table() {
        assert_eq!(parse_exit_node(""), None);
        assert_eq!(parse_exit_node(HEADER), None);
        assert_eq!(
            parse_exit_node(&format!("{HEADER}tailscale0\t00000000\n")),
            None
        );
    }
}
