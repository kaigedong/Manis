use crate::{localization::Language, mihomo::LiveStreamPhase};

pub(crate) fn live_stream_phase(language: Language, phase: &LiveStreamPhase) -> String {
    match (language, phase) {
        (Language::English, LiveStreamPhase::Waiting) => "Waiting to connect".to_owned(),
        (Language::English, LiveStreamPhase::Connecting) => "Establishing live stream".to_owned(),
        (Language::English, LiveStreamPhase::Live) => "Live".to_owned(),
        (Language::English, LiveStreamPhase::Unavailable) => "Live status unavailable".to_owned(),
        (Language::English, LiveStreamPhase::Reconnecting(attempt)) => {
            format!("Reconnecting · attempt {attempt}")
        }
        (Language::English, LiveStreamPhase::InterruptedHttp(status)) => {
            format!("Stream interrupted · HTTP {status}")
        }
        (Language::English, LiveStreamPhase::InvalidData) => {
            "Stream data could not be parsed · retrying".to_owned()
        }
        (Language::English, LiveStreamPhase::ControllerUnavailable) => {
            "Controller temporarily unavailable · retrying".to_owned()
        }
        (Language::English, LiveStreamPhase::Retrying) => {
            "Live stream unavailable · retrying".to_owned()
        }
        (Language::English, LiveStreamPhase::StartFailed(error)) => {
            format!("Could not start: {error}")
        }
        (Language::SimplifiedChinese, LiveStreamPhase::Waiting) => "等待连接".to_owned(),
        (Language::SimplifiedChinese, LiveStreamPhase::Connecting) => "正在建立实时流".to_owned(),
        (Language::SimplifiedChinese, LiveStreamPhase::Live) => "实时".to_owned(),
        (Language::SimplifiedChinese, LiveStreamPhase::Unavailable) => "实时状态不可用".to_owned(),
        (Language::SimplifiedChinese, LiveStreamPhase::Reconnecting(attempt)) => {
            format!("正在重连 · 第 {attempt} 次")
        }
        (Language::SimplifiedChinese, LiveStreamPhase::InterruptedHttp(status)) => {
            format!("流中断 · HTTP {status}")
        }
        (Language::SimplifiedChinese, LiveStreamPhase::InvalidData) => {
            "流数据无法解析 · 正在重试".to_owned()
        }
        (Language::SimplifiedChinese, LiveStreamPhase::ControllerUnavailable) => {
            "控制器暂时不可达 · 正在重试".to_owned()
        }
        (Language::SimplifiedChinese, LiveStreamPhase::Retrying) => {
            "实时流不可用 · 正在重试".to_owned()
        }
        (Language::SimplifiedChinese, LiveStreamPhase::StartFailed(error)) => {
            format!("无法启动：{error}")
        }
    }
}

pub(crate) fn tun_mode_rollback_failed(language: Language, error: &str, rollback: &str) -> String {
    match language {
        Language::English => {
            format!("{error}; restoring the previous TUN mode also failed: {rollback}")
        }
        Language::SimplifiedChinese => {
            format!("{error}；恢复原 TUN 模式也失败：{rollback}")
        }
    }
}

pub(crate) fn system_proxy_rollback_failed(
    language: Language,
    error: &str,
    rollback: &str,
) -> String {
    match language {
        Language::English => {
            format!("{error}; restoring the previous system proxy also failed: {rollback}")
        }
        Language::SimplifiedChinese => {
            format!("{error}；恢复原系统代理也失败：{rollback}")
        }
    }
}

pub(crate) fn dns_rollback_failed(language: Language, error: &str, rollback: &str) -> String {
    match language {
        Language::English => {
            format!("{error}; restoring the previous DNS settings also failed: {rollback}")
        }
        Language::SimplifiedChinese => {
            format!("{error}；恢复原 DNS 设置也失败：{rollback}")
        }
    }
}

pub(crate) fn dns_reactivation_failed(language: Language, error: &str, rollback: &str) -> String {
    match language {
        Language::English => {
            format!("{error}; reactivating TUN DNS also failed: {rollback}")
        }
        Language::SimplifiedChinese => {
            format!("{error}；重新启用 TUN DNS 也失败：{rollback}")
        }
    }
}

pub(crate) fn tun_shutdown_rollback_failed(
    language: Language,
    error: &str,
    rollback: &str,
) -> String {
    match language {
        Language::English => {
            format!("{error}; stopping the newly started TUN also failed: {rollback}")
        }
        Language::SimplifiedChinese => {
            format!("{error}；关闭已启动的 TUN 也失败：{rollback}")
        }
    }
}

