//! Footprint reviews (PART-6).
//!
//! Footprint reviews (PART-6): a person checked that the footprint with a review hash (`better_erc` `berc fp hash`,
//! `fp1:` and 16 hex digits) fits the land pattern and package of some parts, against one document. A review covers
//! single parts (`mpn`) or MPN families (`family`: every part whose current MPN decode reading, MPN-1, is of one
//! scheme with fixed values such as `case_eia=0402`). Reviews are never deleted, only retracted; a retracted review
//! covers nothing. `better_erc` links these types for its parts snapshot.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "client")]
use crate::blocking::BlockingClient;
use crate::endpoints::part::PartId;

/// Which reviews cover each of some MPNs under one footprint hash (lab:read)
pub const EP_FP_REVIEW_CHECK: &str = "/fp/review/check";
/// Reviews by hash or part, or all (lab:read)
pub const EP_FP_REVIEW_LIST: &str = "/fp/review/list";
/// Record a review (lab:write)
pub const EP_FP_REVIEW_ACCEPT: &str = "/fp/review/accept";
/// Retract a review (lab:write)
pub const EP_FP_REVIEW_RETRACT: &str = "/fp/review/retract";

/// Longest hash accepted: `fp` and a version, a colon, at most 64 hex digits
pub const HASH_LEN_MAX: usize = 80;

