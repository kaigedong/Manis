use manis_core::{KernelCapability, KernelKind};

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
    let mihomo = KernelKind::Mihomo.capabilities();
    let sing_box = KernelKind::SingBox.capabilities();
    for capability in [
        KernelCapability::SubscriptionProviders,
        KernelCapability::ManualVless,
        KernelCapability::Selector,
        KernelCapability::UrlTest,
        KernelCapability::Fallback,
        KernelCapability::LoadBalance,
        KernelCapability::ClashApi,
        KernelCapability::Tun,
    ] {
        assert!(mihomo.supports(capability));
    }
    for capability in [
        KernelCapability::ManualVless,
        KernelCapability::Selector,
        KernelCapability::UrlTest,
        KernelCapability::ClashApi,
    ] {
        assert!(sing_box.supports(capability));
    }
    for capability in [
        KernelCapability::SubscriptionProviders,
        KernelCapability::Fallback,
        KernelCapability::LoadBalance,
        KernelCapability::Tun,
    ] {
        assert!(!sing_box.supports(capability));
    }
}
