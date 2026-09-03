/// A proxy core supported by Manis's kernel-neutral configuration boundary.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum KernelKind {
    /// `MetaCubeX` Mihomo, kept as the compatibility-first default.
    #[default]
    Mihomo,
}

impl KernelKind {
    /// Returns the stable value written to user-owned configuration files.
    #[must_use]
    pub const fn persistence_key(self) -> &'static str {
        match self {
            Self::Mihomo => "mihomo",
        }
    }

    /// Parses a stable persisted value without guessing aliases.
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "mihomo" => Some(Self::Mihomo),
            _ => None,
        }
    }

    /// Returns the product name shown in the UI.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Mihomo => "Mihomo",
        }
    }

    /// Returns only capabilities Manis can preserve without changing semantics.
    #[must_use]
    pub const fn capabilities(self) -> KernelCapabilities {
        match self {
            Self::Mihomo => KernelCapabilities::MIHOMO,
        }
    }
}

/// Features that are both native to a kernel and implemented by Manis's adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelCapabilities {
    bits: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KernelCapability {
    SubscriptionProviders,
    ManualVless,
    Selector,
    UrlTest,
    Fallback,
    LoadBalance,
    ClashApi,
    Tun,
}

impl KernelCapability {
    const fn bit(self) -> u8 {
        match self {
            Self::SubscriptionProviders => 1 << 0,
            Self::ManualVless => 1 << 1,
            Self::Selector => 1 << 2,
            Self::UrlTest => 1 << 3,
            Self::Fallback => 1 << 4,
            Self::LoadBalance => 1 << 5,
            Self::ClashApi => 1 << 6,
            Self::Tun => 1 << 7,
        }
    }
}

impl KernelCapabilities {
    const MIHOMO: Self = Self { bits: u8::MAX };
    #[must_use]
    pub const fn supports(self, capability: KernelCapability) -> bool {
        self.bits & capability.bit() != 0
    }
}
