use crate::azure;
use crate::deepl;
use crate::http::HttpClient;
use crate::language::{self, alternate_language, choose_target, LanguageId};
use crate::provider::{FallbackReason, ProviderError, ProviderKind};

pub const MAX_TRANSLATE_CHARS: usize = 50_000;

#[derive(Debug, Clone)]
pub struct Credentials {
    pub deepl_key: String,
    pub azure_key: String,
    pub azure_region: String,
}

#[derive(Debug, Clone, Copy)]
pub struct TranslateOptions {
    pub primary: ProviderKind,
    pub fallback_to_azure: bool,
    pub language1: LanguageId,
    pub language2: LanguageId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Translation {
    pub text: String,
    pub detected_raw: String,
    pub detected: Option<LanguageId>,
    pub target: LanguageId,
    pub provider: ProviderKind,
    pub fallback: Option<FallbackReason>,
    /// Successful calls, in order. Each entry is the provider and the source
    /// length it billed. A language swap can bill two calls.
    pub usage: Vec<(ProviderKind, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslateFailure {
    Empty,
    NoApiKey,
    QuotaExceeded,
    Network,
    TooLong,
    Auth,
    Unavailable,
}

impl std::fmt::Display for TranslateFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty"),
            Self::NoApiKey => write!(f, "no api key"),
            Self::QuotaExceeded => write!(f, "quota"),
            Self::Network => write!(f, "network"),
            Self::TooLong => write!(f, "too long"),
            Self::Auth => write!(f, "auth"),
            Self::Unavailable => write!(f, "unavailable"),
        }
    }
}

/// `explicit_target` is `Some` when the user picked a language in the popup.
/// `None` chooses language 1, or language 2 when the text is already language 1.
pub fn translate(
    http: &dyn HttpClient,
    text: &str,
    explicit_target: Option<LanguageId>,
    credentials: &Credentials,
    options: &TranslateOptions,
) -> Result<Translation, TranslateFailure> {
    let text = text.trim();
    if text.is_empty() {
        return Err(TranslateFailure::Empty);
    }
    if text.chars().count() > MAX_TRANSLATE_CHARS {
        return Err(TranslateFailure::TooLong);
    }

    let (target, explicit) = match explicit_target {
        Some(target) => (target, true),
        None => (
            choose_target(text, options.language1, options.language2).target,
            false,
        ),
    };

    let first = try_providers(http, text, target, credentials, options)?;
    if explicit {
        return Ok(first.into_translation(Vec::new()));
    }
    let Some(detected) = first.detected else {
        return Ok(first.into_translation(Vec::new()));
    };
    if detected != first.target {
        return Ok(first.into_translation(Vec::new()));
    }
    let Some(other) = alternate_language(first.target, options.language1, options.language2) else {
        return Ok(first.into_translation(Vec::new()));
    };
    let prior = vec![(first.provider, first.chars)];
    match try_providers(http, text, other, credentials, options) {
        Ok(second) => Ok(second.into_translation(prior)),
        Err(_) => Ok(first.into_translation(Vec::new())),
    }
}

struct Attempt {
    text: String,
    detected_raw: String,
    detected: Option<LanguageId>,
    target: LanguageId,
    provider: ProviderKind,
    fallback: Option<FallbackReason>,
    chars: u64,
}

impl Attempt {
    fn into_translation(self, mut usage: Vec<(ProviderKind, u64)>) -> Translation {
        usage.push((self.provider, self.chars));
        Translation {
            text: self.text,
            detected_raw: self.detected_raw,
            detected: self.detected,
            target: self.target,
            provider: self.provider,
            fallback: self.fallback,
            usage,
        }
    }
}

fn try_providers(
    http: &dyn HttpClient,
    text: &str,
    target: LanguageId,
    credentials: &Credentials,
    options: &TranslateOptions,
) -> Result<Attempt, TranslateFailure> {
    let providers = provider_order(options, credentials);
    if providers.is_empty() {
        return Err(TranslateFailure::NoApiKey);
    }
    let chars = text.chars().count() as u64;
    let mut errors = Vec::new();
    for (index, provider) in providers.iter().copied().enumerate() {
        match call_provider(http, provider, text, target, credentials) {
            Ok((translated, detected_raw)) => {
                let fallback = if index == 0 {
                    None
                } else if errors.iter().any(ProviderError::is_quota) {
                    Some(FallbackReason::Quota)
                } else {
                    Some(FallbackReason::Other)
                };
                return Ok(Attempt {
                    text: translated,
                    detected: language::from_detected_code(&detected_raw),
                    detected_raw,
                    target,
                    provider,
                    fallback,
                    chars,
                });
            }
            Err(error) => errors.push(error),
        }
    }
    Err(collapse(&errors))
}

