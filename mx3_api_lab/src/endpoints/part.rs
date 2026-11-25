use crate::endpoints::companies::CompanyId;
use crate::endpoints::file::FileHeader;
use crate::endpoints::folder::FolderId;
use crate::endpoints::params::{self, Resolved};
#[cfg(feature = "client")]
use crate::promise::Error;
#[cfg(feature = "client")]
use crate::request::RequestInitiator;
use crate::warehouse::models::{BoxItemKind, WarehouseBox};
#[cfg(feature = "client")]
use crate::{MxClient, Promise};
use chrono::{DateTime, Utc};
use human_repr::HumanCount;
use mx3_api_core::money::Money;
use rvariant::Variant;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct PartId(pub i32);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct PartSupplyId(pub i32);

impl PartId {
    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn new(id: i32) -> Self {
        Self(id)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub struct CreatePart {
    pub mfg_id: CompanyId,
    pub part_number: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MpnString {
    pub manufacturer_name: String,
    pub part_number: String,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct LookupParts {
    pub mpns: Vec<MpnString>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LookupPartsResult {
    pub parts: Vec<LookupPartResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LookupPartResult {
    Found { id: PartId, pn: String },
    NotFound,
    NotUnique(Vec<PartId>),
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn lookup_parts(
        &self,
        lookup_parts: &LookupParts,
        marker: &'static str,
    ) -> Promise<LookupPartsResult> {
        let (req, rx) = RequestInitiator::post_json_arg("lookup_parts".into(), lookup_parts);
        match self.request_tx.send(req) {
            Ok(()) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }

    pub fn get_part(&self, id: PartId, marker: &'static str) -> Promise<Part> {
        let (req, rx) = RequestInitiator::post_json_arg("get_part".into(), &id);
        match self.request_tx.send(req) {
            Ok(()) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }
}

impl LookupParts {
    pub fn find_idx(&self, part_number: &str) -> Option<usize> {
        self.mpns
            .iter()
            .enumerate()
            .find(|(_, mfg_pn_str)| {
                mfg_pn_str
                    .part_number
                    .eq_ignore_ascii_case(part_number.trim())
            })
            .map(|(idx, _)| idx)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Part {
    pub id: PartId,
    pub mfg: CompanyId,
    pub part_number: String,
    pub supply: Vec<PartSupply>,
    pub folder_id: FolderId,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub is_incomplete: bool,
    pub parameters: Vec<Resolved>,
    pub warehouse_stock: Vec<WarehouseBox>,
    pub is_watched: bool,
    pub files: Vec<FileHeader>,
    /// The crawl of this part per supplier (INT-12): when and whether it failed
    #[serde(default)]
    pub crawl: PartCrawl,
}

/// When the crawler last looked a part up at each supplier, and what went wrong (INT-12).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PartCrawl {
    pub digikey_crawled: Option<DateTime<Utc>>,
    /// The last Digi-Key crawl failed; the part waits until it is queued again
    pub digikey_error: bool,
    pub mouser_crawled: Option<DateTime<Utc>>,
    /// Why the last Mouser crawl failed; the part waits until it is queued again
    pub mouser_error: Option<String>,
    /// The last LCSC look-up (INT-15)
    #[serde(default)]
    pub lcsc_crawled: Option<DateTime<Utc>>,
    /// Why the last LCSC look-up failed; retried by itself some days later
    #[serde(default)]
    pub lcsc_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartSupply {
    pub id: PartSupplyId,
    pub part_id: PartId,
    pub supplier_id: CompanyId,
    pub spn: String,
    pub stock: i64,
    pub updated: DateTime<Utc>,
    pub packaging: String,
    pub price_breaks: Vec<PriceBreak>,
    /// The supplier's product page (INT-12: Mouser's; Digi-Key rows have none)
    #[serde(default)]
    pub url: Option<String>,
    /// Where the row comes from when not from mx3's own supply rows: `octopart` for a distributor's offer of a
    /// market look-up (INT-16), kept 24 hours at most and never stored as a supply row (`id` 0 then); `updated`
    /// is the look-up's time then
    #[serde(default)]
    pub source: Option<String>,
    /// The distributor is authorized by the maker; `false` for brokers and other unauthorized sellers, whose rows
    /// cost, purchase lists and stock checks leave out unless asked for ([`SupplyFilter::brokers`])
    #[serde(default = "authorized_default")]
    pub authorized: bool,
    /// When the row must be gone (`source` `octopart`: Nexar's 24 hours after the look-up); views drop it then
    #[serde(default)]
    pub expires: Option<DateTime<Utc>>,
    /// The least the seller sells
    #[serde(default)]
    pub moq: Option<i32>,
    /// The step above the MOQ
    #[serde(default)]
    pub order_multiple: Option<i32>,
    /// The seller's lead time when out of stock, in days
    #[serde(default)]
    pub lead_days: Option<i32>,
}

fn authorized_default() -> bool {
    true
}

/// Which supply rows count for cost, purchase lists and stock: one supplier's or all, brokers or not (INT-16).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SupplyFilter {
    /// Only this supplier's rows (`None`: every supplier)
    pub supplier: Option<CompanyId>,
    /// Count unauthorized sellers too (`--brokers`)
    pub brokers: bool,
}

impl SupplyFilter {
    /// One supplier's rows, brokers left out
    pub fn supplier(id: CompanyId) -> Self {
        Self {
            supplier: Some(id),
            brokers: false,
        }
    }

    /// Every supplier's rows, brokers left out, or one supplier's when `only` is given
    pub fn only(only: Option<CompanyId>) -> Self {
        Self {
            supplier: only,
            brokers: false,
        }
    }

    pub fn with_brokers(self, brokers: bool) -> Self {
        Self { brokers, ..self }
    }

    pub fn takes(&self, s: &PartSupply) -> bool {
        (self.brokers || s.authorized) && self.supplier.is_none_or(|id| s.supplier_id == id)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PartSupplyCreate {
    pub part_id: PartId,
    pub supplier_id: CompanyId,
    pub spn: String,
    pub stock: i64,
    pub packaging: String,
    pub price_breaks: Vec<PriceBreak>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PriceBreak {
    pub break_quantity: i32,
    pub break_price: Money,
}

impl Part {
    pub fn warehouse_stock(&self) -> u64 {
        let mut stock = 0;
        for b in &self.warehouse_stock {
            for item in &b.items.items {
                if let BoxItemKind::Part { part_id } = &item.kind
                    && part_id == &self.id
                {
                    stock += item.count as u64;
                }
            }
        }
        stock
    }

    /// The most any one supply option has in stock
    pub fn supplier_stock(&self) -> u64 {
        self.supplier_stock_of(SupplyFilter::default())
    }

    /// [`Self::supplier_stock`] of the options `only` takes
    pub fn supplier_stock_of(&self, only: SupplyFilter) -> u64 {
        let mut stock = 0;
        for supply in self.supply_of(only) {
            stock = stock.max(u64::try_from(supply.stock).unwrap_or(0));
        }
        stock
    }

    /// The supply options `only` takes
    pub fn supply_of(&self, only: SupplyFilter) -> impl Iterator<Item = &PartSupply> {
        self.supply.iter().filter(move |s| only.takes(s))
    }

    pub fn manufacturer_stock(&self) -> u64 {
        match params::find(&self.parameters, "manufacturer public quantity") {
            Some(p) => p.value.as_u64().unwrap_or(0),
            None => 0,
        }
    }

    pub fn total_stock(&self) -> u64 {
        self.warehouse_stock() + self.supplier_stock() + self.manufacturer_stock()
    }

    pub fn find_parameter(&self, name: &str) -> Option<Variant> {
        params::find(&self.parameters, name).map(|p| p.value.clone())
    }

    pub fn find_parameter_many(&self, names: &[&str]) -> Option<Variant> {
        for name in names {
            if let Some(value) = self.find_parameter(name) {
                return Some(value);
            }
        }
        None
    }

    pub fn supply_overview(&self) -> Vec<String> {
        self.supply
            .iter()
            .map(|s| {
                let breaks: String = s.price_breaks.iter().map(|b| b.to_string() + " ").collect();
                format!("{}: {}", s.packaging, breaks)
            })
            .collect()
    }

    pub fn best_price_for_break(&self, quantity: u32) -> Option<Money> {
        self.best_offer(quantity, SupplyFilter::default())
            .map(|(_, price)| price)
    }

    /// The cheapest supply option for `quantity` parts and its unit price, among the options `only` takes (one
    /// supplier's: INT-12, brokers or not: INT-16); on a tie, the option with more stock.
    pub fn best_offer(&self, quantity: u32, only: SupplyFilter) -> Option<(&PartSupply, Money)> {
        self.supply_of(only)
            .filter_map(|s| s.find_price(quantity).map(|p| (s, p)))
            .min_by(|(a, pa), (b, pb)| pa.cmp(pb).then(b.stock.cmp(&a.stock)))
    }
}

impl PartSupply {
    pub fn find_price(&self, quantity: u32) -> Option<Money> {
        Self::find_price_inner(quantity, &self.price_breaks)
    }

    fn find_price_inner(quantity: u32, price_breaks: &[PriceBreak]) -> Option<Money> {
        if quantity == 0 {
            return None;
        }
        if price_breaks.is_empty() {
            return None;
        }
        match price_breaks.binary_search_by(|b| (b.break_quantity as u32).cmp(&quantity)) {
            Ok(landed_on_price_break) => Some(price_breaks[landed_on_price_break].break_price),
            Err(landed_between_price_breaks) => {
                if landed_between_price_breaks == 0 {
                    if price_breaks[0].break_quantity == 1 {
                        Some(price_breaks[0].break_price)
                    } else {
                        // if the first price break is e.g., 1k and requesting 10 pcs
                        None
                    }
                } else {
                    if landed_between_price_breaks >= price_breaks.len() {
                        Some(price_breaks[price_breaks.len() - 1].break_price)
                    } else {
                        Some(price_breaks[landed_between_price_breaks - 1].break_price)
                    }
                }
            }
        }
    }
}

impl Display for PriceBreak {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}",
            self.break_quantity.human_count(""),
            self.break_price
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::endpoints::part::{PartSupply, PriceBreak};
    use mx3_api_core::money::Money;

    #[test]
    fn find_price_sanity() {
        let price_breaks = vec![
            PriceBreak {
                break_quantity: 1,
                break_price: Money::from_f32(0.43),
            },
            PriceBreak {
                break_quantity: 10,
                break_price: Money::from_f32(0.26),
            },
            PriceBreak {
                break_quantity: 100,
                break_price: Money::from_f32(0.16),
            },
        ];
        assert_eq!(PartSupply::find_price_inner(0, &price_breaks), None);
        assert_eq!(
            PartSupply::find_price_inner(1, &price_breaks),
            Some(Money::from_f32(0.43))
        );
        assert_eq!(
            PartSupply::find_price_inner(3, &price_breaks),
            Some(Money::from_f32(0.43))
        );
        assert_eq!(
            PartSupply::find_price_inner(10, &price_breaks),
            Some(Money::from_f32(0.26))
        );
        assert_eq!(
            PartSupply::find_price_inner(20, &price_breaks),
            Some(Money::from_f32(0.26))
        );
        assert_eq!(
            PartSupply::find_price_inner(100, &price_breaks),
            Some(Money::from_f32(0.16))
        );
        assert_eq!(
            PartSupply::find_price_inner(200, &price_breaks),
            Some(Money::from_f32(0.16))
        );
    }

    #[test]
    fn find_price_starts_at_first_break() {
        let price_breaks = vec![
            PriceBreak {
                break_quantity: 1000,
                break_price: Money::from_f32(0.21),
            },
            PriceBreak {
                break_quantity: 2000,
                break_price: Money::from_f32(0.13),
            },
            PriceBreak {
                break_quantity: 3000,
                break_price: Money::from_f32(0.08),
            },
        ];
        assert_eq!(PartSupply::find_price_inner(0, &price_breaks), None);
        assert_eq!(PartSupply::find_price_inner(1, &price_breaks), None);
        assert_eq!(PartSupply::find_price_inner(10, &price_breaks), None);
        assert_eq!(
            PartSupply::find_price_inner(1000, &price_breaks),
            Some(Money::from_f32(0.21))
        );
        assert_eq!(
            PartSupply::find_price_inner(1001, &price_breaks),
            Some(Money::from_f32(0.21))
        );
        assert_eq!(
            PartSupply::find_price_inner(2000, &price_breaks),
            Some(Money::from_f32(0.13))
        );
        assert_eq!(
            PartSupply::find_price_inner(2500, &price_breaks),
            Some(Money::from_f32(0.13))
        );
        assert_eq!(
            PartSupply::find_price_inner(3000, &price_breaks),
            Some(Money::from_f32(0.08))
        );
        assert_eq!(
            PartSupply::find_price_inner(3001, &price_breaks),
            Some(Money::from_f32(0.08))
        );
    }
}

/// Set the manufacturer of parts (INT-11): clears the crawler error so the crawler tries them again, now with the
/// manufacturer's Digi-Key ids.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SetPartManufacturer {
    pub part_ids: Vec<PartId>,
    pub company_id: CompanyId,
}
