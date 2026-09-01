use super::{
    Div, Language, ManisApp, ParentElement, QxRuleImportFeedback, Styled, TextRole, Theme, copy,
    div,
};

impl ManisApp {
    pub(in crate::app) fn qx_rule_import_feedback(&self, theme: Theme, language: Language) -> Div {
        let (message, color) = match &self.rule_sources.feedback {
        QxRuleImportFeedback::Idle => (
            language
                .localized(copy::configuration::HTTPS_ONLY_UP_TO_1_MIB_INVALID_LINES_ARE_COUNTED)
                .to_owned(),
            theme.text_secondary,
        ),
        QxRuleImportFeedback::Importing => (
            language
                .localized(copy::configuration::SECURELY_DOWNLOADING_PARSING_AND_WRITING_LOCALLY)
                .to_owned(),
            theme.action_primary,
        ),
        QxRuleImportFeedback::Imported {
            rule_count,
            diagnostic_count,
        } => (
            copy::configuration::imported_rules(
                language,
                *rule_count,
                *diagnostic_count,
            ),
            theme.status_success,
        ),
        QxRuleImportFeedback::AlreadyExists {
            rule_count,
            target_policy,
            ..
        } => (
            copy::configuration::duplicate_rule_source(
                language,
                *rule_count,
                target_policy,
            ),
            theme.status_warning,
        ),
        QxRuleImportFeedback::InvalidDocument => (
            language
                .localized(copy::configuration::FILE_DOWNLOADED_BUT_NO_RECOGNIZABLE_QX_DOMAIN_RULES_WERE_FOUND)
                .to_owned(),
            theme.status_error,
        ),
        QxRuleImportFeedback::DownloadFailed(error) => (
            copy::configuration::rule_download_error(language, *error).to_owned(),
            theme.status_error,
        ),
        QxRuleImportFeedback::StoreFailed(error) => (
            copy::configuration::subscription_store_error(language, *error).to_owned(),
            theme.status_error,
        ),
    };
        div()
            .mt_2()
            .text_size(TextRole::Body.size())
            .line_height(TextRole::Body.line_height())
            .text_color(color)
            .child(message)
    }

    pub(in crate::app) fn qx_rule_targets(&self) -> Vec<String> {
        let mut targets = self
            .managed_policies
            .groups
            .iter()
            .map(|group| group.name.clone())
            .collect::<Vec<_>>();
        targets.push("DIRECT".to_owned());
        targets
    }

    pub(in crate::app) fn effective_rule_target(&self, target: &str, language: Language) -> String {
        if target != "Proxy"
            || self
                .managed_policies
                .groups
                .iter()
                .any(|group| group.name == target)
        {
            return target.to_owned();
        }
        self.managed_policies.groups.first().map_or_else(
            || {
                language
                    .localized(copy::configuration::GLOBAL_EXIT)
                    .to_owned()
            },
            |group| group.name.clone(),
        )
    }
}
