use super::*;

pub(super) struct PreviewWorkspace {
    path: PathBuf,
}

impl PreviewWorkspace {
    pub(super) fn create() -> Result<Self, std::io::Error> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        #[cfg(unix)]
        let temp_root = PathBuf::from("/tmp");
        #[cfg(not(unix))]
        let temp_root = env::temp_dir();
        for _ in 0..16 {
            let sequence = NEXT_PREVIEW_WORKSPACE.fetch_add(1, Ordering::Relaxed);
            let path = temp_root.join(format!(
                "manis-p-{:x}-{nonce:x}-{sequence:x}",
                std::process::id()
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not reserve a unique preview workspace",
        ))
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PreviewWorkspace {
    fn drop(&mut self) {
        if let Ok(metadata) = fs::symlink_metadata(&self.path)
            && metadata.is_dir()
            && !metadata.file_type().is_symlink()
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

pub(crate) fn discover_subscription_proxy_nameservers(source: &SecretUrl) -> Vec<ProxyDnsServer> {
    if let Ok(document) = download_subscription_document(source) {
        let nameservers = extract_subscription_proxy_nameservers(&document);
        record_event(
            LogLevel::Info,
            "subscription.metadata.loaded",
            format!("proxy_dns_count={}", nameservers.len()),
        );
        nameservers
    } else {
        record_event(
            LogLevel::Warn,
            "subscription.metadata.unavailable",
            "proxy_dns_count=0; using safe defaults",
        );
        Vec::new()
    }
}

fn download_subscription_document(source: &SecretUrl) -> Result<String, ()> {
    let config = Agent::config_builder()
        .https_only(source.is_https())
        .max_redirects(SUBSCRIPTION_MAX_REDIRECTS)
        .timeout_global(Some(SUBSCRIPTION_DOWNLOAD_TIMEOUT))
        .user_agent("clash.meta")
        .build();
    let agent: Agent = config.into();
    let mut response = source
        .expose_to(|url| agent.get(url).call())
        .map_err(|_error| ())?;
    if source.is_https() && response.get_uri().scheme_str() != Some("https") {
        return Err(());
    }
    let document = response
        .body_mut()
        .with_config()
        .limit(MAX_SUBSCRIPTION_DOCUMENT_BYTES + 1)
        .lossy_utf8(false)
        .read_to_string()
        .map_err(|_error| ())?;
    if document.len() as u64 > MAX_SUBSCRIPTION_DOCUMENT_BYTES {
        return Err(());
    }
    Ok(document)
}

pub(super) fn extract_subscription_proxy_nameservers(document: &str) -> Vec<ProxyDnsServer> {
    let mut dns_indent = None;
    let mut list_indent = None;
    let mut nameservers = Vec::new();

    for line in document.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line
            .len()
            .saturating_sub(line.trim_start_matches(' ').len());
        let trimmed = line.trim();
        let Some(active_dns_indent) = dns_indent else {
            if indent == 0 && trimmed == "dns:" {
                dns_indent = Some(indent);
            }
            continue;
        };
        if indent <= active_dns_indent {
            break;
        }

        if let Some(active_list_indent) = list_indent {
            if indent <= active_list_indent {
                break;
            }
            let Some(value) = trimmed.strip_prefix('-') else {
                continue;
            };
            push_proxy_dns_scalar(value, &mut nameservers);
            if nameservers.len() == MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
                break;
            }
            continue;
        }

        let Some(value) = trimmed.strip_prefix("proxy-server-nameserver:") else {
            continue;
        };
        list_indent = Some(indent);
        let value = value.trim();
        if let Some(inline) = value
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            for scalar in split_inline_yaml_scalars(inline) {
                push_proxy_dns_scalar(scalar, &mut nameservers);
                if nameservers.len() == MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
                    break;
                }
            }
            break;
        }
    }
    nameservers
}

