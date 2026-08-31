use crate::localization::Language;

pub(crate) fn success_fraction(language: Language, succeeded: usize, total: usize) -> String {
    match language {
        Language::English => format!("{succeeded}/{total} succeeded"),
        Language::SimplifiedChinese => format!("{succeeded}/{total} 成功"),
    }
}

pub(crate) fn group_limit(language: Language, count: usize) -> String {
    match language {
        Language::English => format!("group contains {count} nodes"),
        Language::SimplifiedChinese => format!("分组包含 {count} 个节点"),
    }
}

pub(crate) fn single_test_limit(language: Language, limit: usize) -> String {
    match language {
        Language::English => format!("a single test supports up to {limit}"),
        Language::SimplifiedChinese => format!("单次最多测试 {limit} 个"),
    }
}

pub(crate) fn selected_count(language: Language, count: usize) -> String {
    match language {
        Language::English => format!("{count} selected"),
        Language::SimplifiedChinese => format!("已选 {count} 项"),
    }
}

pub(crate) fn interval(language: Language, seconds: u32) -> String {
    let label = match seconds {
        60 => super::INTERVAL_1_MINUTE,
        300 => super::INTERVAL_5_MINUTES,
        600 => super::INTERVAL_10_MINUTES,
        1_800 => super::INTERVAL_30_MINUTES,
        _ => {
            return match language {
                Language::English => format!("{seconds} sec"),
                Language::SimplifiedChinese => format!("{seconds} 秒"),
            };
        }
    };
    language.localized(label).to_owned()
}

pub(crate) fn candidate_selection_title(language: Language, selected: usize) -> String {
    match language {
        Language::English => format!("Select candidates · {selected} selected"),
        Language::SimplifiedChinese => format!("选择候选项 · 已选 {selected} 项"),
    }
}
