//! Market checks (INT-16): one look-up per part at Octopart (Nexar) returns every distributor's offers (Arrow,
//! Farnell, TME, LCSC, Digi-Key, Mouser, ...). Nexar bills by matched part from a small budget, so a request names
//! the parts, can ask what it would spend first (`dry_run`), and reuses a look-up younger than a day. Nexar's terms
//! allow keeping the answers 24 hours at most: the backend deletes them after that, and only while they are kept
//! are their offers the part's supply (`PartSupply` rows of source `octopart`, INT-17).

use crate::endpoints::part::{PartId, PriceBreak};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Look parts up at the market (lab:write: spends the Octopart budget)
pub const EP_MARKET: &str = "/market";

/// The source of every market answer so far.
pub const SOURCE_OCTOPART: &str = "octopart";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketRequest {
    /// The parts to look up, by their MPN (and maker, when known)
    pub parts: Vec<PartId>,
    /// Only say what the look-up would spend and answer with what is kept; no call
    #[serde(default)]
    pub dry_run: bool,
    /// Look up again even parts whose answer is younger than a day
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketAnswer {
    pub budget: MarketBudget,
    /// Parts that needed a look-up (no kept answer, or `refresh`): the most this request spends
    pub to_look_up: u32,
    /// Parts answered from a kept look-up, at no cost
    pub kept: u32,
    /// Parts matched, what this request spent of the budget (0 on a dry run)
    pub spent: u32,
    /// Every part's last look-up, the ones made now and the kept ones; a dry run answers with the kept ones only
    pub parts: Vec<MarketPart>,
    /// Parts not looked up and why (no MPN, the budget, an earlier failure in this request)
    pub skipped: Vec<MarketSkip>,
    /// Why the look-up stopped early, when it did (part limit, credentials, Nexar unreachable)
    #[serde(default)]
    pub error: Option<String>,
    /// Nexar's notes about single fields (a field the plan leaves out)
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// The Octopart budget: matched parts per period (the `octopart` quota, `mx3 crawler quota`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketBudget {
    pub used: u32,
    pub limit: u32,
    /// The period's first day; 1 Jan 1970 for a budget that never resets (the Evaluation plan's lifetime parts)
    pub period_start: NaiveDate,
    /// When the next period starts, `None` when the budget never resets
    pub resets: Option<NaiveDate>,
    /// Credentials are stored (`mx3_backend octopart-key set`)
    pub key_set: bool,
}

impl MarketBudget {
    pub fn left(&self) -> u32 {
        self.limit.saturating_sub(self.used)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSkip {
    pub part_id: PartId,
    pub reason: String,
}

/// A part's last look-up at a market source, with provenance: the source, when, what was asked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketPart {
    pub part_id: PartId,
    pub source: String,
    pub fetched: DateTime<Utc>,
    /// What was asked
    pub mpn: String,
    pub maker: Option<String>,
    /// What the source matched; `None` when it matched nothing
    pub matched_mpn: Option<String>,
    pub matched_maker: Option<String>,
    /// `Production`, `NRND`, `EOL`, `Obsolete`, `New` (not on Nexar's Standard plan)
    pub lifecycle: Option<String>,
    /// The part's page at the source (Octopart)
    pub page_url: Option<String>,
    /// Stock over every distributor
    pub total_stock: Option<i64>,
    /// Estimated factory lead time (not on Nexar's Standard plan)
    pub lead_days: Option<i32>,
    pub hits: Option<i32>,
    pub error: Option<String>,
    pub offers: Vec<MarketOffer>,
}

/// One distributor's offer: one SKU with its stock, order rules, lead time and price breaks in USD.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketOffer {
    pub distributor: String,
    /// An authorized distributor of the maker (only these count in `bom cost` and `bom show`)
    pub authorized: bool,
    pub sku: Option<String>,
    /// Pieces in stock; `None` when the distributor gives no count
    pub stock: Option<i64>,
    pub moq: Option<i32>,
    pub order_multiple: Option<i32>,
    pub packaging: Option<String>,
    pub lead_days: Option<i32>,
    /// When the distributor last updated the offer
    pub seller_updated: Option<DateTime<Utc>>,
    /// The offer at the distributor (Octopart's tracked link)
    pub url: Option<String>,
    pub price_breaks: Vec<PriceBreak>,
}

impl MarketOffer {
    /// Pieces bought and the unit price for needing `qty`: `qty` rounded up to the MOQ, the first break and the
    /// order multiple, at the highest break that reaches. `None` without prices.
    pub fn price_at(&self, qty: u32) -> Option<(u32, mx3_api_core::money::Money)> {
        let first = self.price_breaks.first()?;
        let moq = self.moq.and_then(|m| u32::try_from(m).ok()).unwrap_or(0);
        let mut buy = qty
            .max(moq)
            .max(u32::try_from(first.break_quantity).unwrap_or(0))
            .max(1);
        if let Some(step) = self
            .order_multiple
            .and_then(|m| u32::try_from(m).ok())
            .filter(|m| *m > 1)
        {
            buy = buy.div_ceil(step).saturating_mul(step);
        }
        let b = self
            .price_breaks
            .iter()
            .rev()
            .find(|b| u32::try_from(b.break_quantity).is_ok_and(|q| q <= buy))?;
        Some((buy, b.break_price))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mx3_api_core::money::Money;

    #[test]
    fn an_offer_prices_what_must_be_bought() {
        let o = MarketOffer {
            distributor: "Arrow Electronics".into(),
            authorized: true,
            sku: None,
            stock: Some(39000),
            moq: Some(3000),
            order_multiple: Some(3000),
            packaging: None,
            lead_days: None,
            seller_updated: None,
            url: None,
            price_breaks: vec![
                PriceBreak {
                    break_quantity: 3000,
                    break_price: Money::from_f64(0.551),
                },
                PriceBreak {
                    break_quantity: 6000,
                    break_price: Money::from_f64(0.538),
                },
            ],
        };
        assert_eq!(o.price_at(10), Some((3000, Money::from_f64(0.551))));
        assert_eq!(o.price_at(3001), Some((6000, Money::from_f64(0.538))));
        let none = MarketOffer {
            price_breaks: vec![],
            ..o
        };
        assert_eq!(none.price_at(10), None);
    }
}
