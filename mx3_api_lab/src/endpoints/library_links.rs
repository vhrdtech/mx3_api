//! The library's part-number links (LIB-1).
//!
//! A link says which content a fetch by part number saved or found for an MPN, and the URL it came from. The server
//! keeps one link per MPN and content; [`super::library::ByMpnQuery`] reads them.
//!
//! Scope: `lib:write`.

use serde::{Deserialize, Serialize};

#[cfg(feature = "client")]
use crate::blocking::BlockingClient;

/// Record the contents part numbers led to; a known link is kept (lib:write)
pub const EP_LIB_LINKS: &str = "/lib/links";

/// Links per [`Links`]
pub const LINKS_MAX: usize = 5000;

/// One part number's content and the URL it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewLink {
    /// The part number, 1 to [`super::library::MPN_BYTES_MAX`] bytes
    pub mpn: String,
    /// The content's hash
    pub hash: String,
    /// The `http` or `https` URL the content came from
    pub url: String,
}

/// Links to record, at most [`LINKS_MAX`]; the server names them by the token's computer and the time it received
/// them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Links {
    pub links: Vec<NewLink>,
}

/// What the server did with [`Links`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinksAdded {
    /// Links new to the library
    pub added: usize,
    /// Links the library held already, kept as they were
    pub known: usize,
}

/// Record links; the whole request fails when one is not a part number, a content hash and a web address.
#[cfg(feature = "client")]
pub fn add_links(c: &BlockingClient, q: &Links) -> anyhow::Result<LinksAdded> {
    c.post_json(q, EP_LIB_LINKS)
}
