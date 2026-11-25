use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "client")]
use crate::Promise;
use crate::orders::api::MxOrdersClient;
#[cfg(feature = "client")]
use crate::promise::Error;
#[cfg(feature = "client")]
use crate::request::{RequestInitiator, RequestKind};
use crate::warehouse::api::MxWarehouseClient;
use reqwest::Client;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::error;

#[derive(Clone)]
pub struct MxClient {
    // next_id: u32,
    pub(crate) request_tx: UnboundedSender<RequestInitiator>,
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn new(base_url: impl AsRef<str>, ui_repaint: tokio::sync::mpsc::Sender<()>) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let base_url = base_url.as_ref().to_string();
        tokio::spawn(async move {
            request_dispatcher(rx, ui_repaint, base_url).await;
        });
        Self {
            // next_id: 0,
            request_tx: tx,
        }
    }

    pub fn orders(&self) -> MxOrdersClient<'_> {
        MxOrdersClient {
            request_tx: &self.request_tx,
        }
    }

    pub fn warehouse(&self) -> MxWarehouseClient<'_> {
        MxWarehouseClient {
            request_tx: &self.request_tx,
        }
    }

    pub fn send_req<T: DeserializeOwned>(
        &self,
        req: RequestInitiator,
        rx: tokio::sync::oneshot::Receiver<Result<Vec<u8>, Error>>,
        marker: &'static str,
    ) -> Promise<T> {
        match self.request_tx.send(req) {
            Ok(_) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }

    pub fn set_backend_url(&self, url: String) {
        let r = self.request_tx.send(RequestInitiator {
            endpoint: "".to_string(),
            kind: RequestKind::SetBaseUrl(url),
            timeout: None,
            response_tx: None,
        });
        if r.is_err() {
            error!("set_backend_url failed to send request");
        }
    }

    /// The client token (PLT-18) every following request carries as `Authorization: Bearer`.
    pub fn set_token(&self, token: Option<String>) {
        let token = token
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        let r = self.request_tx.send(RequestInitiator {
            endpoint: "".to_string(),
            kind: RequestKind::SetToken(token),
            timeout: None,
            response_tx: None,
        });
        if r.is_err() {
            error!("set_token failed to send request");
        }
    }
}

async fn request_dispatcher(
    mut rx: UnboundedReceiver<RequestInitiator>,
    ui_repaint: tokio::sync::mpsc::Sender<()>,
    base_url: String,
) {
    let client = Client::builder()
        .default_headers(mx3_api_core::client_headers())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(30))
        .tcp_keepalive(Some(Duration::from_secs(180)))
        .http2_keep_alive_interval(Some(Duration::from_secs(10)))
        .http2_keep_alive_timeout(Duration::from_secs(3))
        .http2_keep_alive_while_idle(true)
        .build()
        .expect("reqwest::Client::build()");
    let mut base_url = Arc::new(base_url);
    let mut token: Arc<Option<String>> = Arc::new(None);
    while let Some(req) = rx.recv().await {
        let client = client.clone();
        let ui_repaint = ui_repaint.clone();
        match req.kind {
            RequestKind::SetBaseUrl(base) => base_url = Arc::new(base),
            RequestKind::SetToken(t) => token = Arc::new(t),
            _ => {
                let base_url = base_url.clone();
                let token = token.clone();
                tokio::spawn(async move {
                    handle_request(client, req, base_url, token, ui_repaint).await;
                });
            }
        }
    }
}

async fn handle_request(
    client: Client,
    mut req: RequestInitiator,
    base_url: Arc<String>,
    token: Arc<Option<String>>,
    ui_repaint: tokio::sync::mpsc::Sender<()>,
) {
    let builder = match req.kind {
        RequestKind::GetNoArgs => client.get(format!("{}/{}", base_url, req.endpoint)),
        // ArgsKind::Json(args) => builder.json(&args),
        RequestKind::PostJson(args) => {
            let builder = client.post(format!("{}/{}", base_url, req.endpoint));
            builder
                .header("Content-Type", "application/json")
                .body(args)
        }
        RequestKind::Unencodable(e) => {
            RequestInitiator::send_response(req.response_tx.take(), Err(e));
            _ = ui_repaint.try_send(());
            return;
        }
        RequestKind::SetBaseUrl(_) | RequestKind::SetToken(_) => {
            return;
        }
    };
    let builder = match token.as_deref() {
        Some(token) => builder.bearer_auth(token),
        None => builder,
    };
    // let builder = builder.header("request_id", req.id.0);
    let builder = if let Some(timeout) = req.timeout {
        builder.timeout(timeout)
    } else {
        builder
    };
    match builder.send().await {
        Ok(response) => {
            if !response.status().is_success() {
                let status = response.status();
                let error_text = response
                    .text()
                    .await
                    .unwrap_or("Error text not available".into());
                RequestInitiator::send_response(
                    req.response_tx.take(),
                    Err(Error::RequestFailed(
                        status,
                        mx3_api_core::error_message(&error_text),
                    )),
                );
                _ = ui_repaint.try_send(());
                return;
            }

            let result = response.bytes().await.map(|b| b.to_vec());
            RequestInitiator::send_response(req.response_tx.take(), result.map_err(Error::Reqwest));
            _ = ui_repaint.try_send(());
        }
        Err(e) => {
            RequestInitiator::send_response(req.response_tx.take(), Err(Error::Reqwest(e)));
            _ = ui_repaint.try_send(());
        }
    }
}

pub(crate) fn send_req<T: DeserializeOwned>(
    request_tx: &UnboundedSender<RequestInitiator>,
    req: RequestInitiator,
    rx: tokio::sync::oneshot::Receiver<Result<Vec<u8>, Error>>,
    marker: &'static str,
) -> Promise<T> {
    match request_tx.send(req) {
        Ok(_) => Promise::waiting(rx, marker),
        Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
    }
}
