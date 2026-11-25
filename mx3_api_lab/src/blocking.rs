use std::time::Duration;

use anyhow::{Result, anyhow};
use reqwest::blocking::Client;
use serde::{Serialize, de::DeserializeOwned};

#[derive(Clone)]
pub struct BlockingClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl BlockingClient {
    /// A client for the backend at `base_url`.
    ///
    /// # Panics
    ///
    /// When the HTTP client cannot be built (no TLS backend available).
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .default_headers(mx3_api_core::client_headers())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(30))
            .tcp_keepalive(Some(Duration::from_secs(180)))
            .build()
            .expect("reqwest::Client::build()");
        Self {
            client,
            base_url,
            token: None,
        }
    }

    /// Sent as `Authorization: Bearer` (the finance endpoints require it).
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    fn post(&self, endpoint: &str) -> reqwest::blocking::RequestBuilder {
        let endpoint = endpoint.strip_prefix("/").unwrap_or(endpoint);
        let builder = self.client.post(format!("{}/{}", self.base_url, endpoint));
        match &self.token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }

    fn get(&self, endpoint: &str) -> reqwest::blocking::RequestBuilder {
        let endpoint = endpoint.strip_prefix("/").unwrap_or(endpoint);
        let builder = self.client.get(format!("{}/{}", self.base_url, endpoint));
        match &self.token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// [`post_json`](Self::post_json) for a call that may take longer than the client's 30 s (one that makes several
    /// supplier calls).
    pub fn post_json_waiting<Q: Serialize, T: DeserializeOwned>(
        &self,
        req: &Q,
        endpoint: impl AsRef<str>,
        timeout: Duration,
    ) -> Result<T> {
        let response = self
            .post(endpoint.as_ref())
            .timeout(timeout)
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(req)?)
            .send()?;
        self.json_response(response)
    }

    /// POST a JSON body, decode the JSON answer.
    pub fn post_json<Q: Serialize, T: DeserializeOwned>(
        &self,
        req: &Q,
        endpoint: impl AsRef<str>,
    ) -> Result<T> {
        let response = self
            .post(endpoint.as_ref())
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(req)?)
            .send()?;
        self.json_response(response)
    }

    /// GET an endpoint without arguments, decode the JSON answer.
    pub fn get_json<T: DeserializeOwned>(&self, endpoint: impl AsRef<str>) -> Result<T> {
        self.json_response(self.get(endpoint.as_ref()).send()?)
    }

    /// POST a multipart form (photo upload), ignoring the answer body.
    pub fn post_multipart(
        &self,
        form: reqwest::blocking::multipart::Form,
        endpoint: impl AsRef<str>,
    ) -> Result<()> {
        let response = self.post(endpoint.as_ref()).multipart(form).send()?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text()?;
            return Err(self.request_error(status, &body));
        }
        Ok(())
    }

    fn json_response<T: DeserializeOwned>(
        &self,
        response: reqwest::blocking::Response,
    ) -> Result<T> {
        let status = response.status();
        if !status.is_success() {
            let body = response.text()?;
            return Err(self.request_error(status, &body));
        }
        let bytes = response.bytes()?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// A 404 of this CLI's own request (PLT-24) often just means the server predates the endpoint, or this
    /// build predates the server's: name the gap instead of a bare 404, best-effort (never fails the call it
    /// is building the error for).
    fn request_error(&self, status: reqwest::StatusCode, body: &str) -> anyhow::Error {
        let server_version = (status == reqwest::StatusCode::NOT_FOUND)
            .then(|| self.server_version())
            .flatten();
        build_request_error(status, body, server_version.as_deref())
    }

    fn server_version(&self) -> Option<String> {
        let bytes = self.get("version").send().ok()?.bytes().ok()?;
        serde_json::from_slice::<crate::endpoints::version::BackendVersion>(&bytes)
            .ok()
            .map(|v| v.version)
    }
}

fn build_request_error(
    status: reqwest::StatusCode,
    body: &str,
    server_version: Option<&str>,
) -> anyhow::Error {
    let msg = mx3_api_core::error_message(body);
    match server_version.filter(|_| status == reqwest::StatusCode::NOT_FOUND) {
        Some(server) => anyhow!(
            "Request failed: {status} {msg} (the server runs {server}, this CLI {}: deploy the server)",
            env!("CARGO_PKG_VERSION")
        ),
        None => anyhow!("Request failed: {status} {msg}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_404_names_the_version_gap() {
        let e = build_request_error(
            reqwest::StatusCode::NOT_FOUND,
            "{\"error\":\"no such route\"}",
            Some("0.23.5"),
        );
        let s = e.to_string();
        assert!(s.contains("the server runs 0.23.5"), "{s}");
        assert!(s.contains(env!("CARGO_PKG_VERSION")), "{s}");
        assert!(s.contains("deploy the server"), "{s}");
    }

    #[test]
    fn a_404_without_a_known_server_version_stays_plain() {
        let e = build_request_error(reqwest::StatusCode::NOT_FOUND, "{\"error\":\"nope\"}", None);
        assert!(!e.to_string().contains("deploy the server"));
    }

    #[test]
    fn other_statuses_never_get_the_hint() {
        let e = build_request_error(
            reqwest::StatusCode::BAD_REQUEST,
            "{\"error\":\"bad\"}",
            Some("0.23.5"),
        );
        assert!(!e.to_string().contains("deploy the server"));
    }
}
