use std::net::Ipv4Addr;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use manis_profile::MANIS_EXIT_IP_PROBE_GROUP_NAME;
use ureq::{Agent, Proxy};

use super::{ProxyDelayTarget, put_policy_group_selection};

const EXIT_IP_URL: &str = "https://api.ipify.org";
const EXIT_IP_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_EXIT_IP_RESPONSE_BYTES: u64 = 64;
const MAX_PROBES_PER_RUN: usize = 32;

// The selector is shared by every lookup. Hold the lock across both selection and the HTTP
// request so another benchmark cannot switch the selector while a request is in flight.
static PROBE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(super) fn probe_targets(
    endpoint: &str,
    port: u16,
    controller_secret: Option<&str>,
    targets: &[ProxyDelayTarget],
    mut on_result: impl FnMut(&str, Option<Ipv4Addr>),
) {
    let Ok(_guard) = PROBE_LOCK.get_or_init(|| Mutex::new(())).lock() else {
        return;
    };
    let Ok(proxy) = Proxy::new(&format!("http://127.0.0.1:{port}")) else {
        return;
    };
    let config = Agent::config_builder()
        .proxy(Some(proxy))
        .https_only(true)
        .max_redirects(0)
        .timeout_global(Some(EXIT_IP_TIMEOUT))
        .build();
    for target in targets.iter().take(MAX_PROBES_PER_RUN) {
        // A CONNECT tunnel keeps using the node selected when it was opened. Give each target a
        // fresh connection pool so switching this hidden selector cannot reuse the prior route.
        let agent = Agent::new_with_config(config.clone());
        let ip = put_policy_group_selection(
            endpoint,
            MANIS_EXIT_IP_PROBE_GROUP_NAME,
            target.name(),
            controller_secret,
        )
        .ok()
        .and_then(|()| lookup_ipv4(&agent));
        on_result(target.name(), ip);
    }
}

fn lookup_ipv4(agent: &Agent) -> Option<Ipv4Addr> {
    let mut response = agent.get(EXIT_IP_URL).call().ok()?;
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_EXIT_IP_RESPONSE_BYTES)
        .lossy_utf8(false)
        .read_to_string()
        .ok()?;
    parse_public_ipv4(&body)
}

fn parse_public_ipv4(body: &str) -> Option<Ipv4Addr> {
    let ip: Ipv4Addr = body.trim().parse().ok()?;
    (!ip.is_private()
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_multicast()
        && !ip.is_unspecified()
        && !ip.is_broadcast())
    .then_some(ip)
}

#[cfg(test)]
mod tests {
    use super::parse_public_ipv4;

    #[test]
    fn accepts_only_a_public_ipv4_literal() {
        assert_eq!(
            parse_public_ipv4(" 8.8.8.8\n").map(|ip| ip.to_string()),
            Some("8.8.8.8".to_owned())
        );
        assert!(parse_public_ipv4("10.0.0.1").is_none());
        assert!(parse_public_ipv4("127.0.0.1").is_none());
        assert!(parse_public_ipv4("not an IP").is_none());
        assert!(parse_public_ipv4("2001:4860:4860::8888").is_none());
    }
}
