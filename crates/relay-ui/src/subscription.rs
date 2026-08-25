use std::fmt;

use relay_profile::{Profile, SecretUrl};

pub(crate) const MAX_SUBSCRIPTION_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SubscriptionPreview {
    pub providers: usize,
    pub groups: usize,
    pub rules: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubscriptionInputError {
    Empty,
    HttpsRequired,
    TooLong,
    InvalidPreset,
}

impl fmt::Display for SubscriptionInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "请输入订阅链接",
            Self::HttpsRequired => "链接无效，请输入完整的 HTTPS 订阅地址",
            Self::TooLong => "链接过长，请确认复制的是订阅地址",
            Self::InvalidPreset => "链接有效，但无法生成默认策略预览",
        })
    }
}

pub(crate) fn validate_subscription_preview(
    input: &str,
) -> Result<SubscriptionPreview, SubscriptionInputError> {
    if input.is_empty() {
        return Err(SubscriptionInputError::Empty);
    }
    if input.len() > MAX_SUBSCRIPTION_BYTES {
        return Err(SubscriptionInputError::TooLong);
    }
    let subscription =
        SecretUrl::parse_https(input).map_err(|_error| SubscriptionInputError::HttpsRequired)?;
    let profile = Profile::qx_default(subscription)
        .map_err(|_error| SubscriptionInputError::InvalidPreset)?;
    Ok(SubscriptionPreview {
        providers: profile.providers.len(),
        groups: profile.groups.len(),
        rules: profile.rules.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::{SubscriptionInputError, validate_subscription_preview};

    #[test]
    fn valid_https_subscription_builds_a_safe_qx_style_preview() {
        let preview = validate_subscription_preview(
            "https://subscription.example.invalid/client?token=fixture-secret",
        )
        .expect("fixture HTTPS subscription should validate");

        assert_eq!(preview.providers, 1);
        assert_eq!(preview.groups, 2);
        assert_eq!(preview.rules, 2);
    }

    #[test]
    fn invalid_subscription_error_never_contains_the_input() {
        let input = "http://subscription.example.invalid/private-token";
        let error = validate_subscription_preview(input).expect_err("HTTP must be rejected");

        assert_eq!(error, SubscriptionInputError::HttpsRequired);
        assert!(!format!("{error:?}").contains(input));
        assert!(!error.to_string().contains("private-token"));
    }
}
