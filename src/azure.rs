use crate::http::{shorten_message, HttpClient, HttpError, HttpRequest, HttpResponse, Method};
use crate::language::LanguageId;
use crate::provider::ProviderError;

pub const DETECT_PROBE: &str = "hi";
pub const MONTHLY_CHAR_LIMIT: u64 = 2_000_000;

pub struct AzureRegion {
    pub id: &'static str,
    pub label: &'static str,
}

pub const AZURE_REGIONS: &[AzureRegion] = &[
    AzureRegion {
        id: "japaneast",
        label: "Japan East (japaneast)",
    },
    AzureRegion {
        id: "japanwest",
        label: "Japan West (japanwest)",
    },
    AzureRegion {
        id: "koreacentral",
        label: "Korea Central (koreacentral)",
    },
    AzureRegion {
        id: "koreasouth",
        label: "Korea South (koreasouth)",
    },
    AzureRegion {
        id: "eastus",
        label: "East US (eastus)",
    },
    AzureRegion {
        id: "eastus2",
        label: "East US 2 (eastus2)",
    },
    AzureRegion {
        id: "westus",
        label: "West US (westus)",
    },
    AzureRegion {
        id: "westus2",
        label: "West US 2 (westus2)",
    },
    AzureRegion {
        id: "westeurope",
        label: "West Europe (westeurope)",
    },
    AzureRegion {
        id: "northeurope",
        label: "North Europe (northeurope)",
    },
    AzureRegion {
        id: "southeastasia",
        label: "Southeast Asia (southeastasia)",
    },
    AzureRegion {
        id: "eastasia",
        label: "East Asia (eastasia)",
    },
    AzureRegion {
        id: "global",
        label: "Global (global)",
    },
];

pub fn region_index(id: &str) -> usize {
    AZURE_REGIONS
        .iter()
        .position(|region| region.id.eq_ignore_ascii_case(id.trim()))
        .unwrap_or(0)
}