fn split_inline_yaml_scalars(value: &str) -> Vec<&str> {
    let mut values = Vec::new();
    let mut quote = None;
    let mut start = 0;
    for (index, character) in value.char_indices() {
        match (quote, character) {
            (None, '\'' | '"') => quote = Some(character),
            (Some(active), current) if active == current => quote = None,
            (None, ',') => {
                values.push(&value[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    values.push(&value[start..]);
    values
}

fn push_proxy_dns_scalar(value: &str, nameservers: &mut Vec<ProxyDnsServer>) {
    let value = value.trim();
    let value = if value.len() >= 2
        && ((value.starts_with('\'') && value.ends_with('\''))
            || (value.starts_with('"') && value.ends_with('"')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    };
    let Ok(nameserver) = ProxyDnsServer::parse_https(value) else {
        return;
    };
    if !nameservers.contains(&nameserver) {
        nameservers.push(nameserver);
    }
}

pub(crate) fn preview_subscription(
    input: &str,
) -> Result<Vec<LoadedProvider>, SubscriptionPreviewError> {
    let binary = discover_preview_binary()?;
    preview_subscription_with_binary(input, &binary)
}

pub(crate) fn preview_single_node(
    input: &str,
) -> Result<Vec<LoadedProvider>, SubscriptionStoreError> {
    #[cfg(not(unix))]
    {
        let _ = input;
        return Err(SubscriptionStoreError::StoreUnavailable);
    }
    #[cfg(unix)]
    {
        let binary =
            discover_preview_binary().map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
        let workspace = PreviewWorkspace::create()
            .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
        write_private_atomic(workspace.path(), "single-node.txt", input.as_bytes())
            .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
        let mixed_port =
            reserve_preview_port().map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
        let profile = Profile::qx_sources_with_groups_and_local_providers(
            Vec::new(),
            vec!["./single-node.txt".to_owned()],
            Vec::new(),
            Vec::new(),
            mixed_port,
        )
        .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
        let yaml =
            render_mihomo_yaml(&profile).map_err(|_error| SubscriptionStoreError::InvalidSource)?;
        let config_file = write_private_atomic(workspace.path(), "preview.yaml", yaml.as_bytes())
            .map_err(|_error| SubscriptionStoreError::StoreUnavailable)?;
        let controller = ControllerEndpoint::UnixSocket(workspace.path().join("controller.sock"));
        let config =
            ManagedEngineConfig::new(binary, config_file, workspace.path().to_owned(), controller);
        let mut manager = EngineManager::new(
            config,
            ReadinessPolicy::default(),
            Box::new(MihomoReadinessProbe),
        );
        let endpoint = manager
            .start()
            .map_err(|_error| SubscriptionStoreError::InvalidSource)?;
        let providers = wait_for_preview_providers(&endpoint)
            .map_err(|_error| SubscriptionStoreError::InvalidSource);
        let _ = manager.stop();
        providers
    }
}

pub(crate) fn preview_imported_subscription(
    subscription: SecretUrl,
) -> Result<Vec<LoadedProvider>, SubscriptionPreviewError> {
    let binary = discover_preview_binary()?;
    preview_secret_subscription_with_binary(subscription, &binary)
}

pub(super) fn preview_subscription_with_binary(
    input: &str,
    binary: &Path,
) -> Result<Vec<LoadedProvider>, SubscriptionPreviewError> {
    let subscription = SecretUrl::parse_subscription(input)
        .map_err(|_error| SubscriptionPreviewError::InvalidSource)?;
    preview_secret_subscription_with_binary(subscription, binary)
}

pub(super) fn preview_secret_subscription_with_binary(
    subscription: SecretUrl,
    binary: &Path,
) -> Result<Vec<LoadedProvider>, SubscriptionPreviewError> {
    #[cfg(not(unix))]
    {
        let _ = (subscription, binary);
        return Err(SubscriptionPreviewError::UnsupportedPlatform);
    }

    #[cfg(unix)]
    {
        let binary = canonical_binary(binary)?;
        let workspace = PreviewWorkspace::create()
            .map_err(|_error| SubscriptionPreviewError::WorkspaceUnavailable)?;
        let mixed_port = reserve_preview_port()?;
        let profile = Profile::subscription_preview(subscription, mixed_port)
            .map_err(|_error| SubscriptionPreviewError::ProfileUnavailable)?;
        let yaml = render_mihomo_yaml(&profile)
            .map_err(|_error| SubscriptionPreviewError::ProfileUnavailable)?;
        let config_file = write_private_atomic(workspace.path(), "preview.yaml", yaml.as_bytes())
            .map_err(|_error| SubscriptionPreviewError::WorkspaceUnavailable)?;
        let controller = ControllerEndpoint::UnixSocket(workspace.path().join("controller.sock"));
        let config =
            ManagedEngineConfig::new(binary, config_file, workspace.path().to_owned(), controller);
        let mut manager = EngineManager::new(
            config,
            ReadinessPolicy::default(),
            Box::new(MihomoReadinessProbe),
        );
        let endpoint = manager
            .start()
            .map_err(|_error| SubscriptionPreviewError::EngineUnavailable)?;
        let providers = wait_for_preview_providers(&endpoint);
        manager
            .stop()
            .map_err(|_error| SubscriptionPreviewError::EngineUnavailable)?;
        providers
    }
}

fn discover_preview_binary() -> Result<PathBuf, SubscriptionPreviewError> {
    #[cfg(debug_assertions)]
    if let Some(explicit) = brand::env_var_os(BINARY_ENV, LEGACY_RELAY_BINARY_ENV) {
        return canonical_binary(Path::new(&explicit));
    }
    core_update::managed_core_binary_path()
        .map_err(|_error| SubscriptionPreviewError::BinaryUnavailable)
        .and_then(|path| canonical_binary(&path))
}

pub(super) fn canonical_binary(path: &Path) -> Result<PathBuf, SubscriptionPreviewError> {
    let canonical = path
        .canonicalize()
        .map_err(|_error| SubscriptionPreviewError::BinaryUnavailable)?;
    canonical
        .is_file()
        .then_some(canonical)
        .ok_or(SubscriptionPreviewError::BinaryUnavailable)
}

fn reserve_preview_port() -> Result<u16, SubscriptionPreviewError> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|_error| SubscriptionPreviewError::WorkspaceUnavailable)
}

#[cfg(unix)]
fn wait_for_preview_providers(
    endpoint: &ControllerEndpoint,
) -> Result<Vec<LoadedProvider>, SubscriptionPreviewError> {
    let ControllerEndpoint::UnixSocket(socket_path) = endpoint else {
        return Err(SubscriptionPreviewError::UnsupportedPlatform);
    };
    let client = MihomoClient::new(
        ControllerConfig::default(),
        UnixSocketTransport::new(socket_path),
    );
    for attempt in 0..PREVIEW_PROVIDER_ATTEMPTS {
        if let Ok(providers) = client.fetch_proxy_providers() {
            let providers = load_subscription_provider(&providers);
            if providers.iter().any(|provider| !provider.nodes.is_empty()) {
                return Ok(providers);
            }
        }
        if attempt + 1 < PREVIEW_PROVIDER_ATTEMPTS {
            thread::sleep(PREVIEW_PROVIDER_DELAY);
        }
    }
    match client.fetch_proxy_providers() {
        Ok(providers)
            if providers.iter().any(|provider| {
                provider.name == "subscription" && !provider.proxies.is_empty()
            }) =>
        {
            Ok(load_subscription_provider(&providers))
        }
        Ok(_) => Err(SubscriptionPreviewError::EmptyProvider),
        Err(_) => Err(SubscriptionPreviewError::ProviderUnavailable),
    }
}
