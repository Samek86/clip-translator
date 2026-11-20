use crate::http::{shorten_message, HttpClient, HttpError, HttpRequest, HttpResponse, Method};
use crate::language::LanguageId;
use crate::provider::ProviderError;

pub fn api_base(key: &str) -> &'static str {
    if key.trim().ends_with(":fx") {
        "https://api-free.deepl.com"
    } else {
        "https://api.deepl.com"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeeplTranslation {
    pub text: String,
    pub detected_source_language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeeplUsage {
    pub character_count: u64,
    pub character_limit: u64,
}

pub fn translate_request(key: &str, text: &str, target: LanguageId) -> HttpRequest {
    let body = serde_json::json!({
        "text": [text],
        "target_lang": target.deepl_target(),
    });
    HttpRequest {
        method: Method::Post,
        url: format!("{}/v2/translate", api_base(key)),
        headers: auth_headers(key),
        body: Some(body.to_string()),
    }
}

pub fn usage_request(key: &str) -> HttpRequest {
    HttpRequest {
        method: Method::Get,
        url: format!("{}/v2/usage", api_base(key)),
        headers: auth_headers(key),
        body: None,
    }
}

fn auth_headers(key: &str) -> Vec<(String, String)> {
    vec![
        (
            "Authorization".to_string(),
            format!("DeepL-Auth-Key {}", key.trim()),
        ),
        ("Content-Type".to_string(), "application/json".to_string()),
    ]
}

pub fn translate(
    http: &dyn HttpClient,
    key: &str,
    text: &str,
    target: LanguageId,
) -> Result<DeeplTranslation, ProviderError> {
    let response = execute(http, &translate_request(key, text, target))?;
    parse_translate(response.status, &response.body)
}

pub fn usage(http: &dyn HttpClient, key: &str) -> Result<DeeplUsage, ProviderError> {
    let response = execute(http, &usage_request(key))?;
    parse_usage(response.status, &response.body)
}

fn execute(http: &dyn HttpClient, request: &HttpRequest) -> Result<HttpResponse, ProviderError> {
    http.execute(request).map_err(|err| match err {
        HttpError::Network(_) => ProviderError::Network,
    })
}

pub fn parse_translate(status: u16, body: &[u8]) -> Result<DeeplTranslation, ProviderError> {
    if status != 200 {
        return Err(classify(status, body));
    }
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|err| ProviderError::Parse(err.to_string()))?;
    let item = value
        .get("translations")
        .and_then(|items| items.get(0))
        .ok_or_else(|| ProviderError::Parse("translations".into()))?;
    let text = item
        .get("text")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let detected = item
        .get("detected_source_language")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    if text.is_empty() && detected.is_empty() {
        return Err(ProviderError::Parse("empty translation".into()));
    }
    Ok(DeeplTranslation {
        text,
        detected_source_language: detected,
    })
}

pub fn parse_usage(status: u16, body: &[u8]) -> Result<DeeplUsage, ProviderError> {
    if status != 200 {
        return Err(classify(status, body));
    }
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|err| ProviderError::Parse(err.to_string()))?;
    let character_count = json_u64(value.get("character_count"))
        .ok_or_else(|| ProviderError::Parse("character_count".into()))?;
    let character_limit = json_u64(value.get("character_limit")).unwrap_or(1_000_000);
    Ok(DeeplUsage {
        character_count,
        character_limit,
    })
}

fn classify(status: u16, body: &[u8]) -> ProviderError {
    let message = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(|message| message.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string());
    let short = shorten_message(&message);
    match status {
        456 => ProviderError::Quota,
        401 | 403 => ProviderError::Auth,
        429 => ProviderError::RateLimited,
        400 => ProviderError::BadRequest(short),
        _ => ProviderError::Server(short),
    }
}

pub(crate) fn json_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    let value = value?;
    if let Some(number) = value.as_u64() {
        return Some(number);
    }
    if let Some(number) = value.as_i64() {
        return u64::try_from(number).ok();
    }
    value.as_f64().map(|number| number as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_keys_use_the_free_host() {
        assert_eq!(api_base("secret:fx"), "https://api-free.deepl.com");
        assert_eq!(api_base("  secret:fx  "), "https://api-free.deepl.com");
        assert_eq!(api_base("pro-key"), "https://api.deepl.com");
        assert_eq!(api_base("pro-key:FX"), "https://api.deepl.com");
    }

    #[test]
    fn translate_request_omits_source_language() {
        let request = translate_request("abc:fx", "hello", LanguageId::Ko);
        assert_eq!(request.url, "https://api-free.deepl.com/v2/translate");
        assert!(request
            .headers
            .iter()
            .any(|(name, value)| name == "Authorization" && value == "DeepL-Auth-Key abc:fx"));
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["target_lang"], "KO");
        assert_eq!(body["text"][0], "hello");
        assert!(body.get("source_lang").is_none());
    }

    #[test]
    fn parses_translation_and_the_mockup_usage_numbers() {
        let body = r#"{"translations":[{"detected_source_language":"JA","text":"안녕하세요"}]}"#;
        let parsed = parse_translate(200, body.as_bytes()).unwrap();
        assert_eq!(parsed.text, "안녕하세요");
        assert_eq!(parsed.detected_source_language, "JA");

        let usage = parse_usage(
            200,
            br#"{"character_count":132580,"character_limit":1000000}"#,
        )
        .unwrap();
        assert_eq!(usage.character_count, 132_580);
        assert_eq!(usage.character_limit, 1_000_000);
        assert_eq!(usage.character_limit - usage.character_count, 867_420);
    }

    #[test]
    fn quota_and_auth_status_codes() {
        assert_eq!(
            parse_translate(456, br#"{"message":"Quota exceeded"}"#).unwrap_err(),
            ProviderError::Quota
        );
        assert_eq!(
            parse_usage(403, br#"{"message":"Forbidden"}"#).unwrap_err(),
            ProviderError::Auth
        );
    }
}