pub(crate) fn dns_and_tun_rollback_failed(
    language: Language,
    error: &str,
    dns_rollback: &str,
    tun_rollback: &str,
) -> String {
    match language {
        Language::English => format!(
            "{error}; restoring the previous DNS settings failed: {dns_rollback}; stopping the newly started TUN failed: {tun_rollback}"
        ),
        Language::SimplifiedChinese => format!(
            "{error}；恢复原 DNS 设置失败：{dns_rollback}；关闭已启动的 TUN 失败：{tun_rollback}"
        ),
    }
}

pub(crate) fn subscription_imported(
    language: Language,
    groups: usize,
    providers: usize,
    nodes: usize,
    suffix: &str,
) -> String {
    match language {
        Language::English => format!(
            "Subscription imported · {groups} groups · {providers} sources · {nodes} nodes{suffix}"
        ),
        Language::SimplifiedChinese => format!(
            "订阅已导入 · 共 {groups} 个订阅组 · {providers} 个来源 · {nodes} 个节点{suffix}"
        ),
    }
}

pub(crate) fn subscription_updated(language: Language, nodes: usize, suffix: &str) -> String {
    match language {
        Language::English => format!("Subscription updated · {nodes} nodes{suffix}"),
        Language::SimplifiedChinese => format!("订阅更新完成 · {nodes} 个节点{suffix}"),
    }
}

pub(crate) fn policy_benchmark_complete(
    language: Language,
    automatic: bool,
    current: Option<&str>,
    succeeded: usize,
    total: usize,
) -> String {
    match (language, automatic, current) {
        (Language::English, true, Some(current)) => format!(
            "Policy benchmark complete: {succeeded}/{total} succeeded · current optimum {current}"
        ),
        (Language::English, true, None) => format!(
            "Policy benchmark complete: {succeeded}/{total} succeeded · no single fixed exit"
        ),
        (Language::English, false, _) => {
            format!("Policy benchmark complete: {succeeded}/{total} candidates succeeded")
        }
        (Language::SimplifiedChinese, true, Some(current)) => {
            format!("策略组测速完成：{succeeded}/{total} 成功 · 当前优选 {current}")
        }
        (Language::SimplifiedChinese, true, None) => {
            format!("策略组测速完成：{succeeded}/{total} 成功 · 该策略没有单一固定出口")
        }
        (Language::SimplifiedChinese, false, _) => {
            format!("策略组测速完成：{succeeded}/{total} 个候选项成功")
        }
    }
}

pub(crate) fn benchmark_progress(language: Language, returned: usize, total: usize) -> String {
    match language {
        Language::English => format!("Testing latency · {returned} of {total} candidates returned"),
        Language::SimplifiedChinese => format!("正在测速 · {returned}/{total} 个候选项已返回"),
    }
}

pub(crate) fn benchmark_complete(
    language: Language,
    succeeded: usize,
    total: usize,
    minimum_ms: Option<u16>,
    average_ms: Option<u16>,
) -> String {
    match language {
        Language::English => {
            let latency = minimum_ms
                .zip(average_ms)
                .map_or_else(String::new, |(min, avg)| {
                    format!(" · min {min} ms · avg {avg} ms")
                });
            format!("Latency test complete · {succeeded}/{total} candidates succeeded{latency}")
        }
        Language::SimplifiedChinese => {
            let latency = minimum_ms
                .zip(average_ms)
                .map_or_else(String::new, |(min, avg)| {
                    format!(" · 最低 {min} ms · 平均 {avg} ms")
                });
            format!("测速完成 · {succeeded}/{total} 个候选项成功{latency}")
        }
    }
}

pub(crate) fn mihomo_installed(language: Language, version: &str) -> String {
    match language {
        Language::English => format!("Mihomo {version} installed and verified"),
        Language::SimplifiedChinese => format!("Mihomo {version} 已安装并校验"),
    }
}

pub(crate) fn loading_kernel_data(language: Language, kernel: &str, endpoint: &str) -> String {
    match language {
        Language::English => format!("Loading {kernel} data from {endpoint}"),
        Language::SimplifiedChinese => format!("正在从 {endpoint} 读取 {kernel} 数据"),
    }
}

pub(crate) fn kernel_connection_failed(language: Language, kernel: &str, message: &str) -> String {
    match language {
        Language::English => format!("{kernel} connection failed: {message}"),
        Language::SimplifiedChinese => format!("{kernel} 连接失败：{message}"),
    }
}

