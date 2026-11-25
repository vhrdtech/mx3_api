use crate::endpoints::bom::BomId;
use crate::endpoints::part::PartId;
#[cfg(feature = "client")]
use crate::request::RequestInitiator;
#[cfg(feature = "client")]
use crate::{MxClient, Promise};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

/// A supplier the crawler looks parts up at (INT-12, INT-15).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Supplier {
    DigiKey,
    Mouser,
    Lcsc,
}

impl Supplier {
    pub const ALL: [Supplier; 3] = [Supplier::DigiKey, Supplier::Mouser, Supplier::Lcsc];

    pub fn name(&self) -> &'static str {
        match self {
            Supplier::DigiKey => "Digi-Key",
            Supplier::Mouser => "Mouser",
            Supplier::Lcsc => "LCSC",
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum CrawlerCommand {
    /// Every supplier's crawl
    Pause,
    Resume,
    Status,
    /// Digi-Key's pause between crawls when nothing is queued
    SetConfig {
        interval: Option<Duration>,
    },
    /// Crawl these parts again at every supplier: Digi-Key first, Mouser and LCSC after it (errors of earlier crawls
    /// cleared)
    Crawl(HashSet<PartId>),
    /// Crawl these parts again at one supplier only (INT-12)
    CrawlAt {
        supplier: Supplier,
        parts: HashSet<PartId>,
    },
    PauseOnly(Supplier),
    ResumeOnly(Supplier),
}

/// What the crawler does per supplier (INT-12), `GET crawler_status`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CrawlerStatus {
    /// Whether the crawler runs at all on this backend (`DISABLE_CRAWLER`)
    pub running: bool,
    pub suppliers: Vec<SupplierCrawlStatus>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SupplierCrawlStatus {
    pub supplier: Supplier,
    /// Off by configuration (`MX3_MOUSER_OFF`, `MX3_LCSC_OFF`)
    pub off: bool,
    pub paused: bool,
    pub key_set: bool,
    /// Parts waiting for a look-up at this supplier
    pub queued: i64,
    /// Parts whose last look-up failed (`crawler failed` lists them); Digi-Key and LCSC try them again by themselves
    /// after some days, Mouser waits until they are queued again
    pub errors: i64,
    /// Calls made today (UTC) and the daily quota, where the crawler counts them
    pub calls_today: Option<u32>,
    pub calls_per_day: Option<u32>,
    /// The calls a day the crawler may make: `calls_per_day` less the reserve it leaves for interactive use
    /// (CRAWL-8); `None` from a backend before it, or without a daily limit
    pub crawler_share: Option<u32>,
    /// Why it waits right now (quota used, key refused, no network, ...) and until when
    pub waiting: Option<String>,
    /// The last thing it did
    pub last: Option<String>,
}

/// `POST crawler_retry_failed` (CRAWL-2): clear the errors of every failed part, at one supplier or all, so they are
/// crawled again.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RetryFailed {
    /// Only this supplier's failures; `None`: every supplier's
    pub supplier: Option<Supplier>,
}

/// How many failed parts were queued again, per supplier.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RetryFailedResult {
    pub requeued: Vec<SupplierCount>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SupplierCount {
    pub supplier: Supplier,
    pub parts: i64,
}

/// `POST crawler_failed` (CRAWL-2): the parts whose last crawl failed, at one supplier or all.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ListFailed {
    pub supplier: Option<Supplier>,
}

/// A part whose last crawl at `supplier` failed for a reason about the part.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FailedPart {
    pub supplier: Supplier,
    pub part: PartId,
    pub mpn: String,
    /// The part's manufacturer ("Unknown" when it has none)
    pub maker: String,
    /// Why; `None` for a part that failed before reasons were recorded
    pub reason: Option<String>,
    /// When it failed
    pub at: Option<DateTime<Utc>>,
    /// Failed crawls in a row (Digi-Key and LCSC)
    pub attempts: Option<i32>,
    /// When the crawler tries it again by itself (Digi-Key and LCSC); `None`: only after `crawler retry-failed`
    pub next_retry: Option<DateTime<Utc>>,
    /// What Digi-Key suggests instead (INT-18): substitutes, near MPNs, cross-references
    #[serde(default)]
    pub suggestions: Vec<crate::endpoints::substitutes::Suggestion>,
}

/// `POST crawler_queue` (CRAWL-6): the parts each supplier's crawler takes next, in its order.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QueueQuery {
    /// Only this supplier's queue; `None`: every supplier's
    pub supplier: Option<Supplier>,
    /// At most this many parts per supplier; `None`: all of them
    pub top: Option<u32>,
}

/// One supplier's queue: how many parts wait in all and the first of them.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SupplierQueue {
    pub supplier: Supplier,
    pub queued: i64,
    pub parts: Vec<QueuedPart>,
}

/// A part waiting at a supplier's crawler, with its priority and why (CRAWL-6).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QueuedPart {
    pub part: PartId,
    pub mpn: String,
    pub maker: String,
    /// The `crawl_priority` score: higher first
    pub score: f64,
    /// The kind of part (`crawl_kind`): ic, resistor_chip, ...
    pub kind: String,
    /// Why it has that score, one reason per line
    pub why: Vec<String>,
    /// When this supplier last looked it up; `None`: never
    pub crawled: Option<DateTime<Utc>>,
    /// The crawler's own condition put it behind the others (a retry after a failure)
    pub retry: bool,
}

/// `POST crawler_bom_front` (CRAWL-6, `mx3 bom priority`): move a BOM's parts to the front of every crawler's queue.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BomFront {
    pub bom: BomId,
}