fn provider_order(options: &TranslateOptions, credentials: &Credentials) -> Vec<ProviderKind> {
    let deepl = !credentials.deepl_key.trim().is_empty();
    let azure = !credentials.azure_key.trim().is_empty();
    match options.primary {
        ProviderKind::Deepl => {
            let mut order = Vec::new();
            if deepl {
                order.push(ProviderKind::Deepl);
            }
            if options.fallback_to_azure && azure {
                order.push(ProviderKind::Azure);
            }
            order
        }
        ProviderKind::Azure => {
            if azure {
                vec![ProviderKind::Azure]
            } else {
                Vec::new()
            }
        }
    }
}

fn call_provider(
    http: &dyn HttpClient,
    provider: ProviderKind,
    text: &str,
    target: LanguageId,
    credentials: &Credentials,
) -> Result<(String, String), ProviderError> {
    match provider {
        ProviderKind::Deepl => {
            let result = deepl::translate(http, &credentials.deepl_key, text, target)?;
            Ok((result.text, result.detected_source_language))
        }
        ProviderKind::Azure => {
            let result = azure::translate(
                http,
                &credentials.azure_key,
                &credentials.azure_region,
                text,
                target,
            )?;
            Ok((result.text, result.detected_language))
        }
    }
}

fn collapse(errors: &[ProviderError]) -> TranslateFailure {
    if errors.is_empty() {
        return TranslateFailure::NoApiKey;
    }
    if errors
        .iter()
        .all(|error| matches!(error, ProviderError::Auth))
    {
        return TranslateFailure::Auth;
    }
    if errors
        .iter()
        .any(|error| matches!(error, ProviderError::Network))
    {
        return TranslateFailure::Network;
    }
    if errors.iter().any(ProviderError::is_quota) {
        return TranslateFailure::QuotaExceeded;
    }
    TranslateFailure::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpError, HttpRequest, HttpResponse};
    use std::cell::RefCell;
    use std::collections::VecDeque;

    const JA_SAMPLE: &str =
        "今日の会議は午後3時からです。事前に資料をご確認ください。変更があればチャットで共有します。";
    const KO_SAMPLE: &str = "내일 오전까지 견적서를 보내 드리겠습니다. 검토 후 회신 부탁드립니다.";

    struct Scripted {
        responses: RefCell<VecDeque<Result<HttpResponse, HttpError>>>,
        requests: RefCell<Vec<HttpRequest>>,
    }

    impl Scripted {
        fn new(responses: Vec<Result<HttpResponse, HttpError>>) -> Self {
            Self {
                responses: RefCell::new(responses.into()),
                requests: RefCell::new(Vec::new()),
            }
        }
    }

    impl HttpClient for Scripted {
        fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
            self.requests.borrow_mut().push(request.clone());
            self.responses
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| Err(HttpError::Network("no scripted response".into())))
        }
    }

    fn deepl_body(text: &str, detected: &str) -> HttpResponse {
        let body = serde_json::json!({
            "translations": [{ "detected_source_language": detected, "text": text }]
        });
        HttpResponse {
            status: 200,
            body: body.to_string().into_bytes(),
        }
    }

    fn azure_body(text: &str, detected: &str) -> HttpResponse {
        let body = serde_json::json!([{
            "detectedLanguage": { "language": detected, "score": 1.0 },
            "translations": [{ "text": text, "to": "ko" }]
        }]);
        HttpResponse {
            status: 200,
            body: body.to_string().into_bytes(),
        }
    }

    fn http_status(status: u16, message: &str) -> HttpResponse {
        HttpResponse {
            status,
            body: serde_json::json!({ "message": message })
                .to_string()
                .into_bytes(),
        }
    }

    fn azure_quota() -> HttpResponse {
        HttpResponse {
            status: 403,
            body: br#"{"error":{"code":403001,"message":"exceeded its free quota"}}"#.to_vec(),
        }
    }

    fn creds() -> Credentials {
        Credentials {
            deepl_key: "test-key:fx".into(),
            azure_key: "azure-key".into(),
            azure_region: "japaneast".into(),
        }
    }

    fn opts(fallback: bool) -> TranslateOptions {
        TranslateOptions {
            primary: ProviderKind::Deepl,
            fallback_to_azure: fallback,
            language1: LanguageId::Ko,
            language2: LanguageId::Ja,
        }
    }

    fn hosts(script: &Scripted) -> Vec<String> {
        script
            .requests
            .borrow()
            .iter()
            .map(|request| request.url.clone())
            .collect()
    }

    #[test]
    fn japanese_clipboard_goes_to_korean_in_one_deepl_call() {
        let script = Scripted::new(vec![Ok(deepl_body(
            "오늘 회의는 오후 3시부터입니다.",
            "JA",
        ))]);
        let result = translate(&script, JA_SAMPLE, None, &creds(), &opts(true)).unwrap();
        assert_eq!(result.target, LanguageId::Ko);
        assert_eq!(result.detected, Some(LanguageId::Ja));
        assert_eq!(result.provider, ProviderKind::Deepl);
        assert_eq!(result.fallback, None);
        assert_eq!(hosts(&script).len(), 1);
        assert!(hosts(&script)[0].starts_with("https://api-free.deepl.com/"));
        let body: serde_json::Value =
            serde_json::from_str(script.requests.borrow()[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["target_lang"], "KO");
        assert!(body.get("source_lang").is_none());
    }

    #[test]
    fn korean_clipboard_goes_to_japanese() {
        let script = Scripted::new(vec![Ok(deepl_body("見積書をお送りします。", "KO"))]);
        let result = translate(&script, KO_SAMPLE, None, &creds(), &opts(true)).unwrap();
        assert_eq!(result.target, LanguageId::Ja);
        assert_eq!(result.detected, Some(LanguageId::Ko));
        assert_eq!(hosts(&script).len(), 1);
        let body: serde_json::Value =
            serde_json::from_str(script.requests.borrow()[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["target_lang"], "JA");
    }

    #[test]
    fn unknown_script_confirms_with_detection_and_swaps_when_source_is_language_1() {
        let script = Scripted::new(vec![
            Ok(deepl_body("같은 언어", "KO")),
            Ok(deepl_body("日本語です", "KO")),
        ]);
        let result = translate(&script, "Hello", None, &creds(), &opts(true)).unwrap();
        assert_eq!(result.target, LanguageId::Ja);
        assert_eq!(result.text, "日本語です");
        assert_eq!(result.usage.len(), 2);
        assert_eq!(hosts(&script).len(), 2);
    }

    #[test]
    fn english_detected_as_english_stays_on_language_1() {
        let script = Scripted::new(vec![Ok(deepl_body("안녕하세요", "EN"))]);
        let result = translate(&script, "Hello", None, &creds(), &opts(false)).unwrap();
        assert_eq!(result.target, LanguageId::Ko);
        assert_eq!(result.detected, Some(LanguageId::En));
        assert_eq!(hosts(&script).len(), 1);
    }

    #[test]
    fn explicit_target_does_not_swap_even_if_it_matches_the_source() {
        let script = Scripted::new(vec![Ok(deepl_body("Hello", "EN"))]);
        let result = translate(
            &script,
            "Hello",
            Some(LanguageId::En),
            &creds(),
            &opts(true),
        )
        .unwrap();
        assert_eq!(result.target, LanguageId::En);
        assert_eq!(hosts(&script).len(), 1);
    }

    #[test]
    fn deepl_quota_falls_back_to_azure() {
        let script = Scripted::new(vec![
            Ok(http_status(456, "Quota exceeded")),
            Ok(azure_body("안녕하세요", "ja")),
        ]);
        let result = translate(&script, JA_SAMPLE, None, &creds(), &opts(true)).unwrap();
        assert_eq!(result.provider, ProviderKind::Azure);
        assert_eq!(result.fallback, Some(FallbackReason::Quota));
        assert_eq!(result.detected, Some(LanguageId::Ja));
        assert!(hosts(&script)[0].contains("api-free.deepl.com"));
        assert!(hosts(&script)[1].contains("api.cognitive.microsofttranslator.com"));
    }

    #[test]
    fn quota_without_fallback_is_an_error() {
        let script = Scripted::new(vec![Ok(http_status(456, "Quota exceeded"))]);
        let err = translate(&script, JA_SAMPLE, None, &creds(), &opts(false)).unwrap_err();
        assert_eq!(err, TranslateFailure::QuotaExceeded);
        assert_eq!(hosts(&script).len(), 1);
    }

    #[test]
    fn quota_with_fallback_but_no_azure_key_is_an_error() {
        let script = Scripted::new(vec![Ok(http_status(456, "Quota exceeded"))]);
        let mut credentials = creds();
        credentials.azure_key.clear();
        let err = translate(&script, JA_SAMPLE, None, &credentials, &opts(true)).unwrap_err();
        assert_eq!(err, TranslateFailure::QuotaExceeded);
    }

    #[test]
    fn network_error_falls_back_when_enabled() {
        let script = Scripted::new(vec![
            Err(HttpError::Network("offline".into())),
            Ok(azure_body("안녕", "en")),
        ]);
        let result = translate(&script, "Hello", None, &creds(), &opts(true)).unwrap();
        assert_eq!(result.fallback, Some(FallbackReason::Other));
        assert_eq!(result.provider, ProviderKind::Azure);
    }

    #[test]
    fn both_providers_offline_is_a_network_error() {
        let script = Scripted::new(vec![
            Err(HttpError::Network("a".into())),
            Err(HttpError::Network("b".into())),
        ]);
        let err = translate(&script, "Hello", None, &creds(), &opts(true)).unwrap_err();
        assert_eq!(err, TranslateFailure::Network);
    }

    #[test]
    fn missing_keys_do_not_call_the_network() {
        let script = Scripted::new(vec![]);
        let empty = Credentials {
            deepl_key: String::new(),
            azure_key: String::new(),
            azure_region: "japaneast".into(),
        };
        assert_eq!(
            translate(&script, "Hello", None, &empty, &opts(true)).unwrap_err(),
            TranslateFailure::NoApiKey
        );
        assert!(hosts(&script).is_empty());
    }

    #[test]
    fn primary_azure_does_not_call_deepl() {
        let script = Scripted::new(vec![Ok(azure_body("안녕", "en"))]);
        let mut options = opts(true);
        options.primary = ProviderKind::Azure;
        let result = translate(&script, "Hello", None, &creds(), &options).unwrap();
        assert_eq!(result.provider, ProviderKind::Azure);
        assert_eq!(result.fallback, None);
        assert!(hosts(&script)[0].contains("microsofttranslator"));
    }

    #[test]
    fn empty_and_too_long_text_skip_http() {
        let script = Scripted::new(vec![]);
        assert_eq!(
            translate(&script, "   ", None, &creds(), &opts(true)).unwrap_err(),
            TranslateFailure::Empty
        );
        let long = "가".repeat(MAX_TRANSLATE_CHARS + 1);
        assert_eq!(
            translate(&script, &long, None, &creds(), &opts(true)).unwrap_err(),
            TranslateFailure::TooLong
        );
        assert!(hosts(&script).is_empty());
    }

    #[test]
    fn auth_failure_is_not_reported_as_a_missing_key() {
        let script = Scripted::new(vec![Ok(http_status(403, "Forbidden"))]);
        let err = translate(&script, "Hello", None, &creds(), &opts(false)).unwrap_err();
        assert_eq!(err, TranslateFailure::Auth);
    }

    #[test]
    fn failed_swap_keeps_the_first_translation() {
        let script = Scripted::new(vec![
            Ok(deepl_body("같은 언어", "KO")),
            Ok(http_status(500, "boom")),
            Ok(azure_quota()),
        ]);
        let mut options = opts(true);
        options.fallback_to_azure = true;
        let result = translate(&script, "Hello", None, &creds(), &options).unwrap();
        assert_eq!(result.text, "같은 언어");
        assert_eq!(result.target, LanguageId::Ko);
        assert_eq!(result.usage.len(), 1);
    }

    #[test]
    fn identical_language_pair_does_not_loop() {
        let script = Scripted::new(vec![Ok(deepl_body("안녕", "KO"))]);
        let mut options = opts(false);
        options.language2 = LanguageId::Ko;
        let result = translate(&script, "Hello", None, &creds(), &options).unwrap();
        assert_eq!(result.target, LanguageId::Ko);
        assert_eq!(hosts(&script).len(), 1);
    }
}
