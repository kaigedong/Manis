use manis_core::{KernelCapability, KernelKind};

#[test]
fn kernel_identity_is_stable_for_persistence_and_display() {
    assert_eq!(KernelKind::Mihomo.persistence_key(), "mihomo");
    assert_eq!(KernelKind::parse("mihomo"), Some(KernelKind::Mihomo));
    assert_eq!(KernelKind::parse("unknown"), None);
    assert_eq!(KernelKind::Mihomo.display_name(), "Mihomo");
}

#[test]
fn capability_matrix_does_not_promise_silent_policy_translation() {
    let mihomo = KernelKind::Mihomo.capabilities();
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
}
