use std::time::Duration;

use serde::Serialize;
use tokio::sync::oneshot::{Receiver, Sender};
use tracing::{error, warn};

#[cfg(feature = "client")]
use crate::promise::Error;

pub struct RequestInitiator {
    // pub(crate) id: RequestId,
    pub(crate) endpoint: String,
    pub(crate) kind: RequestKind,
    pub(crate) timeout: Option<Duration>,
    pub(crate) response_tx: Option<Sender<Result<Vec<u8>, Error>>>,
}

pub enum RequestKind {
    GetNoArgs,
    /// POST with this JSON body
    PostJson(Vec<u8>),
    /// A body that could not be encoded: nothing is sent, the promise gets this error
    Unencodable(Error),
    SetBaseUrl(String),
    /// The client token sent as `Authorization: Bearer` from now on (`None`: none)
    SetToken(Option<String>),
}

impl RequestKind {
    /// The kind for a POST of `arg` as JSON, or the encoding error.
    fn post_json<T: Serialize>(arg: &T) -> Self {
        match serde_json::to_vec(arg) {
            Ok(body) => Self::PostJson(body),
            Err(e) => Self::Unencodable(Error::JsonError(e)),
        }
    }
}

impl RequestInitiator {
    pub fn get_no_args(endpoint: String) -> (Self, Receiver<Result<Vec<u8>, Error>>) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (
            Self {
                // id,
                endpoint,
                kind: RequestKind::GetNoArgs,
                timeout: Some(Duration::from_secs(30)),
                response_tx: Some(tx),
            },
            rx,
        )
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "public signature mx3's callers use; changing it is its own API change"
    )]
    pub fn post_json_arg<T: Serialize>(
        endpoint: String,
        arg: &T,
    ) -> (Self, Receiver<Result<Vec<u8>, Error>>) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let endpoint = endpoint.strip_prefix("/").unwrap_or(&endpoint).to_owned();
        let kind = RequestKind::post_json(arg);
        (
            Self {
                // id,
                endpoint,
                kind,
                timeout: Some(Duration::from_secs(30)),
                response_tx: Some(tx),
            },
            rx,
        )
    }

    pub(crate) fn send_response(
        tx: Option<Sender<Result<Vec<u8>, Error>>>,
        result: Result<Vec<u8>, Error>,
    ) {
        if let Some(tx) = tx {
            Self::deliver(tx, result);
        } else {
            error!("RequestInitiator: tried to send response twice");
        }
    }

    fn deliver(tx: Sender<Result<Vec<u8>, Error>>, result: Result<Vec<u8>, Error>) {
        if tx.send(result).is_err() {
            warn!("RequestInitiator: channel closed");
        }
    }
}
