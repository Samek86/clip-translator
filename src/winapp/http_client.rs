use std::time::Duration;

use crate::http::{HttpClient, HttpError, HttpRequest, HttpResponse, Method};

/// Blocking HTTPS client. On Windows, `native-tls` is Schannel, so this
/// cross-compiles without a C crypto library and uses the system certificate store.
#[derive(Clone)]
pub struct ReqwestClient {
    client: reqwest::blocking::Client,
}

impl ReqwestClient {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("clip-translator/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Self { client })
    }
}

impl HttpClient for ReqwestClient {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let method = match request.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
        };
        let mut builder = self.client.request(method, &request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }
        let response = builder
            .send()
            .map_err(|err| HttpError::Network(err.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .map_err(|err| HttpError::Network(err.to_string()))?
            .to_vec();
        Ok(HttpResponse { status, body })
    }
}
