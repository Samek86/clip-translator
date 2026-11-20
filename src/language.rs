/// Languages both DeepL and Azure Translator can target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageId {
    Ko,
    Ja,
    En,
    ZhHans,
    ZhHant,
    Es,
    Fr,
    De,
    Pt,
    It,
    Ru,
    Nl,
    Pl,
    Indonesian,
    Tr,
    Ar,
}

impl LanguageId {
    pub const ALL: [LanguageId; 16] = [
        Self::Ko,
        Self::Ja,
        Self::En,
        Self::ZhHans,
        Self::ZhHant,
        Self::Es,
        Self::Fr,
        Self::De,
        Self::Pt,
        Self::It,
        Self::Ru,
        Self::Nl,
        Self::Pl,
        Self::Indonesian,
        Self::Tr,
        Self::Ar,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Ko => "ko",
            Self::Ja => "ja",
            Self::En => "en",
            Self::ZhHans => "zh-hans",
            Self::ZhHant => "zh-hant",
            Self::Es => "es",
            Self::Fr => "fr",
            Self::De => "de",
            Self::Pt => "pt",
            Self::It => "it",
            Self::Ru => "ru",
            Self::Nl => "nl",
            Self::Pl => "pl",
            Self::Indonesian => "id",
            Self::Tr => "tr",
            Self::Ar => "ar",
        }
    }

    /// English name shown in language lists and in "Japanese detected".
    pub fn endonym(self) -> &'static str {
        match self {
            Self::Ko => "Korean",
            Self::Ja => "Japanese",
            Self::En => "English",
            Self::ZhHans => "Chinese (Simplified)",
            Self::ZhHant => "Chinese (Traditional)",
            Self::Es => "Spanish",
            Self::Fr => "French",
            Self::De => "German",
            Self::Pt => "Portuguese",
            Self::It => "Italian",
            Self::Ru => "Russian",
            Self::Nl => "Dutch",
            Self::Pl => "Polish",
            Self::Indonesian => "Indonesian",
            Self::Tr => "Turkish",
            Self::Ar => "Arabic",
        }
    }

    pub fn deepl_target(self) -> &'static str {
        match self {
            Self::Ko => "KO",
            Self::Ja => "JA",
            Self::En => "EN",
            Self::ZhHans => "ZH-HANS",
            Self::ZhHant => "ZH-HANT",
            Self::Es => "ES",
            Self::Fr => "FR",
            Self::De => "DE",
            Self::Pt => "PT-BR",
            Self::It => "IT",
            Self::Ru => "RU",
            Self::Nl => "NL",
            Self::Pl => "PL",
            Self::Indonesian => "ID",
            Self::Tr => "TR",
            Self::Ar => "AR",
        }
    }

    pub fn azure_code(self) -> &'static str {
        match self {
            Self::Ko => "ko",
            Self::Ja => "ja",
            Self::En => "en",
            Self::ZhHans => "zh-Hans",
            Self::ZhHant => "zh-Hant",
            Self::Es => "es",
            Self::Fr => "fr",
            Self::De => "de",
            Self::Pt => "pt",
            Self::It => "it",
            Self::Ru => "ru",
            Self::Nl => "nl",
            Self::Pl => "pl",
            Self::Indonesian => "id",
            Self::Tr => "tr",
            Self::Ar => "ar",
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|lang| lang.id().eq_ignore_ascii_case(value.trim()))
    }

    pub fn from_endonym(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|lang| lang.endonym() == value)
    }

    pub fn from_index(index: i32) -> Option<Self> {
        Self::ALL.get(usize::try_from(index).ok()?).copied()
    }

    pub fn index(self) -> i32 {
        Self::ALL.iter().position(|lang| *lang == self).unwrap_or(0) as i32
    }
}

/// Map a provider detection code (`JA`, `zh-Hans`, `EN-GB`) onto a language.
pub fn from_detected_code(code: &str) -> Option<LanguageId> {
    let normalized = code.trim().replace('_', "-").to_ascii_uppercase();
    Some(match normalized.as_str() {
        "KO" | "KOR" => LanguageId::Ko,
        "JA" | "JP" => LanguageId::Ja,
        "EN" | "EN-US" | "EN-GB" => LanguageId::En,
        "ZH" | "ZH-HANS" | "ZH-CN" => LanguageId::ZhHans,
        "ZH-HANT" | "ZH-TW" => LanguageId::ZhHant,
        "ES" => LanguageId::Es,
        "FR" => LanguageId::Fr,
        "DE" => LanguageId::De,
        "PT" | "PT-BR" | "PT-PT" => LanguageId::Pt,
        "IT" => LanguageId::It,
        "RU" => LanguageId::Ru,
        "NL" => LanguageId::Nl,
        "PL" => LanguageId::Pl,
        "ID" => LanguageId::Indonesian,
        "TR" => LanguageId::Tr,
        "AR" => LanguageId::Ar,
        _ => return None,
    })
}