/// A review hash as `better_erc` writes it: `fp<version>:` then 16 to 64 lowercase hex digits.
pub fn valid_hash(s: &str) -> bool {
    let Some((version, hex)) = s.split_once(':') else {
        return false;
    };
    let Some(version) = version.strip_prefix("fp") else {
        return false;
    };
    s.len() <= HASH_LEN_MAX
        && !version.is_empty()
        && !version.starts_with('0')
        && version.bytes().all(|b| b.is_ascii_digit())
        && (16..=64).contains(&hex.len())
        && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Where the footprint lay on the PDF page while it was reviewed
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FpPlacement {
    /// Origin in page points from the page's top left
    pub x: f64,
    pub y: f64,
    /// Degrees
    #[serde(default)]
    pub rotation: f64,
    /// Seen from below
    #[serde(default)]
    pub mirror: bool,
}

/// Where a review was made: the document and the spot on it
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FpSource {
    /// Document number (`DS30154`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// Its revision (`rev16`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_rev: Option<String>,
    /// The PDF's blake3, 16 to 64 lowercase hex digits (the document library's `b3:` hash without the prefix)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf_blake3: Option<String>,
    /// The page the land pattern is on, 1-based
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// mm per PDF point
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<FpPlacement>,
}

/// What one review covers
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FpCover {
    /// One part; `mpn` is its MPN when the review was made
    Mpn { part_id: PartId, mpn: String },
    /// Every part whose current MPN decode reading is of `scheme` with these values (scheme key → value as the
    /// scheme writes it: `case_eia` → `0402`)
    Family {
        scheme: String,
        segments: BTreeMap<String, String>,
    },
}

/// A cover to record: a part by id or by MPN, or a family
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FpCoverSpec {
    /// A part: `part_id` when given, else the one part with this MPN (refused when none or several)
    Mpn {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        mpn: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part_id: Option<PartId>,
    },
    /// The scheme must be loaded and every key one of its parameters; at least one value
    Family {
        scheme: String,
        segments: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FpRetraction {
    pub at: DateTime<Utc>,
    pub by: String,
    pub why: String,
}

/// One review as stored
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FpReview {
    pub id: i32,
    pub hash: String,
    /// `Lib:Name` at review time, for display
    pub footprint: String,
    #[serde(flatten)]
    pub source: FpSource,
    pub reviewer: String,
    pub reviewed_on: NaiveDate,
    pub note: String,
    pub covers: Vec<FpCover>,
    /// When it was recorded, and by which API client
    pub recorded: DateTime<Utc>,
    pub by: String,
    /// None while it holds
    pub retracted: Option<FpRetraction>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FpReviewAccept {
    pub hash: String,
    #[serde(default)]
    pub footprint: String,
    #[serde(flatten)]
    pub source: FpSource,
    /// Who checked it; default the client's actor
    #[serde(default)]
    pub reviewer: Option<String>,
    /// Default today
    #[serde(default)]
    pub reviewed_on: Option<NaiveDate>,
    #[serde(default)]
    pub note: String,
    /// At least one
    pub covers: Vec<FpCoverSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FpAccepted {
    pub review: FpReview,
    /// Per cover, in order: how many parts it covers now (a family that covers none is likely a typo)
    pub parts_now: Vec<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FpReviewRetract {
    pub id: i32,
    pub why: String,
}

/// Reviews of one hash, or covering one part, or (neither) all; retracted ones only with `retracted`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FpReviewListQuery {
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub part: Option<PartId>,
    #[serde(default)]
    pub retracted: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FpCheckQuery {
    pub hash: String,
    pub mpns: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FpCheckStatus {
    /// A review of this hash covers the part
    Covered,
    /// The part is known, no review of this hash covers it
    Uncovered,
    /// No part has this MPN
    UnknownMpn,
}

/// One part of a checked MPN (an MPN several makers use gives several items)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FpCheckItem {
    /// As asked
    pub mpn: String,
    pub part_id: Option<PartId>,
    pub maker: Option<String>,
    pub status: FpCheckStatus,
    /// Ids of the reviews of this hash that cover the part
    pub covered_by: Vec<i32>,
    /// Ids of reviews of other hashes that cover the part: the footprint changed since, or another one was reviewed
    pub other_hashes: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FpCheck {
    pub hash: String,
    pub items: Vec<FpCheckItem>,
    /// Every review an item names, by id
    pub reviews: Vec<FpReview>,
}

#[cfg(feature = "client")]
pub fn check(c: &BlockingClient, q: &FpCheckQuery) -> anyhow::Result<FpCheck> {
    c.post_json(q, EP_FP_REVIEW_CHECK)
}

#[cfg(feature = "client")]
pub fn list(c: &BlockingClient, q: &FpReviewListQuery) -> anyhow::Result<Vec<FpReview>> {
    c.post_json(q, EP_FP_REVIEW_LIST)
}

#[cfg(feature = "client")]
pub fn accept(c: &BlockingClient, q: &FpReviewAccept) -> anyhow::Result<FpAccepted> {
    c.post_json(q, EP_FP_REVIEW_ACCEPT)
}

#[cfg(feature = "client")]
pub fn retract(c: &BlockingClient, q: &FpReviewRetract) -> anyhow::Result<FpReview> {
    c.post_json(q, EP_FP_REVIEW_RETRACT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_hashes() {
        assert!(valid_hash("fp1:7726812df851763a"));
        assert!(valid_hash("fp12:7726812df851763a7726812df851763a"));
        assert!(!valid_hash("fp1:7726812DF851763A"));
        assert!(!valid_hash("fp1:7726812df85176"));
        assert!(!valid_hash("fp0:7726812df851763a"));
        assert!(!valid_hash("fp:7726812df851763a"));
        assert!(!valid_hash("7726812df851763a"));
        assert!(!valid_hash("fp1:7726812df851763g"));
    }

    #[test]
    fn covers_are_tagged_by_kind() {
        let c: FpCoverSpec = serde_json::from_str(
            r#"{"kind":"family","scheme":"yageo.rc","segments":{"case_eia":"0402"}}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            FpCoverSpec::Family {
                scheme: "yageo.rc".into(),
                segments: [("case_eia".to_owned(), "0402".to_owned())].into()
            }
        );
        let c: FpCoverSpec = serde_json::from_str(r#"{"kind":"mpn","mpn":"BSS138W-7-F"}"#).unwrap();
        assert_eq!(
            c,
            FpCoverSpec::Mpn {
                mpn: "BSS138W-7-F".into(),
                part_id: None
            }
        );
    }
}
