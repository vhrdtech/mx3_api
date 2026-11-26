//! Crawler watches (LAB-8): parts the crawler refreshes on an interval, listed, shown and stopped. A watch is per
//! part id; [`crate::endpoints::warehouse::WatchParts`] sets it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "client")]
use crate::blocking::BlockingClient;
use crate::endpoints::part::PartId;

pub const EP_PART_WATCHED: &str = "/part/watched";
pub const EP_PART_UNWATCH: &str = "/part/unwatch";

/// The most parts one [`Watched`] or [`Unwatch`] names.
pub const WATCH_PARTS_MAX: usize = 1000;

/// Asks for watched parts: these, or every watched part when `part_ids` is empty.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Watched {
    #[serde(default)]
    pub part_ids: Vec<PartId>,
}

/// One watched part.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartWatch {
    pub part: PartId,
    pub mpn: String,
    /// The company's own name (a synonym's part named by its company)
    pub maker: String,
    /// How often the crawler refreshes it
    pub every_secs: i64,
    /// The last Digi-Key crawl; `None`: never crawled, due now
    pub last_crawl: Option<DateTime<Utc>>,
    /// When it is due again: `last_crawl` plus the interval, `None` when due now
    pub next_crawl: Option<DateTime<Utc>>,
    /// The last crawl failed: the crawler retries it as a failed part, not as a watch
    pub crawl_failed: bool,
}

/// The answer to [`Watched`]: the watched parts by id; a named part that is not watched is left out.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WatchedParts {
    pub watches: Vec<PartWatch>,
}

/// Stops the crawler's watch of these parts (lab:write); their data stays.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Unwatch {
    pub part_ids: Vec<PartId>,
}

/// The answer to [`Unwatch`]: the parts whose watch stopped; a named part that was not watched is left out.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Unwatched {
    pub part_ids: Vec<PartId>,
}

#[cfg(feature = "client")]
pub fn watched(c: &BlockingClient, q: &Watched) -> anyhow::Result<WatchedParts> {
    c.post_json(q, EP_PART_WATCHED)
}

#[cfg(feature = "client")]
pub fn unwatch(c: &BlockingClient, q: &Unwatch) -> anyhow::Result<Unwatched> {
    c.post_json(q, EP_PART_UNWATCH)
}
