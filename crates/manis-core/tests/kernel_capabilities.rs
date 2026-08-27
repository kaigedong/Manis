use manis_core::{KernelCapabilities, KernelKind};

#[test]
fn kernel_identity_is_stable_for_persistence_and_display() {
    assert_eq!(KernelKind::Mihomo.persistence_key(), "mihomo");
    assert_eq!(KernelKind::SingBox.persistence_key(), "sing-box");
    assert_eq!(KernelKind::parse("mihomo"), Some(KernelKind::Mihomo));
    assert_eq!(KernelKind::parse("sing-box"), Some(KernelKind::SingBox));
    assert_eq!(KernelKind::parse("unknown"), None);
    assert_eq!(KernelKind::Mihomo.display_name(), "Mihomo");
    assert_eq!(KernelKind::SingBox.display_name(), "sing-box");
}

#[test]
fn capability_matrix_does_not_promise_silent_policy_translation() {
    assert_eq!(
        KernelKind::Mihomo.capabilities(),
        KernelCapabilities {
            subscription_providers: true,
            manual_vless: true,
            selector: true,
            url_test: true,
            fallback: true,
            load_balance: true,
            clash_api: true,
            tun: true,
        }
    );
    assert_eq!(
        KernelKind::SingBox.capabilities(),
        KernelCapabilities {
            subscription_providers: false,
            manual_vless: true,
            selector: true,
            url_test: true,
            fallback: false,
            load_balance: false,
            clash_api: true,
            tun: false,
        }
    );
}
