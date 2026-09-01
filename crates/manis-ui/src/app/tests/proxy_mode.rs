use super::{
    ControllerReadiness, ProxyMode, ProxyModeBlock, SourceRuntimeApply, TunSupport, mihomo,
    proxy_mode_block, tun_dns_log_details,
};

#[test]
fn tun_dns_diagnostics_describe_the_platform_strategy() {
    let details = tun_dns_log_details();
    #[cfg(target_os = "macos")]
    {
        assert!(
            details
                .install_requested
                .contains("strategy=system_resolver")
        );
        assert!(
            details
                .install_requested
                .contains("resolver=114.114.114.114")
        );
        assert!(details.restore_succeeded.contains("recovery=removed"));
    }
    #[cfg(target_os = "linux")]
    {
        assert!(
            details
                .install_requested
                .contains("strategy=systemd_resolved")
        );
        assert!(details.install_requested.contains("resolver=198.18.0.2"));
        assert!(details.install_requested.contains("domain=~."));
        assert!(details.install_succeeded.contains("cache=flushed"));
        assert!(details.restore_succeeded.contains("recovery=removed"));
    }
}

#[test]
fn tun_is_blocked_until_a_capable_managed_kernel_is_connected() {
    assert_eq!(
        proxy_mode_block(
            ProxyMode::Tun,
            None,
            ControllerReadiness::Disconnected,
            TunSupport::Supported
        ),
        Some(ProxyModeBlock::ControllerNotConnected)
    );
    assert_eq!(
        proxy_mode_block(
            ProxyMode::Tun,
            None,
            ControllerReadiness::Connected,
            TunSupport::KernelUnsupported
        ),
        Some(ProxyModeBlock::KernelHasNoTun)
    );
    assert_eq!(
        proxy_mode_block(
            ProxyMode::Tun,
            None,
            ControllerReadiness::Connected,
            TunSupport::FixtureReadOnly
        ),
        Some(ProxyModeBlock::FixtureReadOnly)
    );
    assert_eq!(
        proxy_mode_block(
            ProxyMode::Tun,
            None,
            ControllerReadiness::Connected,
            TunSupport::Supported
        ),
        None
    );
}

#[test]
fn the_system_proxy_only_needs_a_connected_controller() {
    assert_eq!(
        proxy_mode_block(
            ProxyMode::System,
            None,
            ControllerReadiness::Disconnected,
            TunSupport::Supported
        ),
        Some(ProxyModeBlock::ControllerNotConnected)
    );
    assert_eq!(
        proxy_mode_block(
            ProxyMode::System,
            None,
            ControllerReadiness::Connected,
            TunSupport::FixtureReadOnly
        ),
        None
    );
    assert_eq!(
        proxy_mode_block(
            ProxyMode::System,
            None,
            ControllerReadiness::Connected,
            TunSupport::KernelUnsupported
        ),
        None
    );
}

#[test]
fn a_switch_in_flight_blocks_every_mode() {
    assert_eq!(
        proxy_mode_block(
            ProxyMode::System,
            Some(ProxyMode::Tun),
            ControllerReadiness::Connected,
            TunSupport::Supported
        ),
        Some(ProxyModeBlock::Busy)
    );
    assert_eq!(
        proxy_mode_block(
            ProxyMode::Tun,
            Some(ProxyMode::System),
            ControllerReadiness::Connected,
            TunSupport::Supported
        ),
        Some(ProxyModeBlock::Busy)
    );
}

#[test]
fn source_reload_tun_restore_failure_forces_the_ui_mode_off() {
    let mut mode = ProxyMode::Tun;
    let apply = SourceRuntimeApply::from_result(Err(mihomo::LoadError::ProxyModeLost(
        "fixture restore failure".to_owned(),
    )));

    assert!(apply.reconcile_proxy_mode(&mut mode));
    assert_eq!(mode, ProxyMode::Off);
}

#[test]
fn successful_source_reload_keeps_the_active_tun_mode() {
    let mut mode = ProxyMode::Tun;
    let apply = SourceRuntimeApply::from_result(Ok(mihomo::GeneratedProfileApply::Restarted));

    assert!(!apply.reconcile_proxy_mode(&mut mode));
    assert_eq!(mode, ProxyMode::Tun);
}

#[test]
fn the_status_line_names_the_kernel_that_is_actually_running() {
    let connected = crate::mihomo::ControllerState::Connected {
        endpoint: "http://127.0.0.1:9090".to_owned(),
        version: "1.13.19".to_owned(),
        active_connections: 5,
        download_total: 0,
        upload_total: 0,
    };

    // Hard-coding "Mihomo" here would mislabel every sing-box session.
    assert_eq!(
        super::controller_status_label(
            &connected,
            "sing-box",
            crate::localization::Language::SimplifiedChinese
        ),
        "sing-box 1.13.19 · 5 条活动连接"
    );
    assert_eq!(
        super::controller_status_label(
            &connected,
            "Mihomo",
            crate::localization::Language::English
        ),
        "Mihomo 1.13.19 · 5 active connections"
    );
    assert_eq!(
        super::controller_status_label(
            &crate::mihomo::ControllerState::Disconnected,
            "sing-box",
            crate::localization::Language::SimplifiedChinese
        ),
        "sing-box 未连接"
    );
    // The failure reason must survive; the status bar is now its only home.
    assert_eq!(
        super::controller_status_label(
            &crate::mihomo::ControllerState::Failed {
                endpoint: "http://127.0.0.1:9090".to_owned(),
                message: "connection refused".to_owned(),
            },
            "Mihomo",
            crate::localization::Language::SimplifiedChinese
        ),
        "Mihomo 连接失败 · connection refused"
    );
}
