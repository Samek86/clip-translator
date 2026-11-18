/// Minimal HTTP surface so provider tests can run without a network stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl std::fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let headers: Vec<(String, String)> = self
            .headers
            .iter()
            .map(|(name, value)| {
                let lower = name.to_ascii_lowercase();
                let shown = if lower == "authorization" || lower.contains("subscription-key") {
                    "***".to_string()
                } else {
                    value.clone()
                };
                (name.clone(), shown)
            })
            .collect();
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field("body", &self.body)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    Network(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "{msg}"),
        }
    }
}

pub trait HttpClient {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError>;
}

pub fn shorten_message(message: &str) -> String {
    message
        .trim()
        .chars()
        .take(160)
        .collect::<String>()
        .replace(['\n', '\r'], " ")
}
