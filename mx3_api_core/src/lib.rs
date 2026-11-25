//! Wire types shared by every domain of the mx3 API: the body of a failed answer, money, the file export report
//! and the API level. With no features the crate is serde types only.
//!
//! Features (all off by default):
//! - `client`: [`client_headers`], the headers every mx3 client sends.
//! - `sqlx`: conversions between [`money::Money`] and PostgreSQL's `money`.

pub mod base64_bytes;
pub mod error;
pub mod files;
pub mod money;

pub use error::error_message;

/// The wire level of the API (PRM-5): raised when a client of an older level can no longer talk to the backend.
///
/// Level 2: JSON on every endpoint (PLT-23) and parameters as facts. Clients send it in [`API_LEVEL_HEADER`]; the
/// backend answers an older one, and any msgpack request (level 1 sent no header), with 426 and "update mx3".
pub const API_LEVEL: u32 = 2;

/// The header clients send their [`API_LEVEL`] in.
pub const API_LEVEL_HEADER: &str = "x-mx3-api";

/// The headers every request of an mx3 client carries: [`API_LEVEL_HEADER`] and a `User-Agent` naming this crate's
/// version.
#[cfg(feature = "client")]
pub fn client_headers() -> reqwest::header::HeaderMap {
    use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
    let mut h = HeaderMap::new();
    h.insert(API_LEVEL_HEADER, HeaderValue::from(API_LEVEL));
    h.insert(
        USER_AGENT,
        HeaderValue::from_static(concat!("mx3/", env!("CARGO_PKG_VERSION"))),
    );
    h
}
