//! Which BOM lines a market check looks up (INT-16): the risky ones by default, named lines, or all, and the parts
//! their real choices name. A market look-up spends a small budget per part, so the default asks only where the
//! usual suppliers fall short.

use std::collections::HashSet;

use serde::Serialize;

use crate::endpoints::bom::{Bom, BomGroupId, BomItem};
use crate::endpoints::part::{PartId, SupplyFilter};

/// The lines to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// Lines Digi-Key and Mouser can't fill alone for the board count, and lines other than R, L and C whose
    /// choices all come from one maker
    Risky,
    /// These lines, by their number in `bom show`
    Lines(Vec<usize>),
    All,
}

/// A line to check, why, and the parts to look up for it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MarketLine {
    /// The line's number in `bom show`
    pub line: usize,
    /// Pieces needed for the most boards asked
    pub required: u64,
    /// Why a risky line is risky (empty for named lines and `--all`)
    pub reasons: Vec<String>,
    /// The line's real part choices
    pub parts: Vec<PartId>,
}

/// The lines `selection` names, in BOM order, for `boards` boards (the most asked); lines without a real part choice
/// are left out (nothing to look up). Named lines that don't exist are left out too.
pub fn market_lines(
    bom: &Bom,
    group: Option<BomGroupId>,
    selection: &Selection,
    boards: u32,
) -> Vec<MarketLine> {
    let mut out = vec![];
    for (idx, item) in bom.items.iter().enumerate() {
        if let Some(group) = group
            && !item.is_in_group(group, &bom.groups)
        {
            continue;
        }
        if item.part_choices.is_empty() {
            continue;
        }
        let required = item.designators.len() as u64 * boards as u64;
        let reasons = match selection {
            Selection::All => vec![],
            Selection::Lines(lines) if lines.contains(&idx) => vec![],
            Selection::Lines(_) => continue,
            Selection::Risky => {
                let r = risks(bom, item, required);
                if r.is_empty() {
                    continue;
                }
                r
            }
        };
        out.push(MarketLine {
            line: idx,
            required,
            reasons,
            parts: item.part_choices.clone(),
        });
    }
    out
}

/// Every part the lines name, once, in order.
pub fn parts_of(lines: &[MarketLine]) -> Vec<PartId> {
    let mut seen = HashSet::new();
    lines
        .iter()
        .flat_map(|l| l.parts.iter().copied())
        .filter(|p| seen.insert(*p))
        .collect()
}