/// How many of the BOM's parts wait at each supplier.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BomFrontResult {
    pub queued: Vec<SupplierCount>,
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn crawler_cmd(&self, cmd: &CrawlerCommand, marker: &'static str) -> Promise<()> {
        let (req, rx) = RequestInitiator::post_json_arg("crawler_cmd".into(), cmd);
        self.send_req(req, rx, marker)
    }

    pub fn crawler_status(&self, marker: &'static str) -> Promise<CrawlerStatus> {
        let (req, rx) = RequestInitiator::get_no_args("crawler_status".into());
        self.send_req(req, rx, marker)
    }

    pub fn crawler_retry_failed(
        &self,
        q: &RetryFailed,
        marker: &'static str,
    ) -> Promise<RetryFailedResult> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_RETRY_FAILED.into(), q);
        self.send_req(req, rx, marker)
    }

    pub fn crawler_failed(&self, q: &ListFailed, marker: &'static str) -> Promise<Vec<FailedPart>> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_FAILED.into(), q);
        self.send_req(req, rx, marker)
    }

    /// Every supplier API's quota with what is used of it (CRAWL-7).
    pub fn crawler_quota(&self, marker: &'static str) -> Promise<Vec<SupplierQuota>> {
        let (req, rx) = RequestInitiator::get_no_args(EP_QUOTA.trim_start_matches('/').into());
        self.send_req(req, rx, marker)
    }

    /// Change a supplier API's quota (CRAWL-7, lab:write).
    pub fn crawler_quota_set(&self, q: &SetQuota, marker: &'static str) -> Promise<SetQuotaResult> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_QUOTA_SET.into(), q);
        self.send_req(req, rx, marker)
    }
}

/// `GET crawler_quota` (CRAWL-7, lab:read): every supplier API's quota as this backend uses it, with what is used.
pub const EP_QUOTA: &str = "/crawler_quota";
/// `POST crawler_quota_set` (CRAWL-7, lab:write): change one provider's quota, logged with who and when.
pub const EP_QUOTA_SET: &str = "/crawler_quota_set";

/// One supplier API's quota (CRAWL-7): the `supplier_quota` row with the env vars of the backend's host laid over it,
/// and what is used of it now.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SupplierQuota {
    /// `digikey`, `digikey-search`, `mouser`, `lcsc`, `octopart`
    pub provider: String,
    /// When the provider's day starts, in UTC
    pub day_start_utc: NaiveTime,
    /// Calls a provider's day at most; `None`: no daily limit
    pub per_day: Option<u32>,
    /// Calls a minute at most; `None`: no pace of its own
    pub per_minute: Option<u32>,
    /// Units a budget period at most (Octopart's matched parts); `None`: no budget per period
    pub per_period: Option<u32>,
    /// The day of the month a budget period starts again; 0: never
    pub reset_day: u32,
    /// Calls of `per_day` the crawlers leave for interactive use (CRAWL-8); interactive callers may use them all
    #[serde(default)]
    pub crawler_reserve: u32,
    /// The calls a day the crawlers may make, `per_day` less the reserve; `None` without a daily limit
    #[serde(default)]
    pub crawler_share: Option<u32>,
    pub note: String,
    /// The last change of the row and who made it; `None`: no row, the backend's built-in values
    pub updated_at: Option<DateTime<Utc>>,
    pub updated_by: String,
    /// Env vars of the backend's host that win over the row
    pub overrides: Vec<QuotaOverride>,
    /// Calls made in the provider's current day, or units used in the budget period
    pub used: u32,
    /// The first day of what `used` counts: the provider's day, or the budget period
    pub used_since: NaiveDate,
    /// When the count starts again; `None`: a budget that never resets
    pub next_reset: Option<DateTime<Utc>>,
}

/// An env var that sets a quota field instead of its row (CRAWL-7).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct QuotaOverride {
    /// The field: `day_start_utc`, `per_day`, `per_minute`, `per_period`, `reset_day`, `crawler_reserve`
    pub field: String,
    pub var: String,
    pub value: String,
    /// The row's value it hides, as text; `None` for no limit
    pub stored: Option<String>,
}

/// A limit to set: a number, or none at all.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuotaLimit {
    None,
    At(u32),
}

impl QuotaLimit {
    pub fn get(self) -> Option<u32> {
        match self {
            QuotaLimit::None => None,
            QuotaLimit::At(n) => Some(n),
        }
    }
}

/// `POST crawler_quota_set` (CRAWL-7): the fields to change; `None` leaves a field as it is.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SetQuota {
    pub provider: String,
    pub day_start_utc: Option<NaiveTime>,
    pub per_day: Option<QuotaLimit>,
    pub per_minute: Option<QuotaLimit>,
    pub per_period: Option<QuotaLimit>,
    pub reset_day: Option<u32>,
    /// Calls of the day the crawlers leave for interactive use (CRAWL-8)
    #[serde(default)]
    pub crawler_reserve: Option<u32>,
    pub note: Option<String>,
}

/// What changed, and the quota as it is now.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetQuotaResult {
    pub changes: Vec<QuotaChange>,
    pub quota: SupplierQuota,
}

/// One changed field, as `supplier_quota_log` keeps it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct QuotaChange {
    pub field: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

pub const EP_RETRY_FAILED: &str = "/crawler_retry_failed";
pub const EP_FAILED: &str = "/crawler_failed";
pub const EP_QUEUE: &str = "/crawler_queue";
pub const EP_BOM_FRONT: &str = "/crawler_bom_front";
