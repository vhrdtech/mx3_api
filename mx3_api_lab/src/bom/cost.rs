//! BOM cost analysis.
//!
//! BOM cost analysis (BOM-6): the best price of every line at every price break found in the BOM's supply
//! data, totals per board and for the whole order, and the most expensive lines. The UI's cost tab and
//! `mx3 bom cost` both render this. Every supplier's options compete for each line, or one supplier's only (INT-12).

use std::collections::HashSet;

use serde::Serialize;

use crate::endpoints::bom::{Bom, BomGroupId, ParameterSource};
use crate::endpoints::part::SupplyFilter;
use mx3_api_core::money::Money;

#[derive(Clone, Debug, Serialize)]
pub struct BomCost {
    /// Order quantities (boards) at which some part gets cheaper, ascending
    pub price_breaks: Vec<u32>,
    pub lines: Vec<CostLine>,
    /// One per price break
    pub totals: Vec<CostTotal>,
    /// Lines with no price at some break (no supply, or the first break is above the quantity)
    pub empty_count: usize,
    /// Indices into `lines` of the most expensive lines at the first price break, dearest first (at most 10)
    pub top_lines: Vec<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CostLine {
    /// Index of the line in `Bom::items`
    pub item_idx: usize,
    pub designators: String,
    /// Parts per board
    pub count: u32,
    pub description: String,
    /// Best unit price at each price break (`count * break` parts), `None` when there is none
    pub unit_prices: Vec<Option<Money>>,
    /// The supplier (name) of each best price (INT-12), `None` where there is no price
    pub suppliers: Vec<Option<String>>,
    /// Whether warehouse, suppliers and manufacturer together hold enough parts at each break
    pub enough_supply: Vec<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CostTotal {
    pub price_break: u32,
    /// Cost of one board at this order quantity
    pub per_board: Money,
    /// `per_board * price_break`
    pub investment: Money,
    /// Some line lacks supply for this quantity
    pub not_enough_supply: bool,
}

impl CostLine {
    /// Price of the line on one board at break `idx`
    pub fn line_price(&self, idx: usize) -> Option<Money> {
        self.unit_prices
            .get(idx)
            .copied()
            .flatten()
            .map(|p| Money::from_raw(p.raw() * self.count as i64))
    }

    /// Price went down compared to the previous break that had one (highlighted in the UI)
    pub fn cheaper_than_previous(&self, idx: usize) -> bool {
        let Some(Some(price)) = self.unit_prices.get(idx) else {
            return false;
        };
        self.unit_prices[..idx]
            .iter()
            .rev()
            .find_map(|p| *p)
            .is_some_and(|prev| *price < prev)
    }
}

impl BomCost {
    /// Lines of `group` only (and the groups it adds) when given, else all lines
    pub fn calculate(bom: &Bom, group: Option<BomGroupId>) -> Self {
        Self::calculate_for(bom, group, SupplyFilter::default())
    }

    /// [`Self::calculate`] over the supply options `only` takes: one supplier's (INT-12), brokers too (INT-16)
    pub fn calculate_for(bom: &Bom, group: Option<BomGroupId>, only: SupplyFilter) -> Self {
        let mut price_breaks = HashSet::new();
        for part in bom.referenced_parts.values() {
            for supply in part.supply_of(only) {
                for b in &supply.price_breaks {
                    price_breaks.insert(b.break_quantity as u32);
                }
            }
        }
        let mut price_breaks: Vec<u32> = price_breaks.into_iter().collect();
        price_breaks.sort_unstable();

        let mut totals: Vec<CostTotal> = price_breaks
            .iter()
            .map(|b| CostTotal {
                price_break: *b,
                per_board: Money::from_raw(0),
                investment: Money::from_raw(0),
                not_enough_supply: false,
            })
            .collect();
        let mut lines = vec![];
        let mut empty_count = 0;
        for (item_idx, item) in bom.items.iter().enumerate() {
            if let Some(group) = group
                && !item.is_in_group(group, &bom.groups)
            {
                continue;
            }
            let count = item.designators.len() as u32;
            let parts = &bom.referenced_parts;
            let total_stock = item.warehouse_stock(parts)
                + item.supplier_stock_of(parts, only)
                + item.manufacturer_stock(parts);
            let mut unit_prices = Vec::with_capacity(price_breaks.len());
            let mut suppliers = Vec::with_capacity(price_breaks.len());
            let mut enough_supply = Vec::with_capacity(price_breaks.len());
            for (break_idx, b) in price_breaks.iter().enumerate() {
                let parts_needed = b * count;
                let offer = item.best_offer(parts_needed, parts, only);
                let price = offer.map(|o| o.unit_price);
                suppliers.push(offer.map(|o| bom.supplier_name(o.supply.supplier_id)));
                let enough = total_stock >= parts_needed as u64;
                if let Some(price) = price {
                    let total = &mut totals[break_idx];
                    total.per_board =
                        Money::from_raw(total.per_board.raw() + price.raw() * count as i64);
                    total.not_enough_supply |= !enough;
                }
                unit_prices.push(price);
                enough_supply.push(enough);
            }
            if unit_prices.iter().any(|p| p.is_none()) {
                empty_count += 1;
            }
            lines.push(CostLine {
                item_idx,
                designators: item.designators_range(3),
                count,
                description: item
                    .find_parameter(
                        "Description",
                        &bom.referenced_parts,
                        ParameterSource::PreferGlobal,
                    )
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                unit_prices,
                suppliers,
                enough_supply,
            });
        }
        for total in &mut totals {
            total.investment = Money::from_raw(total.per_board.raw() * total.price_break as i64);
        }

        let mut by_price: Vec<(usize, Money)> = lines
            .iter()
            .enumerate()
            .filter_map(|(idx, line)| line.line_price(0).map(|p| (idx, p)))
            .collect();
        by_price.sort_by_key(|a| std::cmp::Reverse(a.1));
        let top_lines = by_price.into_iter().take(10).map(|(idx, _)| idx).collect();

        Self {
            price_breaks,
            lines,
            totals,
            empty_count,
            top_lines,
        }
    }

    /// Index of the smallest price break at or above `boards`, else the largest one
    pub fn break_for(&self, boards: u32) -> Option<usize> {
        if self.price_breaks.is_empty() {
            return None;
        }
        Some(
            self.price_breaks
                .iter()
                .position(|b| *b >= boards)
                .unwrap_or(self.price_breaks.len() - 1),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoints::bom::{BomHeader, BomId, BomItem, BomItemId};
    use crate::endpoints::companies::CompanyId;
    use crate::endpoints::folder::FolderId;
    use crate::endpoints::part::{Part, PartId, PartSupply, PartSupplyId, PriceBreak};
    use crate::endpoints::user::UserId;
    use chrono::Utc;
    use std::collections::{BTreeMap, HashMap};

    const DK: CompanyId = CompanyId(2);
    const MOUSER: CompanyId = CompanyId(9);

    fn supply(supplier: CompanyId, spn: &str, stock: i64, breaks: &[(i32, f64)]) -> PartSupply {
        PartSupply {
            id: PartSupplyId(0),
            part_id: PartId(1),
            supplier_id: supplier,
            spn: spn.into(),
            stock,
            updated: Utc::now(),
            packaging: "Cut Tape".into(),
            price_breaks: breaks
                .iter()
                .map(|(q, p)| PriceBreak {
                    break_quantity: *q,
                    break_price: Money::from_f64(*p),
                })
                .collect(),
            url: None,
            source: None,
            authorized: true,
            expires: None,
            moq: None,
            order_multiple: None,
            lead_days: None,
        }
    }

    /// One line (C1, C2) with one part: Digi-Key is cheaper for a few, Mouser from 100 on.
    fn bom() -> Bom {
        let part = Part {
            id: PartId(1),
            mfg: CompanyId(1),
            part_number: "GRM155R71H104KE14J".into(),
            supply: vec![
                supply(DK, "490-1-ND", 5000, &[(1, 0.10), (100, 0.020)]),
                supply(MOUSER, "81-GRM", 9000, &[(1, 0.12), (100, 0.015)]),
            ],
            folder_id: FolderId::root(),
            created: Utc::now(),
            updated: Utc::now(),
            is_incomplete: false,
            parameters: vec![],
            warehouse_stock: vec![],
            is_watched: false,
            files: vec![],
            crawl: Default::default(),
        };
        Bom {
            id: BomId(1),
            header: BomHeader {
                id: BomId(1),
                pcb_id: None,
                description: "test".into(),
                created: Utc::now(),
                updated: Utc::now(),
                user_id: UserId::hack(1),
                company_id: CompanyId(1),
                frozen: false,
                chain_id: Default::default(),
                chain: "main".into(),
                rev: 1,
                release: None,
                freeze_reason: None,
                frozen_at: None,
                head: true,
                split_from: None,
            },
            groups: BTreeMap::new(),
            items: vec![BomItem {
                id: BomItemId(1),
                bom_id: BomId(1),
                designators: vec!["C1".into(), "C2".into()],
                group_id: BomGroupId(1),
                part_choices: vec![PartId(1)],
                candidates: vec![],
                parameters: vec![],
                match_group: None,
                part_numbers: None,
                hand_choices: HashSet::new(),
            }],
            parameters: vec![],
            referenced_parts: HashMap::from([(PartId(1), part)]),
            designators_kinds: vec![],
            suppliers: HashMap::from([(DK, "Digi-Key".into()), (MOUSER, "Mouser".into())]),
        }
    }

    #[test]
    fn every_line_takes_the_cheapest_supplier() {
        let cost = BomCost::calculate(&bom(), None);
        assert_eq!(cost.price_breaks, [1, 100]);
        let line = &cost.lines[0];
        assert_eq!(line.unit_prices[0], Some(Money::from_f64(0.10)));
        assert_eq!(line.suppliers[0].as_deref(), Some("Digi-Key"));
        // 100 boards = 200 parts: Mouser's break
        assert_eq!(line.unit_prices[1], Some(Money::from_f64(0.015)));
        assert_eq!(line.suppliers[1].as_deref(), Some("Mouser"));
    }

    #[test]
    fn one_supplier_only() {
        let b = bom();
        let only = b.supplier_by_name("digikey");
        assert_eq!(only, Some(DK));
        assert_eq!(b.supplier_by_name("MOUSER"), Some(MOUSER));
        assert_eq!(b.supplier_by_name("tme"), None);
        let cost = BomCost::calculate_for(&b, None, SupplyFilter::only(only));
        assert_eq!(cost.lines[0].unit_prices[1], Some(Money::from_f64(0.020)));
        assert_eq!(cost.lines[0].suppliers[1].as_deref(), Some("Digi-Key"));
    }

    #[test]
    fn the_purchase_list_names_the_supplier() {
        let b = bom();
        let rows = b.purchase_list(1, SupplyFilter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].supplier.as_str(), rows[0].spn.as_str()),
            ("Digi-Key", "490-1-ND")
        );
        assert_eq!(rows[0].quantity, 2);
        let rows = b.purchase_list(100, SupplyFilter::default());
        assert_eq!(
            (rows[0].supplier.as_str(), rows[0].spn.as_str()),
            ("Mouser", "81-GRM")
        );
        let rows = b.purchase_list(100, SupplyFilter::supplier(DK));
        assert_eq!(rows[0].supplier, "Digi-Key");
        // a supplier none of whose options price the line: the row stays, without a supplier
        let rows = b.purchase_list(1, SupplyFilter::supplier(CompanyId(77)));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].supplier, "");
    }

