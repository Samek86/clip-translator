use crate::language::LanguageId;
use crate::provider::{FallbackReason, ProviderKind};
use crate::router::TranslateFailure;

pub const DEEPL_STATUS_PREFIX: &str = "연결됨 · 남은 글자 수 ";
pub const AZURE_STATUS_PREFIX: &str = "연결됨 · 이번 달 남은 글자 수 ";
#[cfg_attr(not(test), allow(dead_code))]
pub const STATUS_SLASH: &str = " / ";
#[cfg_attr(not(test), allow(dead_code))]
pub const AZURE_PLAN_HINT: &str = "Azure Translator F0 무료 등급 · 매달 200만 자";
pub const HOTKEY_HINT: &str = "칸을 클릭한 뒤 새 조합을 누르면 변경됩니다.";
pub const HOTKEY_RECORDING_HINT: &str = "Esc를 누르면 취소됩니다.";
pub const HOTKEY_NEEDS_MODIFIER: &str = "Ctrl, Alt, Win 중 하나와 함께 눌러 주세요.";
pub const HOTKEY_UNSUPPORTED: &str = "이 키는 단축키로 쓸 수 없습니다.";
pub const HOTKEY_REGISTER_FAILED: &str =
    "이 단축키는 등록할 수 없습니다. 다른 조합을 선택해 주세요.";
pub const TESTING: &str = "확인 중…";
pub const COPY_LABEL: &str = "복사";
pub const COPIED_LABEL: &str = "복사됨";
pub const COPY_FAILED: &str = "복사 실패";
pub const BUSY_NOTE: &str = "번역하는 중…";

pub fn format_grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

pub fn remaining_ratio(remaining_chars: u64, limit: u64) -> f32 {
    if limit == 0 {
        0.0
    } else {
        (remaining_chars as f64 / limit as f64).clamp(0.0, 1.0) as f32
    }
}

pub fn deepl_hint(key: &str, primary: ProviderKind, connected: bool) -> &'static str {
    let key = key.trim();
    if connected {
        if key.ends_with(":fx") || key.is_empty() {
            "DeepL API Developer · 일회 100만 자 · 키 끝이 :fx"
        } else {
            "DeepL API · api.deepl.com"
        }
    } else if key.is_empty() && primary == ProviderKind::Azure {
        "자동 전환을 쓰려면 DeepL 키도 입력해 주세요."
    } else if key.is_empty() {
        "키를 입력한 뒤 연결 테스트를 눌러 주세요."
    } else if key.ends_with(":fx") {
        "DeepL API Developer · 일회 100만 자 · 키 끝이 :fx"
    } else {
        "DeepL API · api.deepl.com"
    }
}

pub fn provider_note(provider: ProviderKind, fallback: Option<FallbackReason>) -> String {
    match (provider, fallback) {
        (ProviderKind::Deepl, _) => "DeepL로 번역됨".to_string(),
        (ProviderKind::Azure, Some(FallbackReason::Quota)) => {
            "Azure Translator로 번역됨 · DeepL 한도 소진으로 전환".to_string()
        }
        (ProviderKind::Azure, Some(FallbackReason::Other)) => {
            "Azure Translator로 번역됨 · DeepL 오류로 전환".to_string()
        }
        (ProviderKind::Azure, None) => "Azure Translator로 번역됨".to_string(),
    }
}

pub fn detected_label(language: Option<LanguageId>, raw: &str) -> String {
    match language {
        Some(language) => format!("{} 감지", language.korean_name()),
        None if raw.trim().is_empty() => "언어를 감지하지 못했습니다".to_string(),
        None => format!("{} 감지", raw.trim()),
    }
}

