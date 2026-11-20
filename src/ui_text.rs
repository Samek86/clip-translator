use crate::language::LanguageId;
use crate::provider::{FallbackReason, ProviderKind};
use crate::router::TranslateFailure;

pub const DEEPL_STATUS_PREFIX: &str = "Connected · characters left ";
pub const AZURE_STATUS_PREFIX: &str = "Connected · characters left this month ";
#[cfg_attr(not(test), allow(dead_code))]
pub const STATUS_SLASH: &str = " / ";
#[cfg_attr(not(test), allow(dead_code))]
pub const AZURE_PLAN_HINT: &str = "Azure Translator F0 free tier · 2 million characters per month";
pub const HOTKEY_HINT: &str = "Click the field, then press the new shortcut.";
pub const HOTKEY_RECORDING_HINT: &str = "Press Esc to cancel.";
pub const HOTKEY_NEEDS_MODIFIER: &str = "Press it together with Ctrl, Alt, or Win.";
pub const HOTKEY_UNSUPPORTED: &str = "This key cannot be used as a shortcut.";
pub const HOTKEY_REGISTER_FAILED: &str =
    "This shortcut could not be registered. Choose another combination.";
pub const TESTING: &str = "Checking…";
pub const COPY_LABEL: &str = "Copy";
pub const COPIED_LABEL: &str = "Copied";
pub const COPY_FAILED: &str = "Copy failed";

pub fn package_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn settings_version_line() -> String {
    format!("Version {}", package_version())
}

pub fn tray_tooltip() -> String {
    format!("Clip Translator {}", package_version())
}

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
            "DeepL API Developer · one-time 1 million characters · keys end with :fx"
        } else {
            "DeepL API · api.deepl.com"
        }
    } else if key.is_empty() && primary == ProviderKind::Azure {
        "Enter a DeepL key too if you want automatic fallback."
    } else if key.is_empty() {
        "Enter a key, then test the connection."
    } else if key.ends_with(":fx") {
        "DeepL API Developer · one-time 1 million characters · keys end with :fx"
    } else {
        "DeepL API · api.deepl.com"
    }
}

pub fn provider_note(provider: ProviderKind, fallback: Option<FallbackReason>) -> String {
    match (provider, fallback) {
        (ProviderKind::Deepl, _) => "Translated with DeepL".to_string(),
        (ProviderKind::Azure, Some(FallbackReason::Quota)) => {
            "Translated with Azure Translator · switched because the DeepL quota was used up"
                .to_string()
        }
        (ProviderKind::Azure, Some(FallbackReason::Other)) => {
            "Translated with Azure Translator · switched because DeepL failed".to_string()
        }
        (ProviderKind::Azure, None) => "Translated with Azure Translator".to_string(),
    }
}

pub fn detected_label(language: Option<LanguageId>, raw: &str) -> String {
    match language {
        Some(language) => format!("{} detected", language.endonym()),
        None if raw.trim().is_empty() => "Language not detected".to_string(),
        None => format!("{} detected", raw.trim()),
    }
}

