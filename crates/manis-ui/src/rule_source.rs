use std::error::Error;
use std::fmt;
use std::time::Duration;

use manis_profile::SecretUrl;
use ureq::{Agent, ResponseExt as _};

pub(crate) const MAX_RULE_DOCUMENT_BYTES: u64 = 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: u32 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuleDownloadError {
    InvalidHttpsUrl,
    NetworkUnavailable,
    RequestRejected,
    InsecureRedirect,
    DocumentTooLarge,
    InvalidText,
}

impl fmt::Display for RuleDownloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidHttpsUrl => "请输入完整的 HTTPS 规则地址",
            Self::NetworkUnavailable => "规则下载失败，请检查网络后重试",
            Self::RequestRejected => "规则源拒绝了请求或返回了异常状态",
            Self::InsecureRedirect => "规则地址跳转到了非 HTTPS 页面，已停止导入",
            Self::DocumentTooLarge => "规则文件超过 1 MiB，未执行导入",
            Self::InvalidText => "规则文件不是有效的 UTF-8 文本",
        })
    }
}

impl Error for RuleDownloadError {}

pub(crate) fn download_qx_rule_document(input: &str) -> Result<String, RuleDownloadError> {
    SecretUrl::parse_https(input).map_err(|_error| RuleDownloadError::InvalidHttpsUrl)?;
    let config = Agent::config_builder()
        .https_only(true)
        .max_redirects(MAX_REDIRECTS)
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .user_agent("Manis/0.1 QX-Rule-Importer")
        .build();
    let agent: Agent = config.into();
    let mut response = agent
        .get(input)
        .call()
        .map_err(|error| map_request_error(&error))?;
    if response.get_uri().scheme_str() != Some("https") {
        return Err(RuleDownloadError::InsecureRedirect);
    }
    response
        .body_mut()
        .with_config()
        .limit(MAX_RULE_DOCUMENT_BYTES + 1)
        .lossy_utf8(false)
        .read_to_string()
        .map_err(|error| map_body_error(&error))
        .and_then(|content| {
            if content.len() as u64 > MAX_RULE_DOCUMENT_BYTES {
                Err(RuleDownloadError::DocumentTooLarge)
            } else {
                Ok(content)
            }
        })
}

pub(crate) fn download_qx_rule_document_secret(
    source: &SecretUrl,
) -> Result<String, RuleDownloadError> {
    source.expose_to(download_qx_rule_document)
}

fn map_request_error(error: &ureq::Error) -> RuleDownloadError {
    match error {
        ureq::Error::StatusCode(_) => RuleDownloadError::RequestRejected,
        ureq::Error::RequireHttpsOnly(_) => RuleDownloadError::InsecureRedirect,
        _ => RuleDownloadError::NetworkUnavailable,
    }
}

fn map_body_error(error: &ureq::Error) -> RuleDownloadError {
    match error {
        ureq::Error::BodyExceedsLimit(_) => RuleDownloadError::DocumentTooLarge,
        _ => RuleDownloadError::InvalidText,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use manis_profile::{QxRuleList, SecretUrl};

    use super::{RuleDownloadError, download_qx_rule_document, download_qx_rule_document_secret};
    use crate::mihomo;

    #[test]
    fn downloader_rejects_non_https_sources_before_network_access() {
        assert_eq!(
            download_qx_rule_document("http://example.com/rules.list"),
            Err(RuleDownloadError::InvalidHttpsUrl)
        );
    }

    #[test]
    fn downloader_diagnostics_never_expose_the_input_url() {
        let source = "https://?token=top-secret";
        let error = download_qx_rule_document(source).expect_err("missing authority must fail");
        assert!(!error.to_string().contains("top-secret"));
        assert!(!format!("{error:?}").contains("top-secret"));
    }

    #[test]
    fn stored_secret_downloader_keeps_the_url_inside_the_network_boundary() {
        let source = SecretUrl::parse_https("https://127.0.0.1:1/rules?token=top-secret")
            .expect("fixture URL passes the local HTTPS parser");

        let error = download_qx_rule_document_secret(&source)
            .expect_err("closed loopback port must fail without external network access");

        assert!(!error.to_string().contains("top-secret"));
        assert!(!format!("{error:?}").contains("top-secret"));
    }

    #[test]
    #[ignore = "requires public network access"]
    fn live_qx_rule_list_downloads_over_https() {
        let url = "https://raw.githubusercontent.com/limbopro/Profiles4limbo/main/airports.list";
        let document = download_qx_rule_document(url).expect("download public QX rule fixture");
        let parsed = QxRuleList::parse(&document);
        assert!(parsed.rules.len() >= 100);

        let root = std::env::temp_dir().join(format!("manis-live-qx-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale live QX fixture");
        }
        let store = root.join("subscriptions");
        let saved = mihomo::save_qx_rule_source_in(&store, url, "Proxy", &document)
            .expect("persist downloaded QX rule fixture");
        assert_eq!(saved.rule_count, parsed.rules.len());
        assert_eq!(mihomo::load_qx_rule_sources_in(&store).unwrap().len(), 1);
        fs::remove_dir_all(root).expect("remove live QX fixture");
    }
}