pub fn connection_error(err: &crate::provider::ProviderError) -> &'static str {
    use crate::provider::ProviderError;
    match err {
        ProviderError::Auth => "API 키를 확인해 주세요.",
        ProviderError::Quota => "할당량을 초과했습니다.",
        ProviderError::Network => "네트워크에 연결할 수 없습니다.",
        ProviderError::RateLimited => "요청이 너무 많습니다. 잠시 후 다시 시도해 주세요.",
        ProviderError::BadRequest(_) | ProviderError::Server(_) | ProviderError::Parse(_) => {
            "연결에 실패했습니다. 키와 지역을 확인해 주세요."
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupText {
    pub title: &'static str,
    pub detail: &'static str,
    pub settings_button: bool,
    pub retry_button: bool,
}

pub fn popup_loading() -> PopupText {
    PopupText {
        title: "번역하는 중…",
        detail: "클립보드의 글을 읽고 있습니다.",
        settings_button: false,
        retry_button: false,
    }
}

pub fn popup_for_failure(failure: &TranslateFailure) -> PopupText {
    match failure {
        TranslateFailure::Empty => PopupText {
            title: "클립보드가 비어 있습니다",
            detail: "번역할 글을 복사한 뒤 단축키를 다시 눌러 주세요.",
            settings_button: false,
            retry_button: false,
        },
        TranslateFailure::NoApiKey => PopupText {
            title: "API 키가 없습니다",
            detail: "설정에서 DeepL 또는 Azure Translator 키를 입력한 뒤 다시 시도해 주세요.",
            settings_button: true,
            retry_button: false,
        },
        TranslateFailure::QuotaExceeded => PopupText {
            title: "번역 한도를 초과했습니다",
            detail: "DeepL 글자 수를 모두 사용했고, Azure로 자동 전환이 꺼져 있거나 Azure 한도도 끝났습니다.",
            settings_button: true,
            retry_button: false,
        },
        TranslateFailure::Network => PopupText {
            title: "네트워크 오류",
            detail: "인터넷 연결을 확인한 뒤 다시 시도해 주세요.",
            settings_button: false,
            retry_button: true,
        },
        TranslateFailure::TooLong => PopupText {
            title: "글이 너무 깁니다",
            detail: "한 번에 최대 50,000자까지 번역할 수 있습니다.",
            settings_button: false,
            retry_button: false,
        },
        TranslateFailure::Auth => PopupText {
            title: "번역에 실패했습니다",
            detail: "API 키를 확인해 주세요.",
            settings_button: true,
            retry_button: false,
        },
        TranslateFailure::Unavailable => PopupText {
            title: "번역에 실패했습니다",
            detail: "번역 서비스가 요청을 거절했습니다. 잠시 후 다시 시도해 주세요.",
            settings_button: false,
            retry_button: true,
        },
    }
}

pub fn popup_not_text() -> PopupText {
    PopupText {
        title: "클립보드에 텍스트가 없습니다",
        detail: "번역할 글을 복사한 뒤 단축키를 다시 눌러 주세요.",
        settings_button: false,
        retry_button: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::FallbackReason;
    use crate::provider::ProviderKind;
    use crate::router::MAX_TRANSLATE_CHARS;

    #[test]
    fn grouped_numbers_match_the_mockup() {
        assert_eq!(format_grouped(867_420), "867,420");
        assert_eq!(format_grouped(1_754_000), "1,754,000");
        assert_eq!(format_grouped(1_000_000), "1,000,000");
        assert_eq!(format_grouped(0), "0");
        assert_eq!(
            format!(
                "{DEEPL_STATUS_PREFIX}{}{STATUS_SLASH}{}",
                format_grouped(867_420),
                format_grouped(1_000_000)
            ),
            "연결됨 · 남은 글자 수 867,420 / 1,000,000"
        );
        assert_eq!(
            format!(
                "{AZURE_STATUS_PREFIX}{}{STATUS_SLASH}{}",
                format_grouped(1_754_000),
                format_grouped(2_000_000)
            ),
            "연결됨 · 이번 달 남은 글자 수 1,754,000 / 2,000,000"
        );
    }

    #[test]
    fn provider_notes_match_the_mockup() {
        assert_eq!(provider_note(ProviderKind::Deepl, None), "DeepL로 번역됨");
        assert_eq!(
            provider_note(ProviderKind::Azure, Some(FallbackReason::Quota)),
            "Azure Translator로 번역됨 · DeepL 한도 소진으로 전환"
        );
        assert_eq!(
            provider_note(ProviderKind::Azure, None),
            "Azure Translator로 번역됨"
        );
    }

    #[test]
    fn detected_labels_and_popup_copy() {
        assert_eq!(detected_label(Some(LanguageId::Ja), "JA"), "일본어 감지");
        assert_eq!(detected_label(Some(LanguageId::Ko), "ko"), "한국어 감지");
        let no_key = popup_for_failure(&TranslateFailure::NoApiKey);
        assert_eq!(no_key.title, "API 키가 없습니다");
        assert_eq!(
            no_key.detail,
            "설정에서 DeepL 또는 Azure Translator 키를 입력한 뒤 다시 시도해 주세요."
        );
        assert!(no_key.settings_button);
        let quota = popup_for_failure(&TranslateFailure::QuotaExceeded);
        assert_eq!(quota.title, "번역 한도를 초과했습니다");
        let empty = popup_for_failure(&TranslateFailure::Empty);
        assert_eq!(empty.title, "클립보드가 비어 있습니다");
        assert_eq!(
            empty.detail,
            "번역할 글을 복사한 뒤 단축키를 다시 눌러 주세요."
        );
        let loading = popup_loading();
        assert_eq!(loading.title, "번역하는 중…");
        assert_eq!(loading.detail, "클립보드의 글을 읽고 있습니다.");
        assert_eq!(
            popup_for_failure(&TranslateFailure::Network).title,
            "네트워크 오류"
        );
        assert!(popup_for_failure(&TranslateFailure::TooLong)
            .detail
            .contains("50,000"));
        assert_eq!(MAX_TRANSLATE_CHARS, 50_000);
    }

    #[test]
    fn deepl_hint_follows_the_key_and_the_selected_provider() {
        assert_eq!(
            deepl_hint("", ProviderKind::Deepl, false),
            "키를 입력한 뒤 연결 테스트를 눌러 주세요."
        );
        assert_eq!(
            deepl_hint("", ProviderKind::Azure, false),
            "자동 전환을 쓰려면 DeepL 키도 입력해 주세요."
        );
        assert_eq!(
            deepl_hint("abc:fx", ProviderKind::Deepl, true),
            "DeepL API Developer · 일회 100만 자 · 키 끝이 :fx"
        );
        assert_eq!(
            deepl_hint("pro-key", ProviderKind::Deepl, true),
            "DeepL API · api.deepl.com"
        );
    }

    #[test]
    fn progress_is_the_remaining_fraction() {
        let ratio = remaining_ratio(867_420, 1_000_000);
        assert!((ratio - 0.86742).abs() < 0.0001);
    }
}