    /// A broker's offer (INT-16): listed with the part, but out of cost, purchase and stock unless asked for.
    #[test]
    fn brokers_count_only_when_asked() {
        const BROKER: CompanyId = CompanyId(40);
        let mut b = bom();
        let mut offer = supply(BROKER, "WW-GRM", 100_000, &[(1, 0.001)]);
        offer.authorized = false;
        offer.source = Some("octopart".into());
        b.referenced_parts
            .get_mut(&PartId(1))
            .unwrap()
            .supply
            .push(offer);
        b.suppliers.insert(BROKER, "Worldway".into());
        let item = &b.items[0];
        let parts = &b.referenced_parts;

        let cost = BomCost::calculate(&b, None);
        assert_eq!(cost.lines[0].suppliers[0].as_deref(), Some("Digi-Key"));
        assert_eq!(item.supplier_stock(parts), 9000);
        assert_eq!(
            b.purchase_list(1, SupplyFilter::default())[0].supplier,
            "Digi-Key"
        );

        let brokers = SupplyFilter::default().with_brokers(true);
        let cost = BomCost::calculate_for(&b, None, brokers);
        assert_eq!(cost.lines[0].suppliers[0].as_deref(), Some("Worldway"));
        assert_eq!(item.supplier_stock_of(parts, brokers), 100_000);
        assert_eq!(b.purchase_list(1, brokers)[0].supplier, "Worldway");
        // one broker named: its rows count
        let only = SupplyFilter::supplier(BROKER);
        assert_eq!(item.supplier_stock_of(parts, only), 0);
        assert_eq!(
            item.supplier_stock_of(parts, only.with_brokers(true)),
            100_000
        );
    }
}