pub(crate) fn policy_missing_from_kernel(language: Language, policy: &str) -> String {
    match language {
        Language::English => format!("Policy group “{policy}” is not present in the active kernel"),
        Language::SimplifiedChinese => format!("当前内核中没有策略组“{policy}”"),
    }
}

pub(crate) fn snapshot_loaded(language: Language, policies: usize, connections: usize) -> String {
    match language {
        Language::English => {
            format!("Loaded {policies} policy groups · {connections} active connections")
        }
        Language::SimplifiedChinese => {
            format!("已读取 {policies} 个策略组 · {connections} 条活动连接")
        }
    }
}

pub(crate) fn selections_restored(language: Language, applied: usize, failed: usize) -> String {
    match language {
        Language::English => {
            format!("Restored {applied} saved node selections · {failed} could not be applied")
        }
        Language::SimplifiedChinese => {
            format!("已恢复 {applied} 个节点选择 · {failed} 个暂时无法应用")
        }
    }
}

pub(crate) fn testing_policy_candidates(language: Language, policy: &str, count: usize) -> String {
    match language {
        Language::English => format!("Testing {count} candidates in policy group “{policy}”"),
        Language::SimplifiedChinese => format!("正在测试策略组“{policy}”的 {count} 个候选项"),
    }
}

pub(crate) fn global_mode_current_exit(language: Language, target: &str) -> String {
    match language {
        Language::English => format!("Global mode enabled · current exit {target}"),
        Language::SimplifiedChinese => format!("全局模式已生效 · 当前出口 {target}"),
    }
}

pub(crate) fn saved_global_exit(language: Language, target: &str, active: bool) -> String {
    match (language, active) {
        (Language::English, true) => format!("Global exit switched to “{target}”"),
        (Language::English, false) => {
            format!("Saved global exit “{target}”; it applies in Global mode")
        }
        (Language::SimplifiedChinese, true) => format!("全局出口已切换到“{target}”"),
        (Language::SimplifiedChinese, false) => {
            format!("已保存全局出口“{target}”；切换到全局模式后生效")
        }
    }
}

pub(crate) fn selecting_global_node(language: Language, target: &str) -> String {
    match language {
        Language::English => format!("Selecting global node “{target}”…"),
        Language::SimplifiedChinese => format!("正在选择全局节点“{target}”…"),
    }
}

pub(crate) fn global_exit_apply_failed(language: Language, target: &str, error: &str) -> String {
    match language {
        Language::English => {
            format!("Saved global exit “{target}”, but it could not be applied now: {error}")
        }
        Language::SimplifiedChinese => format!("已保存全局出口“{target}”，但暂时无法应用：{error}"),
    }
}

pub(crate) fn deferred_policy_selection(language: Language, group: &str, node: &str) -> String {
    match language {
        Language::English => format!(
            "Saved “{node}” for manual policy “{group}”; it will apply when the managed kernel connects"
        ),
        Language::SimplifiedChinese => {
            format!("已为手动策略组“{group}”选择“{node}”；托管内核连接后生效")
        }
    }
}

pub(crate) fn setting_policy_node(language: Language, group: &str, node: &str) -> String {
    match language {
        Language::English => format!("Setting “{group}” to “{node}”…"),
        Language::SimplifiedChinese => format!("正在将“{group}”设为“{node}”…"),
    }
}

pub(crate) fn policy_selection_applied(language: Language, group: &str, node: &str) -> String {
    match language {
        Language::English => format!("“{group}” now uses “{node}” when a rule selects this policy"),
        Language::SimplifiedChinese => format!("规则命中“{group}”时将使用“{node}”"),
    }
}

pub(crate) fn policy_selection_apply_failed(
    language: Language,
    group: &str,
    node: &str,
    error: &str,
) -> String {
    match language {
        Language::English => {
            format!("Saved “{node}” for “{group}”, but it could not be applied now: {error}")
        }
        Language::SimplifiedChinese => {
            format!("已为“{group}”保存“{node}”，但暂时无法应用：{error}")
        }
    }
}

pub(crate) fn policy_group_action(language: Language, group: &str, action: &str) -> String {
    match language {
        Language::English => format!("Policy group “{group}” {action}"),
        Language::SimplifiedChinese => format!("策略组“{group}”已{action}"),
    }
}

pub(crate) fn policy_identity(language: Language, kind: &str, target: &str) -> String {
    match language {
        Language::English => format!("{kind} · current {target}"),
        Language::SimplifiedChinese => format!("{kind} · 当前 {target}"),
    }
}
