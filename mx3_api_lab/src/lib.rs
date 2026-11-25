//! The electronics lab API of mx3: parts and their parameters, MPN schemes, BOMs, PCBs and serials, footprint
//! reviews, suppliers and their search, the warehouse and orders. Request and response types for every endpoint,
//! shared by the backend and its clients, plus the BOM logic clients share (filters, cost, import, assembly).
//!
//! Features (all off by default; with none the crate is serde types and pure logic only):
//! - `client`: `blocking::BlockingClient` and the async `MxClient`, whose calls answer with a polled
//!   `Promise`. reqwest with rustls; the async client needs a tokio runtime.
//! - `sqlx`: `sqlx::Type` on the id newtypes and enums, for a backend built on sqlx and PostgreSQL.

pub use mx3_api_core;
pub use rvariant;

#[cfg(feature = "client")]
pub use mx_client::MxClient;
#[cfg(feature = "client")]
pub use promise::Promise;

pub mod bom;
pub mod endpoints;
pub mod library;
pub mod orders;
pub mod supplier_search;
pub mod warehouse;

#[cfg(feature = "client")]
pub mod blocking;
#[cfg(feature = "client")]
pub mod blocking_api;
#[cfg(feature = "client")]
pub mod mx_client;
#[cfg(feature = "client")]
pub mod promise;
#[cfg(feature = "client")]
pub mod request;

#[cfg(feature = "client")]
pub(crate) mod prelude {
    pub(crate) use crate::mx_client::send_req;
    pub(crate) use crate::promise::Promise;
    pub(crate) use crate::request::RequestInitiator;
    pub(crate) use tokio::sync::mpsc::UnboundedSender;
}
