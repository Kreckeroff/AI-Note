use unicode_normalization::UnicodeNormalization;

pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|ch| {
        matches!(ch,
        '\t'..='\r' | ' ' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
        | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
    })
}

pub(crate) fn fold_name(value: &str) -> String {
    value
        .nfc()
        .collect::<String>()
        .to_lowercase()
        .nfc()
        .collect()
}