fn risks(bom: &Bom, item: &BomItem, required: u64) -> Vec<String> {
    let parts = &bom.referenced_parts;
    let mut out = vec![];
    // what one of the two can deliver alone: the line is bought at one of them
    let direct = ["digikey", "mouser"]
        .iter()
        .filter_map(|n| bom.supplier_by_name(n))
        .map(|id| item.supplier_stock_of(parts, SupplyFilter::supplier(id)))
        .max()
        .unwrap_or(0);
    if direct < required {
        out.push(format!("Digi-Key/Mouser stock {direct} < {required}"));
    }
    let passive = item
        .designators
        .iter()
        .any(|d| d.starts_with(['R', 'L', 'C']));
    let makers: HashSet<_> = item
        .part_choices
        .iter()
        .filter_map(|id| parts.get(id))
        .map(|p| p.mfg)
        .collect();
    if !passive && makers.len() == 1 {
        out.push("single maker".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoints::bom::{BomHeader, BomId, BomItemId};
    use crate::endpoints::companies::CompanyId;
    use crate::endpoints::folder::FolderId;
    use crate::endpoints::part::{Part, PartSupply, PartSupplyId};
    use crate::endpoints::user::UserId;
    use chrono::Utc;
    use std::collections::{BTreeMap, HashMap};

    const DK: CompanyId = CompanyId(2);
    const MOUSER: CompanyId = CompanyId(9);

    fn part(id: i32, maker: i32, stock: &[(CompanyId, i64)]) -> Part {
        Part {
            id: PartId(id),
            mfg: CompanyId(maker),
            part_number: format!("P{id}"),
            supply: stock
                .iter()
                .map(|(s, n)| PartSupply {
                    id: PartSupplyId(0),
                    part_id: PartId(id),
                    supplier_id: *s,
                    spn: format!("S{id}"),
                    stock: *n,
                    updated: Utc::now(),
                    packaging: String::new(),
                    price_breaks: vec![],
                    url: None,
                    source: None,
                    authorized: true,
                    expires: None,
                    moq: None,
                    order_multiple: None,
                    lead_days: None,
                })
                .collect(),
            folder_id: FolderId::root(),
            created: Utc::now(),
            updated: Utc::now(),
            is_incomplete: false,
            parameters: vec![],
            warehouse_stock: vec![],
            is_watched: false,
            files: vec![],
            crawl: Default::default(),
        }
    }

    fn item(id: i32, designators: &[&str], choices: &[i32]) -> BomItem {
        BomItem {
            id: BomItemId(id),
            bom_id: BomId(1),
            designators: designators.iter().map(|d| d.to_string()).collect(),
            group_id: BomGroupId(1),
            part_choices: choices.iter().map(|c| PartId(*c)).collect(),
            candidates: vec![],
            parameters: vec![],
            match_group: None,
            part_numbers: None,
            hand_choices: HashSet::new(),
        }
    }

    /// C1-C2: plenty at Digi-Key; U1: one maker, plenty; U2: two makers, 30 at Mouser only; J1: no choice.
    fn bom() -> Bom {
        let parts = [
            part(1, 10, &[(DK, 5000)]),
            part(2, 11, &[(DK, 900)]),
            part(3, 12, &[(MOUSER, 30)]),
            part(4, 13, &[]),
        ];
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
            items: vec![
                item(1, &["C1", "C2"], &[1]),
                item(2, &["U1"], &[2]),
                item(3, &["U2"], &[3, 4]),
                item(4, &["J1"], &[]),
            ],
            parameters: vec![],
            referenced_parts: parts.into_iter().map(|p| (p.id, p)).collect(),
            designators_kinds: vec![],
            suppliers: HashMap::from([(DK, "Digi-Key".into()), (MOUSER, "Mouser".into())]),
        }
    }

    #[test]
    fn risky_lines_are_short_or_single_sourced() {
        let b = bom();
        let lines = market_lines(&b, None, &Selection::Risky, 10);
        let got: Vec<(usize, Vec<String>)> =
            lines.iter().map(|l| (l.line, l.reasons.clone())).collect();
        // U2 has two makers and Mouser's 30 cover 10 boards
        assert_eq!(got, [(1, vec!["single maker".to_string()])]);
        // at 100 boards U2 needs 100 and Mouser has 30
        let lines = market_lines(&b, None, &Selection::Risky, 100);
        let got: Vec<(usize, Vec<String>)> =
            lines.iter().map(|l| (l.line, l.reasons.clone())).collect();
        assert_eq!(
            got,
            [
                (1, vec!["single maker".to_string()]),
                (2, vec!["Digi-Key/Mouser stock 30 < 100".to_string()]),
            ]
        );
        assert_eq!(parts_of(&lines), [PartId(2), PartId(3), PartId(4)]);
    }

    #[test]
    fn named_lines_and_all() {
        let b = bom();
        let lines = market_lines(&b, None, &Selection::Lines(vec![0, 3, 9]), 1);
        // J1 has no choice, line 9 doesn't exist
        assert_eq!(lines.iter().map(|l| l.line).collect::<Vec<_>>(), [0]);
        assert_eq!(lines[0].required, 2);
        let all = market_lines(&b, None, &Selection::All, 1);
        assert_eq!(all.iter().map(|l| l.line).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(parts_of(&all).len(), 4);
    }
}
