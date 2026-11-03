use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gpui::Context;

use super::{Duration, ManisApp, benchmark_timestamp, mihomo};

const EXIT_IP_CACHE_SECS: u64 = 30 * 60;
const EXIT_IP_RETRY_SECS: u64 = 5 * 60;
type ExitIpUpdates = Arc<Mutex<VecDeque<(String, Option<String>)>>>;

impl ManisApp {
    pub(in crate::app) fn exit_ip_label(&self, node_name: &str) -> Option<&str> {
        let now = benchmark_timestamp();
        self.exit_ips.get(node_name).and_then(|(ip, checked_at)| {
            (now.saturating_sub(*checked_at) < EXIT_IP_CACHE_SECS)
                .then_some(ip.as_deref())
                .flatten()
        })
    }

    fn exit_ip_recently_checked(&self, node_name: &str) -> bool {
        let now = benchmark_timestamp();
        self.exit_ips
            .get(node_name)
            .is_some_and(|(ip, checked_at)| {
                now.saturating_sub(*checked_at)
                    < if ip.is_some() {
                        EXIT_IP_CACHE_SECS
                    } else {
                        EXIT_IP_RETRY_SECS
                    }
            })
    }

    pub(in crate::app) fn start_exit_ip_probe(
        &mut self,
        targets: Vec<mihomo::ProxyDelayTarget>,
        force_refresh: bool,
        cx: &mut Context<Self>,
    ) {
        if self.exit_ip_probe_active {
            return;
        }
        let targets = self.exit_ip_targets_to_probe(targets, force_refresh);
        if targets.is_empty() {
            return;
        }
        self.exit_ip_probe_active = true;
        let updates = Arc::new(Mutex::new(VecDeque::new()));
        let done = Arc::new(AtomicBool::new(false));
        self.poll_exit_ip_progress(updates.clone(), done.clone(), cx);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            executor
                .spawn(async move {
                    runtime.probe_exit_ip_targets(&targets, |name, ip| {
                        if let Ok(mut queue) = updates.lock() {
                            queue.push_back((name.to_owned(), ip.map(|ip| ip.to_string())));
                        }
                    });
                    done.store(true, Ordering::Release);
                })
                .await;
            this.update(cx, |_this, cx| cx.notify()).ok();
        })
        .detach();
    }

    fn exit_ip_targets_to_probe(
        &self,
        targets: Vec<mihomo::ProxyDelayTarget>,
        force_refresh: bool,
    ) -> Vec<mihomo::ProxyDelayTarget> {
        targets
            .into_iter()
            .filter(|target| force_refresh || !self.exit_ip_recently_checked(target.name()))
            .collect()
    }

    fn poll_exit_ip_progress(
        &mut self,
        updates: ExitIpUpdates,
        done: Arc<AtomicBool>,
        cx: &mut Context<Self>,
    ) {
        let drained = updates
            .lock()
            .map(|mut queue| queue.drain(..).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut changed = false;
        for (name, ip) in drained {
            changed |= ip.is_some();
            self.exit_ips.insert(name, (ip, benchmark_timestamp()));
        }
        if changed {
            cx.notify();
        }
        if done.load(Ordering::Acquire) {
            self.exit_ip_probe_active = false;
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.poll_exit_ip_progress(updates, done, cx);
                });
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_benchmark_rechecks_a_cached_exit_ip() {
        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
        app.exit_ips.insert(
            "HK 04".to_owned(),
            (Some("81.168.109.195".to_owned()), benchmark_timestamp()),
        );
        let target = || vec![mihomo::ProxyDelayTarget::direct("HK 04")];

        assert!(app.exit_ip_targets_to_probe(target(), false).is_empty());
        assert_eq!(app.exit_ip_targets_to_probe(target(), true), target());
    }
}
