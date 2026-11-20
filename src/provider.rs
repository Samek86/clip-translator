use serde::{Deserialize, Serialize};

/// Which translation service produces the final text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[default]
    #[serde(alias = "DeepL", alias = "deepl")]
    Deepl,
    #[serde(alias = "Azure", alias = "azure")]
    Azure,
}

impl ProviderKind {
    pub fn from_index(index: i32) -> Self {
        if index == 1 {
            Self::Azure
        } else {
            Self::Deepl
        }
    }

    pub fn index(self) -> i32 {
        match self {
            Self::Deepl => 0,
            Self::Azure => 1,
        }
    }
}

/// Why a single provider call failed. The router decides whether to try Azure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    Auth,
    Quota,
    RateLimited,
    Network,
    BadRequest(String),
    Server(String),
    Parse(String),
}

impl ProviderError {
    pub fn is_quota(&self) -> bool {
        matches!(self, Self::Quota)
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auth => write!(f, "auth"),
            Self::Quota => write!(f, "quota"),
            Self::RateLimited => write!(f, "rate limit"),
            Self::Network => write!(f, "network"),
            Self::BadRequest(msg) | Self::Server(msg) | Self::Parse(msg) => write!(f, "{msg}"),
        }
    }
}

/// DeepL succeeded, then Azure was used because DeepL could not finish the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackReason {
    Quota,
    Other,
}
