//! Manufacturer part-number schemes (docs/mpn-schemes.md): the corpus of every part's maker, MPN and curated values
//! for the `mpn_schemes` corpus test (MPN-2); the loaded scheme set, an MPN read segment by segment, and parts
//! decoded into facts of the source `mpn_decode` (MPN-1, §7). Decoded values travel as the scheme crate's stable
//! text form (`100nF`, `±10%`, `0402`, `reel_7in_paper`), which the source's aliases read into each definition's
//! type.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[cfg(feature = "client")]
use crate::blocking::BlockingClient;
use crate::endpoints::part::PartId;

pub const EP_MPN_CORPUS: &str = "/mpn/corpus";

/// A page of the corpus: parts after `after` by id, at most `limit` (the backend caps it).
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct CorpusQuery {
    #[serde(default)]
    pub after: Option<PartId>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CorpusPage {
    pub parts: Vec<CorpusPart>,
    /// Ask again with `after` = this for the next page; none = that was the last
    pub next: Option<PartId>,
}

/// One line of the corpus file: the keys are curated definitions only.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CorpusPart {
    pub maker: String,
    pub mpn: String,
    pub part_id: i32,
    /// Curated key → the value as its winning source stated it (the source's own text)
    pub params: BTreeMap<String, String>,
    /// Curated key → key of the source that stated it
    pub sources: BTreeMap<String, String>,
}

impl CorpusPart {
    /// The class file the part goes to, from the curated keys it has: `capacitor`, `resistor`, `inductor` or `other`.
    pub fn class(&self) -> &'static str {
        let has = |k: &str| self.params.contains_key(k);
        if has("capacitance") {
            "capacitor"
        } else if has("resistance") {
            "resistor"
        } else if has("inductance") {
            "inductor"
        } else {
            "other"
        }
    }
}

#[cfg(feature = "client")]
pub fn corpus(c: &BlockingClient, q: &CorpusQuery) -> anyhow::Result<CorpusPage> {
    c.post_json(q, EP_MPN_CORPUS)
}

