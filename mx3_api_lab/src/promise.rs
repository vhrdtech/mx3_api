use std::fmt::{Display, Formatter};

#[cfg(feature = "client")]
use crate::promise::Error::RequestChannelClosed;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::sync::oneshot::Receiver;
use tokio::sync::oneshot::error::TryRecvError;
use tracing::warn;

pub struct Promise<T> {
    state: PromiseState<T>,
    marker: &'static str,
    seen: bool,
    // TODO: Add id to Promise
    // TODO: Add time to Promise?
    // TODO: Add user type, to optionally keep request info needed for processing eventual result
}

#[derive(Default)]
pub enum PromiseState<T> {
    #[default]
    None,
    /// The JSON bytes of the answer, once there
    Waiting(/*RequestId, */ Receiver<Result<Vec<u8>, Error>>),
    Done(Option<T>), // Option used here to make Drop and take() work
    Err(Error),
}

// #[derive(Copy, Clone)]
// pub struct RequestId(pub(crate) u32);

#[derive(Debug, Error)]
pub enum Error {
    #[error("Dispatcher task is no longer running")]
    RequestDispatcherNotAvailable,
    #[error("Promise was dropped before a response was sent to it")]
    RequestChannelClosed,
    #[error("JSON error: {}", .0)]
    JsonError(#[from] serde_json::Error),
    #[error("reqwest error: {}", .0)]
    Reqwest(#[from] reqwest::Error),
    #[error("Status: {}, Message: {}", .0, .1)]
    RequestFailed(reqwest::StatusCode, String),
}

impl<T: DeserializeOwned> Promise<T> {
    pub fn empty(marker: &'static str) -> Self {
        Self {
            state: PromiseState::None,
            marker,
            seen: false,
        }
    }

    pub fn new_ready(value: T, marker: &'static str) -> Self {
        Self {
            state: PromiseState::Done(Some(value)),
            marker,
            seen: false,
        }
    }

    pub fn waiting(rx: Receiver<Result<Vec<u8>, Error>>, marker: &'static str) -> Self {
        Self {
            state: PromiseState::Waiting(rx),
            marker,
            seen: false,
        }
    }

    pub fn error(error: Error, marker: &'static str) -> Self {
        Self {
            state: PromiseState::Err(error),
            marker,
            seen: false,
        }
    }

    /// Polls the promise and either returns a reference to the data or [None] if still pending.
    /// Note that error is also an option, but this method ignores it.
    pub fn ready(&mut self) -> Option<&T> {
        self.sync_poll();
        self.ready_no_poll()
    }

    pub fn ready_no_poll(&self) -> Option<&T> {
        if let PromiseState::Done(response) = &self.state {
            response.as_ref()
        } else {
            None
        }
    }

    pub fn ready_mut(&mut self) -> Option<&mut T> {
        self.sync_poll();
        if let PromiseState::Done(response) = &mut self.state {
            response.as_mut()
        } else {
            None
        }
    }

    pub fn ready_if_unseen(&mut self) -> Option<&T> {
        self.sync_poll();
        if let PromiseState::Done(response) = &self.state {
            if self.seen {
                None
            } else {
                self.seen = true;
                response.as_ref()
            }
        } else {
            None
        }
    }

    pub fn take_ready(&mut self) -> Option<T> {
        self.sync_poll();
        if !matches!(self.state, PromiseState::Done(_)) {
            return None;
        }
        if let PromiseState::Done(ref mut response) = core::mem::take(&mut self.state) {
            response.take()
            // Some(response)
        } else {
            None
        }
    }

    pub fn take(&mut self) -> Result<Option<T>, String> {
        if let Some(r) = self.take_ready() {
            Ok(Some(r))
        } else {
            if let Some(e) = self.peek_error() {
                self.state = PromiseState::None;
                Err(e)
            } else {
                Ok(None)
            }
        }
    }

    pub fn peek_error(&mut self) -> Option<String> {
        self.sync_poll();
        if let PromiseState::Err(e) = &self.state {
            Some(format!("{e:?}"))
        } else {
            None
        }
    }

    pub fn sync_poll(&mut self) {
        if let PromiseState::Waiting(rx) = &mut self.state {
            match rx.try_recv() {
                Ok(response) => self.handle_response(response),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Closed) => {
                    self.state = PromiseState::Err(RequestChannelClosed);
                }
            }
        }
    }

    pub async fn poll(&mut self) {
        if let PromiseState::Waiting(rx) = &mut self.state {
            match rx.await {
                Ok(response) => self.handle_response(response),
                Err(_) => {
                    self.state = PromiseState::Err(RequestChannelClosed);
                }
            }
        }
    }

    fn handle_response(&mut self, result: Result<Vec<u8>, Error>) {
        self.state = match result.and_then(|bytes| Ok(serde_json::from_slice(&bytes)?)) {
            Ok(response) => PromiseState::Done(Some(response)),
            Err(e) => PromiseState::Err(e),
        };
    }
}

impl<T> Promise<T> {
    pub fn is_waiting(&self) -> bool {
        matches!(self.state, PromiseState::Waiting(_))
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.state, PromiseState::None)
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.state, PromiseState::Done(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self.state, PromiseState::Err(_))
    }

    pub fn state(&self) -> &PromiseState<T> {
        &self.state
    }

    pub fn marker(&self) -> &str {
        self.marker
    }
}

impl<T> Display for Promise<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Promise('{}')::", self.marker)?;
        match &self.state {
            PromiseState::None => write!(f, "None"),
            PromiseState::Waiting(_) => write!(f, "Waiting"),
            PromiseState::Done(_) => write!(f, "Done"),
            PromiseState::Err(e) => write!(f, "Err({e:?})"),
        }
    }
}

impl<T> Drop for Promise<T> {
    fn drop(&mut self) {
        if matches!(self.state, PromiseState::Waiting(_)) {
            warn!(
                "Dropping Promise('{}')::Waiting(T), likely an error",
                self.marker
            );
        }
    }
}