pub fn connection_error(err: &crate::provider::ProviderError) -> &'static str {
    use crate::provider::ProviderError;
    match err {
        ProviderError::Auth => "Check the API key.",
        ProviderError::Quota => "Quota exceeded.",
        ProviderError::Network => "Could not reach the network.",
        ProviderError::RateLimited => "Too many requests. Try again in a moment.",
        ProviderError::BadRequest(_) | ProviderError::Server(_) | ProviderError::Parse(_) => {
            "Connection failed. Check the key and region."
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

pub fn popup_for_failure(failure: &TranslateFailure) -> PopupText {
    match failure {
        TranslateFailure::Empty => PopupText {
            title: "Clipboard is empty",
            detail: "Copy the text to translate, then press the shortcut again.",
            settings_button: false,
            retry_button: false,
        },
        TranslateFailure::NoApiKey => PopupText {
            title: "No API key",
            detail: "Enter a DeepL or Azure Translator key in Settings, then try again.",
            settings_button: true,
            retry_button: false,
        },
        TranslateFailure::QuotaExceeded => PopupText {
            title: "Translation quota exceeded",
            detail: "DeepL's character quota is used up, and automatic fallback is off or Azure's quota is used up too.",
            settings_button: true,
            retry_button: false,
        },
        TranslateFailure::Network => PopupText {
            title: "Network error",
            detail: "Check the internet connection, then try again.",
            settings_button: false,
            retry_button: true,
        },
        TranslateFailure::TooLong => PopupText {
            title: "Text is too long",
            detail: "At most 50,000 characters can be translated at once.",
            settings_button: false,
            retry_button: false,
        },
        TranslateFailure::Auth => PopupText {
            title: "Translation failed",
            detail: "Check the API key.",
            settings_button: true,
            retry_button: false,
        },
        TranslateFailure::Unavailable => PopupText {
            title: "Translation failed",
            detail: "The translation service rejected the request. Try again in a moment.",
            settings_button: false,
            retry_button: true,
        },
    }
}

pub fn popup_not_text() -> PopupText {
    PopupText {
        title: "Clipboard has no text",
        detail: "Copy the text to translate, then press the shortcut again.",
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
            "Connected · characters left 867,420 / 1,000,000"
        );
        assert_eq!(
            format!(
                "{AZURE_STATUS_PREFIX}{}{STATUS_SLASH}{}",
                format_grouped(1_754_000),
                format_grouped(2_000_000)
            ),
            "Connected · characters left this month 1,754,000 / 2,000,000"
        );
    }

    #[test]
    fn provider_notes_are_english() {
        assert_eq!(
            provider_note(ProviderKind::Deepl, None),
            "Translated with DeepL"
        );
        assert_eq!(
            provider_note(ProviderKind::Azure, Some(FallbackReason::Quota)),
            "Translated with Azure Translator · switched because the DeepL quota was used up"
        );
        assert_eq!(
            provider_note(ProviderKind::Azure, None),
            "Translated with Azure Translator"
        );
    }

    #[test]
    fn detected_labels_and_popup_copy() {
        assert_eq!(
            detected_label(Some(LanguageId::Ja), "JA"),
            "Japanese detected"
        );
        assert_eq!(
            detected_label(Some(LanguageId::Ko), "ko"),
            "Korean detected"
        );
        assert_eq!(LanguageId::Ko.endonym(), "Korean");
        assert_eq!(LanguageId::Ja.endonym(), "Japanese");
        let no_key = popup_for_failure(&TranslateFailure::NoApiKey);
        assert_eq!(no_key.title, "No API key");
        assert_eq!(
            no_key.detail,
            "Enter a DeepL or Azure Translator key in Settings, then try again."
        );
        assert!(no_key.settings_button);
        let quota = popup_for_failure(&TranslateFailure::QuotaExceeded);
        assert_eq!(quota.title, "Translation quota exceeded");
        let empty = popup_for_failure(&TranslateFailure::Empty);
        assert_eq!(empty.title, "Clipboard is empty");
        assert_eq!(
            empty.detail,
            "Copy the text to translate, then press the shortcut again."
        );
        assert_eq!(
            popup_for_failure(&TranslateFailure::Network).title,
            "Network error"
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
            "Enter a key, then test the connection."
        );
        assert_eq!(
            deepl_hint("", ProviderKind::Azure, false),
            "Enter a DeepL key too if you want automatic fallback."
        );
        assert_eq!(
            deepl_hint("abc:fx", ProviderKind::Deepl, true),
            "DeepL API Developer · one-time 1 million characters · keys end with :fx"
        );
        assert_eq!(
            deepl_hint("pro-key", ProviderKind::Deepl, true),
            "DeepL API · api.deepl.com"
        );
    }

    #[test]
    fn version_line_and_tray_tooltip_use_the_package_version() {
        assert_eq!(package_version(), env!("CARGO_PKG_VERSION"));
        assert_eq!(
            settings_version_line(),
            format!("Version {}", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            tray_tooltip(),
            format!("Clip Translator {}", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(settings_version_line(), "Version 0.1.0");
        assert_eq!(tray_tooltip(), "Clip Translator 0.1.0");
    }

    #[test]
    fn progress_is_the_remaining_fraction() {
        let ratio = remaining_ratio(867_420, 1_000_000);
        assert!((ratio - 0.86742).abs() < 0.0001);
    }
}