/// The loaded schemes, where each came from and what the overlay refused (lab:read)
pub const EP_MPN_SCHEMES: &str = "/mpn/schemes";
/// Read the overlay directory again (lab:write)
pub const EP_MPN_SCHEMES_RELOAD: &str = "/mpn/schemes/reload";
/// Every reading of one MPN, best first (lab:read)
pub const EP_MPN_PARSE: &str = "/mpn/parse";
/// Decode parts into `mpn_decode` facts, a dry run unless `apply` (lab:read; `apply` needs lab:write)
pub const EP_MPN_DECODE: &str = "/mpn/decode";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MpnSchemes {
    pub schemes: Vec<MpnScheme>,
    /// `MX3_MPN_SCHEMES_DIR`, when set
    pub overlay_dir: Option<String>,
    /// What the overlay did, one line per file's scheme (`murata.grm v2 replaces v1 (...)`)
    pub overlay_notes: Vec<String>,
    /// Files the overlay refused, with their errors; the embedded version stays in use
    pub errors: Vec<String>,
    /// The scheme ids and versions in one text: decodes made under another one are stale
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpnScheme {
    pub id: String,
    pub version: u32,
    pub maker: String,
    pub class: String,
    pub title: String,
    /// `embedded` or the overlay file
    pub from: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MpnParseQuery {
    pub mpn: String,
    /// The maker's name, which ranks its schemes first
    #[serde(default)]
    pub maker: Option<String>,
}

/// One reading of an MPN by one scheme
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpnParse {
    pub scheme: String,
    pub version: u32,
    pub maker: String,
    pub class: String,
    /// The whole MPN was read; a partial reading left `rest`
    pub complete: bool,
    pub rest: String,
    pub params: Vec<MpnParam>,
    pub segments: Vec<MpnSegment>,
    /// `not_in_table`, `outside_series:<segment>`, `absent:<segment>`
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MpnParam {
    pub key: String,
    pub value: String,
    /// None for a fact the scheme implies
    pub segment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpnSegment {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub code: String,
    pub key: Option<String>,
    pub value: Option<String>,
    pub free: bool,
}

/// The reading an MPN would be decoded under, the way `mpn_decode` facts choose one (MPN-1, `mpn::decode::decide`):
/// a complete reading by a scheme of the known maker, with no equally good rival. Used by `params/by_mpn` (PRM-17)
/// so a caller sees the same scheme id, version and segments a decoded fact would carry, even for an MPN mx3 holds
/// no part for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpnReading {
    /// `decoded`, `no_scheme`, `partial`, `ambiguous`, `maker_mismatch`
    pub outcome: String,
    /// Set only when `outcome` is `decoded`
    #[serde(default)]
    pub scheme: Option<String>,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub segments: Vec<MpnSegment>,
    /// The schemes considered but not chosen (partial, ambiguous, or of another maker)
    #[serde(default)]
    pub schemes: Vec<String>,
}

/// Which parts to decode. `parts` alone, or every part (`all`) or those decoded under another scheme set (`stale`),
/// a page at a time from `after`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MpnDecodeQuery {
    #[serde(default)]
    pub parts: Vec<PartId>,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub stale: bool,
    /// Write the facts; without it nothing is written
    #[serde(default)]
    pub apply: bool,
    /// Parts after this id (the previous page's `next`)
    #[serde(default)]
    pub after: Option<PartId>,
    /// Parts per page; 0: the backend's default
    #[serde(default)]
    pub limit: u32,
    /// Answer with every part's outcome, not only the ones worth a look (ambiguous, maker mismatch)
    #[serde(default)]
    pub items: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MpnDecodeReport {
    pub applied: bool,
    pub parts: usize,
    /// Parts per outcome (`decoded`, `no_scheme`, `partial`, `ambiguous`, `maker_mismatch`)
    pub outcomes: BTreeMap<String, usize>,
    /// Decoded parts per scheme id
    pub per_scheme: BTreeMap<String, usize>,
    /// Decoded values per parameter key
    pub per_param: BTreeMap<String, usize>,
    /// Decoded values per key that no definition takes (no alias of `mpn_decode`, no definition with that key)
    pub no_def: BTreeMap<String, usize>,
    /// With `apply`: facts written new or changed, kept as they were, retracted
    pub facts_changed: usize,
    pub facts_unchanged: usize,
    pub facts_retracted: usize,
    pub items: Vec<MpnDecodeItem>,
    /// The last part of this page, when more may follow
    pub next: Option<PartId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpnDecodeItem {
    pub part: PartId,
    pub mpn: String,
    pub maker: String,
    pub outcome: String,
    /// The schemes involved: the one decoded, or the competing ones
    pub schemes: Vec<String>,
    pub params: Vec<MpnParam>,
}

#[cfg(feature = "client")]
pub fn schemes(c: &BlockingClient) -> anyhow::Result<MpnSchemes> {
    c.post_json(&(), EP_MPN_SCHEMES)
}

#[cfg(feature = "client")]
pub fn schemes_reload(c: &BlockingClient) -> anyhow::Result<MpnSchemes> {
    c.post_json(&(), EP_MPN_SCHEMES_RELOAD)
}

#[cfg(feature = "client")]
pub fn parse(c: &BlockingClient, q: &MpnParseQuery) -> anyhow::Result<Vec<MpnParse>> {
    c.post_json(q, EP_MPN_PARSE)
}

#[cfg(feature = "client")]
pub fn decode(c: &BlockingClient, q: &MpnDecodeQuery) -> anyhow::Result<MpnDecodeReport> {
    c.post_json(q, EP_MPN_DECODE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(keys: &[&str]) -> CorpusPart {
        CorpusPart {
            maker: "M".into(),
            mpn: "X".into(),
            part_id: 1,
            params: keys.iter().map(|k| (k.to_string(), "1".into())).collect(),
            sources: BTreeMap::new(),
        }
    }

    #[test]
    fn class_follows_the_curated_keys() {
        assert_eq!(part(&["capacitance", "rated_voltage"]).class(), "capacitor");
        assert_eq!(part(&["resistance", "power"]).class(), "resistor");
        assert_eq!(part(&["inductance"]).class(), "inductor");
        assert_eq!(part(&["zener_voltage"]).class(), "other");
        assert_eq!(part(&[]).class(), "other");
    }
}