/// Cheap script check so a clearly Korean or Japanese clipboard does not need
/// a second request. Pure kanji and Latin text stay unknown.
pub fn guess_language(text: &str) -> Option<LanguageId> {
    let mut hangul = 0usize;
    let mut kana = 0usize;
    for ch in text.chars() {
        let code = ch as u32;
        if (0xAC00..=0xD7A3).contains(&code)
            || (0x1100..=0x11FF).contains(&code)
            || (0x3130..=0x318F).contains(&code)
        {
            hangul += 1;
        } else if (0x3040..=0x309F).contains(&code)
            || (0x30A0..=0x30FF).contains(&code)
            || (0xFF66..=0xFF9D).contains(&code)
        {
            kana += 1;
        }
    }
    if hangul > 0 && hangul >= kana {
        Some(LanguageId::Ko)
    } else if kana > 0 {
        Some(LanguageId::Ja)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetChoice {
    pub target: LanguageId,
    /// `true` when the script check was confident. The provider result can
    /// still override this if detection says the source is the target.
    pub confident: bool,
}

/// Language 1 is the normal target. Text already in language 1 goes to language 2.
pub fn choose_target(text: &str, language1: LanguageId, language2: LanguageId) -> TargetChoice {
    match guess_language(text) {
        Some(source) if source == language1 => TargetChoice {
            target: language2,
            confident: true,
        },
        Some(_) => TargetChoice {
            target: language1,
            confident: true,
        },
        None => TargetChoice {
            target: language1,
            confident: false,
        },
    }
}

/// The other configured language, when it is actually different.
pub fn alternate_language(
    current: LanguageId,
    language1: LanguageId,
    language2: LanguageId,
) -> Option<LanguageId> {
    let other = if current == language1 {
        language2
    } else {
        language1
    };
    (other != current).then_some(other)
}

#[cfg(test)]
mod tests {
    use super::*;

    const JA_SAMPLE: &str = "今日の会議は午後3時からです。事前に資料をご確認ください。変更があればチャットで共有します。";
    const KO_SAMPLE: &str = "내일 오전까지 견적서를 보내 드리겠습니다. 검토 후 회신 부탁드립니다.";

    #[test]
    fn mockup_samples_pick_the_other_configured_language() {
        assert_eq!(guess_language(JA_SAMPLE), Some(LanguageId::Ja));
        assert_eq!(guess_language(KO_SAMPLE), Some(LanguageId::Ko));
        assert_eq!(
            choose_target(JA_SAMPLE, LanguageId::Ko, LanguageId::Ja),
            TargetChoice {
                target: LanguageId::Ko,
                confident: true,
            }
        );
        assert_eq!(
            choose_target(KO_SAMPLE, LanguageId::Ko, LanguageId::Ja),
            TargetChoice {
                target: LanguageId::Ja,
                confident: true,
            }
        );
    }

    #[test]
    fn script_guess_covers_hangul_kana_and_ambiguous_text() {
        assert_eq!(guess_language("안녕하세요"), Some(LanguageId::Ko));
        assert_eq!(guess_language("こんにちは"), Some(LanguageId::Ja));
        assert_eq!(guess_language("カタカナ"), Some(LanguageId::Ja));
        assert_eq!(guess_language("今日は会議です"), Some(LanguageId::Ja));
        assert_eq!(guess_language("東京"), None);
        assert_eq!(guess_language("Hello"), None);
        assert_eq!(guess_language("你好"), None);
        assert_eq!(guess_language("OK 감사합니다"), Some(LanguageId::Ko));
    }

    #[test]
    fn english_falls_through_to_language_1() {
        assert_eq!(
            choose_target("Hello", LanguageId::Ko, LanguageId::Ja),
            TargetChoice {
                target: LanguageId::Ko,
                confident: false,
            }
        );
    }

    #[test]
    fn detection_codes_from_both_providers() {
        assert_eq!(from_detected_code("JA"), Some(LanguageId::Ja));
        assert_eq!(from_detected_code("ko"), Some(LanguageId::Ko));
        assert_eq!(from_detected_code("EN-GB"), Some(LanguageId::En));
        assert_eq!(from_detected_code("zh-Hans"), Some(LanguageId::ZhHans));
        assert_eq!(from_detected_code("ZH-HANT"), Some(LanguageId::ZhHant));
        assert_eq!(from_detected_code("PT-BR"), Some(LanguageId::Pt));
        assert_eq!(from_detected_code("xx"), None);
    }

    #[test]
    fn alternate_language_skips_a_duplicate_pair() {
        assert_eq!(
            alternate_language(LanguageId::Ko, LanguageId::Ko, LanguageId::Ja),
            Some(LanguageId::Ja)
        );
        assert_eq!(
            alternate_language(LanguageId::Ko, LanguageId::Ko, LanguageId::Ko),
            None
        );
    }
}
