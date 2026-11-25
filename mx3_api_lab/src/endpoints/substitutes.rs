//! Digi-Key's suggestions for a part (INT-18): its substitutes and recommended products, and the near matches of a
//! keyword search ("did you mean", cross-references). Asked for one MPN by `mx3 supplier substitutes`, and kept on
//! the parts the crawler could not find at Digi-Key, for `crawler failed` and the BOM's lines.

use crate::endpoints::bom::BomItemId;
use crate::endpoints::part::PartId;
use crate::endpoints::supplier_search::AddedPart;
use crate::supplier_search::price::QtyPrice;
use chrono::{DateTime, Utc};
use mx3_api_core::money::Money;
use serde::{Deserialize, Serialize};

/// Ask Digi-Key for the suggestions for one MPN (lab:write: it spends Digi-Key calls)
pub const EP_SUPPLIER_SUBSTITUTES: &str = "/supplier_substitutes";
/// The kept suggestions of parts
pub const EP_DK_SUGGESTIONS: &str = "/dk_suggestions";

/// Where a suggestion comes from
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKind {
    /// Digi-Key's substitutes of the product (`dk_kind` says which kind: Direct, Similar, ...)
    Substitute,
    /// A keyword search's product that names the MPN among its other names (a cross-reference)
    CrossReference,
    /// A keyword search's product whose MPN differs from the asked one in a character or two ("did you mean")
    Similar,
    /// Digi-Key's recommended products for the product
    Recommended,
}

impl SuggestionKind {
    /// The word in the database
    pub fn as_str(self) -> &'static str {
        match self {
            SuggestionKind::Substitute => "substitute",
            SuggestionKind::CrossReference => "cross_reference",
            SuggestionKind::Similar => "similar",
            SuggestionKind::Recommended => "recommended",
        }
    }

    pub fn parse(s: &str) -> Option<SuggestionKind> {
        [
            SuggestionKind::Substitute,
            SuggestionKind::CrossReference,
            SuggestionKind::Similar,
            SuggestionKind::Recommended,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }

    /// How a person reads it
    pub fn label(self) -> &'static str {
        match self {
            SuggestionKind::Substitute => "substitute",
            SuggestionKind::CrossReference => "cross-reference",
            SuggestionKind::Similar => "did you mean",
            SuggestionKind::Recommended => "recommended",
        }
    }
}

/// One suggested product
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Suggestion {
    /// 1-based, the best first: substitutes, cross-references, near MPNs, recommended
    pub rank: u32,
    pub kind: SuggestionKind,
    /// Digi-Key's own word for a substitute (`Direct`, `Similar`, ...), empty for the other kinds
    #[serde(default)]
    pub dk_kind: String,
    pub manufacturer: String,
    pub mpn: String,
    #[serde(default)]
    pub dk_pn: Option<String>,
    #[serde(default)]
    pub description: String,
    /// Digi-Key's stock, when it said
    #[serde(default)]
    pub stock: Option<i64>,
    /// Digi-Key's unit price of one piece, when it said
    #[serde(default)]
    pub unit_price: Option<Money>,
    /// The cheapest way to buy the asked quantity (`supplier substitutes --qty`, from the product's details)
    #[serde(default)]
    pub price: Option<QtyPrice>,
    #[serde(default)]
    pub url: String,
    /// The mx3 part with this maker and MPN, when there is one
    #[serde(default)]
    pub part: Option<PartId>,
}

impl Suggestion {
    /// One line for a terminal: `did you mean BAT750-7-F (Diodes Incorporated) · stock 54k · 0.42`
    pub fn short(&self) -> String {
        let kind = if self.dk_kind.is_empty() {
            self.kind.label().to_string()
        } else {
            format!("{} ({})", self.kind.label(), self.dk_kind.to_lowercase())
        };
        let mut s = format!("{kind} {}", self.mpn);
        if !self.manufacturer.is_empty() {
            s.push_str(&format!(" ({})", self.manufacturer));
        }
        if let Some(n) = self.stock {
            s.push_str(&format!(" · stock {n}"));
        }
        if let Some(p) = self.unit_price {
            s.push_str(&format!(" · {:.4}", p.to_f64()));
        }
        if let Some(p) = self.part {
            s.push_str(&format!(" · #{}", p.0));
        }
        s
    }
}

/// Ask Digi-Key for the suggestions for one MPN
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SubstitutesQuery {
    pub mpn: String,
    /// The maker, when known: picks the mx3 part the suggestions are kept on, and how the MPN is spelled
    #[serde(default)]
    pub maker: Option<String>,
    /// Price this many pieces from each shown product's details (one more Digi-Key call each)
    #[serde(default)]
    pub qty: Option<u32>,
    /// Suggestions to answer with; 0: 10
    #[serde(default)]
    pub top: u32,
    /// Also Digi-Key's recommended products (one more call)
    #[serde(default)]
    pub recommended: bool,
    /// Also near MPNs and cross-references from a keyword search (one more call)
    #[serde(default)]
    pub similar: bool,
    /// Add the picks as candidates to this BOM line
    #[serde(default)]
    pub add_to: Option<BomItemId>,
    /// Which suggestions (1-based rank) `add_to` takes; empty: all shown
    #[serde(default)]
    pub pick: Vec<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SubstitutesResult {
    pub mpn: String,
    /// The mx3 part with this MPN; the answer is kept on it
    pub part: Option<PartId>,
    pub suggestions: Vec<Suggestion>,
    /// Parts made or found for the picks, and whether they were added to the line
    pub added: Vec<AddedPart>,
    /// What went wrong without failing the whole answer (a call refused, an unread price)
    pub warnings: Vec<String>,
    /// Digi-Key calls this made, made today by searches, and the daily limit for searches
    pub calls: u32,
    pub calls_today: u32,
    pub calls_per_day: u32,
    /// What Digi-Key says is left of the account's daily calls
    pub dk_remaining: Option<u64>,
}

/// The kept suggestions of these parts
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SuggestionsQuery {
    pub parts: Vec<PartId>,
}

/// The kept suggestions of one part
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PartSuggestions {
    pub part: PartId,
    /// When Digi-Key was asked
    pub at: Option<DateTime<Utc>>,
    pub suggestions: Vec<Suggestion>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip() {
        for k in [
            SuggestionKind::Substitute,
            SuggestionKind::CrossReference,
            SuggestionKind::Similar,
            SuggestionKind::Recommended,
        ] {
            assert_eq!(SuggestionKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(SuggestionKind::parse("other"), None);
    }

    #[test]
    fn short_line() {
        let s = Suggestion {
            rank: 1,
            kind: SuggestionKind::Substitute,
            dk_kind: "Similar".into(),
            manufacturer: "Panjit International Inc.".into(),
            mpn: "SBA130CS_R1_00001".into(),
            dk_pn: None,
            description: String::new(),
            stock: Some(12000),
            unit_price: Some(Money::from_f64(0.31)),
            price: None,
            url: String::new(),
            part: None,
        };
        assert_eq!(
            s.short(),
            "substitute (similar) SBA130CS_R1_00001 (Panjit International Inc.) · stock 12000 · 0.3100"
        );
    }
}
