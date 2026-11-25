//! Deterministic two-letter region codes for node names.
//!
//! The design system replaces flag emoji with a crisp 2-letter ISO square. Node names come from
//! subscriptions and the kernel, so the mapping must never depend on rendering state: the same
//! name always earns the same code, and an unknown name falls back to its first two ASCII letters
//! before the neutral `--` placeholder.

/// Recognised regions, in a fixed priority order for equal match positions.
///
/// Two-letter entries are matched as whole tokens so `AUSTRALIA` never reads as `US`; longer
/// entries match as substrings of the upper-cased name.
const REGIONS: &[(&str, &[&str])] = &[
    ("HK", &["香港", "HONG KONG", "HK"]),
    ("SG", &["新加坡", "SINGAPORE", "SG"]),
    ("JP", &["日本", "JAPAN", "JP"]),
    ("US", &["美国", "UNITED STATES", "USA", "US"]),
    ("TW", &["台湾", "台灣", "TAIWAN", "TW"]),
    ("KR", &["韩国", "韓國", "SOUTH KOREA", "KOREA", "KR"]),
    (
        "GB",
        &[
            "英国",
            "英國",
            "UNITED KINGDOM",
            "GREAT BRITAIN",
            "GB",
            "UK",
        ],
    ),
    ("DE", &["德国", "德國", "GERMANY", "DE"]),
    ("NL", &["荷兰", "荷蘭", "NETHERLANDS", "NL"]),
    ("CA", &["加拿大", "CANADA", "CA"]),
    ("AU", &["澳大利亚", "澳洲", "AUSTRALIA", "AU"]),
];

/// The 2-letter region square for a node name.
pub(in crate::app) fn region_code(name: &str) -> String {
    let upper = name.to_uppercase();
    let tokens = ascii_tokens(&upper);
    let mut best: Option<(usize, &'static str)> = None;
    for (code, needles) in REGIONS {
        let index = needles
            .iter()
            .filter_map(|needle| needle_index(&upper, &tokens, needle))
            .min();
        if let Some(index) = index
            && best.is_none_or(|(current, _)| index < current)
        {
            best = Some((index, code));
        }
    }
    if let Some((_, code)) = best {
        return (*code).to_owned();
    }
    let mut letters: String = name
        .chars()
        .filter(char::is_ascii_alphabetic)
        .take(2)
        .collect();
    letters.make_ascii_uppercase();
    if letters.len() == 2 {
        letters
    } else {
        "--".to_owned()
    }
}

/// Byte offset of the first token equal to `needle`, or of the first substring match when the
/// needle is not a plain two-letter code.
fn needle_index(upper: &str, tokens: &[(usize, &str)], needle: &str) -> Option<usize> {
    let bytes = needle.as_bytes();
    if bytes.len() == 2 && bytes.iter().all(u8::is_ascii_alphabetic) {
        return tokens
            .iter()
            .find(|(_, token)| token.eq_ignore_ascii_case(needle))
            .map(|(index, _)| *index);
    }
    upper.find(needle)
}

/// ASCII alphanumeric runs of `value` with their byte offsets, so codes match whole words.
fn ascii_tokens(value: &str) -> Vec<(usize, &str)> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, character) in value.char_indices() {
        if character.is_ascii_alphanumeric() {
            start.get_or_insert(index);
        } else if let Some(start) = start.take() {
            tokens.push((start, &value[start..index]));
        }
    }
    if let Some(start) = start {
        tokens.push((start, &value[start..]));
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::region_code;

    #[test]
    fn region_codes_recognise_names_codes_and_chinese_labels() {
        for (name, code) in [
            ("HK 03", "HK"),
            ("香港 01", "HK"),
            ("Hong Kong 01", "HK"),
            ("SG-02", "SG"),
            ("新加坡 01", "SG"),
            ("Singapore 02", "SG"),
            ("Tokyo JP 01", "JP"),
            ("日本节点", "JP"),
            ("US West", "US"),
            ("美国 05", "US"),
            ("台湾 01", "TW"),
            ("韩国 01", "KR"),
            ("英国 01", "GB"),
            ("德国 01", "DE"),
            ("荷兰 01", "NL"),
            ("加拿大 01", "CA"),
            ("澳大利亚 01", "AU"),
            ("Australia 01", "AU"),
        ] {
            assert_eq!(region_code(name), code, "{name}");
        }
    }

    #[test]
    fn unmatched_names_fall_back_to_two_ascii_letters_then_the_neutral_square() {
        assert_eq!(region_code("edge-01"), "ED");
        assert_eq!(region_code("東京 01"), "--");
        assert_eq!(region_code("01"), "--");
        assert_eq!(region_code(""), "--");
    }
}
