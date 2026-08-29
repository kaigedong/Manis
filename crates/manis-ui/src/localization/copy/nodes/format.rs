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
    match (seconds, language) {
        (60, Language::English) => "1 min".to_owned(),
        (60, Language::SimplifiedChinese) => "1 分钟".to_owned(),
        (300, Language::English) => "5 min".to_owned(),
        (300, Language::SimplifiedChinese) => "5 分钟".to_owned(),
        (600, Language::English) => "10 min".to_owned(),
        (600, Language::SimplifiedChinese) => "10 分钟".to_owned(),
        (1_800, Language::English) => "30 min".to_owned(),
        (1_800, Language::SimplifiedChinese) => "30 分钟".to_owned(),
        (seconds, Language::English) => format!("{seconds} sec"),
        (seconds, Language::SimplifiedChinese) => format!("{seconds} 秒"),
    }
}

pub(crate) fn candidate_selection_title(language: Language, selected: usize) -> String {
    match language {
        Language::English => format!("Select candidates · {selected} selected"),
        Language::SimplifiedChinese => format!("选择候选项 · 已选 {selected} 项"),
    }
}
