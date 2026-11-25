//! Parametric search at Digi-Key (SRCH-4): narrow by category, package, stock, lifecycle and parameters the way
//! the website does, see what values remain for the next step, take the cheapest at a quantity, and add picks as
//! BOM line candidates (BOM-24). Filters by name and value text; the backend maps them to Digi-Key's ids.

use crate::endpoints::bom::BomItemId;
use crate::endpoints::part::PartId;
use crate::supplier_search::filter::ParamFilter;
use crate::supplier_search::price::QtyPrice;
use serde::{Deserialize, Serialize};

/// Search at Digi-Key (lab:write: every search spends Digi-Key calls)
pub const EP_SUPPLIER_SEARCH: &str = "/supplier_search";
/// Digi-Key's categories whose name contains a text, from the cached tree (refreshed when old)
pub const EP_SUPPLIER_CATEGORIES: &str = "/supplier_categories";

/// One search. Everything but `keywords` is optional; parameter filters need a category (Digi-Key's parameter ids
/// are per category).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartSearch {
    /// Words, a partial MPN or a Digi-Key part number; may be empty with a category
    #[serde(default)]
    pub keywords: String,
    /// Category name (a unique part of it, ignoring case) or Digi-Key's id
    #[serde(default)]
    pub category: Option<String>,
    /// Package / case values, any of them: `0402`, `SOT-23`; matched like `Package / Case=...`
    #[serde(default)]
    pub package: Vec<String>,
    /// Parameter filters, all of them
    #[serde(default)]
    pub params: Vec<ParamFilter>,
    /// Manufacturer names (unique parts of them), any of them
    #[serde(default)]
    pub manufacturers: Vec<String>,
    /// Only what Digi-Key has in stock
    #[serde(default)]
    pub in_stock: bool,
    /// Only what Digi-Key normally stocks
    #[serde(default)]
    pub normally_stocking: bool,
    /// Product status names (`Active`, `Not For New Designs`, ...); empty: any. The CLI sends `Active` unless asked
    #[serde(default)]
    pub status: Vec<String>,
    /// Also products of marketplace sellers
    #[serde(default)]
    pub marketplace: bool,
    /// Price this many pieces, each product's cheapest packaging for it (1 when not given)
    #[serde(default)]
    pub qty: Option<u32>,
    /// Packaging names (unique parts, ignoring case: `cut tape`, `reel`) a price may come from; empty: any
    #[serde(default)]
    pub packaging: Vec<String>,
    /// Products to answer with, cheapest at `qty` first
    pub top: u32,
    /// Products to look at (Digi-Key sorted by its own unit price) before sorting at `qty`; 0: the backend's default
    #[serde(default)]
    pub scan: u32,
    /// Also answer with what the next request can narrow by
    #[serde(default)]
    pub facets: bool,
    /// Create parts for the picks, filled from the search answer (no further Digi-Key calls)
    #[serde(default)]
    pub create: bool,
    /// Add the picks as candidates to this BOM line (implies `create`)
    #[serde(default)]
    pub add_to: Option<BomItemId>,
    /// Which hits (1-based rank) `create`/`add_to` take; empty: all of them
    #[serde(default)]
    pub pick: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartSearchResult {
    /// The search's row in the history (`part_search`), named by the parts it created
    pub search_id: i64,
    pub category: Option<CategoryRef>,
    /// Products matching all filters at Digi-Key
    pub products_count: u32,
    /// Products looked at for the price sort
    pub scanned: u32,
    /// Each filter with the values it became
    pub resolved: Vec<ResolvedFilter>,
    pub hits: Vec<SearchHit>,
    pub facets: Option<Facets>,
    /// Parts made or found for the picks, and the BOM line they were added to
    pub added: Vec<AddedPart>,
    /// What the search left out or couldn't read
    pub warnings: Vec<String>,
    /// Digi-Key calls this search made, made today by searches, and the daily limit for searches
    pub calls: u32,
    pub calls_today: u32,
    pub calls_per_day: u32,
    /// What Digi-Key says is left of the account's daily calls (`X-RateLimit-Remaining`)
    pub dk_remaining: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryRef {
    pub id: i32,
    pub name: String,
    /// Names from the root down, the category's own last
    pub path: Vec<String>,
}

/// One filter as Digi-Key got it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedFilter {
    /// The filter as given: `Voltage - Rated>=25V`, `status=Active`
    pub filter: String,
    /// Digi-Key's name of what it filters (parameter name, `Manufacturer`, `Status`)
    pub name: String,
    pub parameter_id: Option<i32>,
    /// The values it selects, by name
    pub values: Vec<String>,
    /// Values whose text couldn't be compared with the filter (no number in its unit)
    pub unreadable: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// 1-based, cheapest at the quantity first
    pub rank: u32,
    pub manufacturer: String,
    pub mpn: String,
    pub description: String,
    /// The cheapest way to buy the quantity; `None` when no packaging sells it (minimum order, no price)
    pub price: Option<QtyPrice>,
    /// Digi-Key's stock of the product, all packagings
    pub stock: u64,
    pub status: String,
    pub normally_stocking: bool,
    pub url: String,
    /// The product's parameters, (name, value) as Digi-Key writes them
    pub params: Vec<(String, String)>,
    /// The mx3 part with this maker and MPN, when there is one
    pub part: Option<PartId>,
}

/// What the next request can narrow by, each value with its product count.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Facets {
    pub categories: Vec<FacetValue>,
    pub manufacturers: Vec<FacetValue>,
    pub packaging: Vec<FacetValue>,
    pub status: Vec<FacetValue>,
    pub parameters: Vec<ParamFacet>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FacetValue {
    pub id: String,
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamFacet {
    pub parameter_id: i32,
    pub name: String,
    pub values: Vec<FacetValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddedPart {
    pub rank: u32,
    pub part: PartId,
    pub mpn: String,
    /// Made by this search (else it existed and was refreshed from the answer)
    pub created: bool,
    /// Added as a candidate to the line (false: it was a choice or candidate there already)
    pub added_to_line: bool,
}

/// Digi-Key's categories whose name contains `text` (all of them for an empty text).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryQuery {
    #[serde(default)]
    pub text: String,
    /// Fetch the tree again now, whatever its age (one Digi-Key call)
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryHit {
    pub category: CategoryRef,
    pub product_count: i64,
    /// A category without children: the only kind parameter filters work in
    pub leaf: bool,
}