pub fn region_id(index: i32) -> &'static str {
    usize::try_from(index)
        .ok()
        .and_then(|index| AZURE_REGIONS.get(index))
        .map(|region| region.id)
        .unwrap_or(AZURE_REGIONS[0].id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AzureTranslation {
    pub text: String,
    pub detected_language: String,
}

pub fn translate_request(key: &str, region: &str, text: &str, target: LanguageId) -> HttpRequest {
    let body = serde_json::json!([{ "Text": text }]);
    HttpRequest {
        method: Method::Post,
        url: format!(
            "https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to={}",
            target.azure_code()
        ),
        headers: auth_headers(key, region),
        body: Some(body.to_string()),
    }
}

pub fn detect_request(key: &str, region: &str) -> HttpRequest {
    let body = serde_json::json!([{ "Text": DETECT_PROBE }]);
    HttpRequest {
        method: Method::Post,
        url: "https://api.cognitive.microsofttranslator.com/detect?api-version=3.0".to_string(),
        headers: auth_headers(key, region),
        body: Some(body.to_string()),
    }
}

fn auth_headers(key: &str, region: &str) -> Vec<(String, String)> {
    vec![
        (
            "Ocp-Apim-Subscription-Key".to_string(),
            key.trim().to_string(),
        ),
        (
            "Ocp-Apim-Subscription-Region".to_string(),
            region.trim().to_string(),
        ),
        ("Content-Type".to_string(), "application/json".to_string()),
    ]
}

pub fn translate(
    http: &dyn HttpClient,
    key: &str,
    region: &str,
    text: &str,
    target: LanguageId,
) -> Result<AzureTranslation, ProviderError> {
    let response = execute(http, &translate_request(key, region, text, target))?;
    parse_translate(response.status, &response.body)
}

pub fn detect(http: &dyn HttpClient, key: &str, region: &str) -> Result<(), ProviderError> {
    let response = execute(http, &detect_request(key, region))?;
    parse_detect(response.status, &response.body)
}

fn execute(http: &dyn HttpClient, request: &HttpRequest) -> Result<HttpResponse, ProviderError> {
    http.execute(request).map_err(|err| match err {
        HttpError::Network(_) => ProviderError::Network,
    })
}

pub fn parse_translate(status: u16, body: &[u8]) -> Result<AzureTranslation, ProviderError> {
    if status != 200 {
        return Err(classify(status, body));
    }
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|err| ProviderError::Parse(err.to_string()))?;
    let item = value
        .get(0)
        .ok_or_else(|| ProviderError::Parse("empty response".into()))?;
    let detected = item
        .get("detectedLanguage")
        .and_then(|value| value.get("language"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let text = item
        .get("translations")
        .and_then(|items| items.get(0))
        .and_then(|item| item.get("text"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    if text.is_empty() && detected.is_empty() {
        return Err(ProviderError::Parse("empty translation".into()));
    }
    Ok(AzureTranslation {
        text,
        detected_language: detected,
    })
}

pub fn parse_detect(status: u16, body: &[u8]) -> Result<(), ProviderError> {
    if status != 200 {
        return Err(classify(status, body));
    }
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|err| ProviderError::Parse(err.to_string()))?;
    if value.as_array().is_some_and(|items| !items.is_empty()) {
        Ok(())
    } else {
        Err(ProviderError::Parse("detect".into()))
    }
}

fn classify(status: u16, body: &[u8]) -> ProviderError {
    let code = error_code(body);
    let message = error_message(body);
    let quota = code == Some(403_001) || message.to_ascii_lowercase().contains("quota");
    let short = shorten_message(&message);
    match status {
        401 => ProviderError::Auth,
        403 if quota => ProviderError::Quota,
        403 => ProviderError::Auth,
        429 => ProviderError::RateLimited,
        400 => ProviderError::BadRequest(short),
        _ if quota => ProviderError::Quota,
        _ => ProviderError::Server(short),
    }
}

fn error_code(body: &[u8]) -> Option<i64> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let code = value.get("error")?.get("code")?;
    code.as_i64()
        .or_else(|| code.as_str().and_then(|text| text.parse().ok()))
}

fn error_message(body: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(|message| message.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_request_uses_v3_and_puts_the_key_in_a_header() {
        let request = translate_request("azure-key", "japaneast", "안녕", LanguageId::Ja);
        assert_eq!(
            request.url,
            "https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to=ja"
        );
        assert!(!request.url.contains("azure-key"));
        assert!(request
            .headers
            .iter()
            .any(|(name, value)| { name == "Ocp-Apim-Subscription-Key" && value == "azure-key" }));
        assert!(request.headers.iter().any(|(name, value)| {
            name == "Ocp-Apim-Subscription-Region" && value == "japaneast"
        }));
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body[0]["Text"], "안녕");
    }

    #[test]
    fn parses_detection_from_the_translate_response() {
        let body = r#"[{"detectedLanguage":{"language":"ko","score":0.99},"translations":[{"text":"明日の午前中までに見積書をお送りします。","to":"ja"}]}]"#;
        let parsed = parse_translate(200, body.as_bytes()).unwrap();
        assert_eq!(parsed.detected_language, "ko");
        assert!(parsed.text.contains("見積書"));
    }

    #[test]
    fn quota_code_403001_is_quota_and_401_is_auth() {
        let quota = br#"{"error":{"code":403001,"message":"The operation is not allowed because the subscription has exceeded its free quota."}}"#;
        assert_eq!(
            parse_translate(403, quota).unwrap_err(),
            ProviderError::Quota
        );
        let auth = br#"{"error":{"code":401000,"message":"The request is not authorized because credentials are missing or invalid."}}"#;
        assert_eq!(parse_detect(401, auth).unwrap_err(), ProviderError::Auth);
        assert!(parse_detect(200, br#"[{"language":"en","score":1}]"#).is_ok());
    }

    #[test]
    fn japan_east_is_the_default_region() {
        assert_eq!(AZURE_REGIONS[0].label, "Japan East (japaneast)");
        assert_eq!(region_index("japaneast"), 0);
        assert_eq!(region_index("missing"), 0);
        assert_eq!(region_id(2), "koreacentral");
        assert_eq!(MONTHLY_CHAR_LIMIT, 2_000_000);
    }
}
