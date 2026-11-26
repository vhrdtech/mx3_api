//! Maker-less part stubs and their merge into the part with a maker (LAB-9, LAB-10).
//!
//! One part per maker: a stub (maker "Unknown") beside a part of the same MPN with a maker is the same part twice. The merge moves what the
//! stub holds to the part and deletes the stub; the merge record keeps where things came from.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "client")]
use crate::blocking::BlockingClient;
use crate::endpoints::part::{MpnMaker, PartId};

pub const EP_PART_STUBS: &str = "/part/stubs";
pub const EP_PART_MERGE: &str = "/part/merge";

/// What a part holds that a merge moves, by kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartHoldings {
    /// Pieces in boxes (the sum of its box items' counts)
    pub stock: i64,
    pub box_items: i64,
    /// Moves of its pieces between boxes, into serials
    pub box_moves: i64,
    /// BOM lines that choose it
    pub bom_choices: i64,
    /// BOM lines that list it as a candidate
    pub bom_candidates: i64,
    /// Parameter facts, superseded ones included
    pub facts: i64,
    /// Rank overrides scoped to it
    pub ranks: i64,
    /// Photos and other files
    pub files: i64,
    /// Supply rows (supplier offers)
    pub supply: i64,
    pub order_items: i64,
    /// Footprint reviews that cover it by MPN
    pub fp_reviews: i64,
    /// The crawler watches it
    pub watched: bool,
}

impl PartHoldings {
    /// Holds nothing a merge would move.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Asks for the maker-less parts whose MPN a part with a maker has (the merge candidates).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stubs {
    /// Only these MPNs (exact, ignoring case); empty: every stub
    #[serde(default)]
    pub mpns: Vec<String>,
}

/// One maker-less part and the parts with a maker it could merge into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartStub {
    pub stub: PartId,
    pub mpn: String,
    pub holds: PartHoldings,
    /// A frozen BOM revision names it: it can not be merged (BOM-28)
    pub frozen: bool,
    /// The parts of the same MPN with a maker
    pub into: Vec<MpnMaker>,
}

/// The answer to [`Stubs`], by MPN then stub id.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartStubs {
    pub stubs: Vec<PartStub>,
}

/// Merges the maker-less part `stub` into `into` (lab:write): everything it holds moves, then it is deleted.
///
/// Refused when `stub` has a maker, the MPNs differ (ignoring case), the two are one part, or a frozen BOM revision
/// names the stub. `dry_run` counts and changes nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePart {
    pub stub: PartId,
    pub into: PartId,
    #[serde(default)]
    pub dry_run: bool,
}

/// The answer to [`MergePart`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartMerged {
    pub stub: PartId,
    pub into: PartId,
    pub mpn: String,
    /// What moved (a dry run: what would move)
    pub moved: PartHoldings,
    /// The stub's current facts `into` already has from the same source: kept as history, superseded
    pub facts_superseded: i64,
    /// BOM lines that named both: the stub's row dropped, the line keeps `into` (a real choice when either was)
    pub bom_lines_joined: i64,
    pub dry_run: bool,
    /// The merge record's id and time; `None` on a dry run
    pub merge_id: Option<i32>,
    pub at: Option<DateTime<Utc>>,
}

#[cfg(feature = "client")]
pub fn stubs(c: &BlockingClient, q: &Stubs) -> anyhow::Result<PartStubs> {
    c.post_json(q, EP_PART_STUBS)
}

#[cfg(feature = "client")]
pub fn merge(c: &BlockingClient, q: &MergePart) -> anyhow::Result<PartMerged> {
    c.post_json(q, EP_PART_MERGE)
}
