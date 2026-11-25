//! The BOM as the VHRD spreadsheet.
//!
//! The BOM as the VHRD spreadsheet (BOM-27): what goes in it, worked out from a [`Bom`] alone. The backend turns
//! a [`BomExport`] into the workbook (`mx3_backend/src/bom_xlsx.rs`); keeping the choices here (the preferred part,
//! the risks, stock and price per supplier at the order quantity) lets the CLI and UI show the same numbers.
//!
//! - The preferred part of a line: among its real choices the cheapest at the order quantity that one authorized
//!   seller has enough of; else such a candidate; else the choice or candidate with the most authorized stock.
//! - Supplier columns: the suppliers mx3 crawls itself (rows of its own, not market offers), Digi-Key, Mouser and
//!   LCSC first. Every other seller (Octopart's kept offers, INT-16) is listed under the MPN, brokers marked.
//! - A line is risky when no authorized seller has enough of any of its parts, when every real choice comes from
//!   one maker (chip R and C excepted), and when it is a match group (one MPN per board, BOM-26).

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;

use crate::endpoints::bom::{Bom, BomItem};
use crate::endpoints::companies::CompanyId;
use crate::endpoints::part::{Part, PartId, PartSupply, SupplyFilter};
use crate::endpoints::pcb::Pcb;
use mx3_api_core::money::Money;

/// What the sheets need besides the BOM.
#[derive(Clone, Debug)]
pub struct ExportContext {
    /// The product the board belongs to ("Portwright"); empty when unknown
    pub product: String,
    /// The board ("B153 rev B")
    pub board: String,
    /// The PCB's description, shown when it says more than the product's name
    pub title: String,
    /// Maker names by company id
    pub makers: HashMap<CompanyId, String>,
    /// Boards to order for: the quantities stock and prices are judged at
    pub boards: u32,
    pub date: NaiveDate,
}

impl ExportContext {
    /// Product, board and title from a PCB and its parents (the product of the nearest PCB that names one).
    pub fn from_pcbs(pcb: Option<&Pcb>, all: &[Pcb]) -> (String, String, String) {
        let Some(pcb) = pcb else {
            return Default::default();
        };
        let mut product = pcb.product.clone();
        let mut at = pcb.parent_id;
        let mut hops = 0;
        while product.is_empty() && hops < 16 {
            let Some(parent) = at.and_then(|id| all.iter().find(|p| p.id == id)) else {
                break;
            };
            product = parent.product.clone();
            at = parent.parent_id;
            hops += 1;
        }
        let rev = pcb.revision.trim();
        let board = if rev.is_empty() || pcb.name.to_lowercase().contains("rev") {
            pcb.name.clone()
        } else {
            format!("{} rev {rev}", pcb.name)
        };
        (product, board, pcb.description.clone())
    }
}

#[derive(Clone, Debug)]
pub struct BomExport {
    pub product: String,
    pub board: String,
    pub title: String,
    pub date: NaiveDate,
    /// BOM number, its description and, when known, the file and commit it was imported from
    pub source: String,
    pub boards: u32,
    /// The supplier columns of the Supply sheet, in order
    pub suppliers: Vec<String>,
    pub lines: Vec<ExportLine>,
}

#[derive(Clone, Debug)]
pub struct ExportLine {
    /// 1-based, the line's number in `mx3 bom show`
    pub number: usize,
    pub designators: Vec<String>,
    /// Parts per board
    pub qty: u32,
    /// Parts for the order: `qty` × boards
    pub need: u32,
    pub value: String,
    pub footprint: String,
    pub description: String,
    /// The line's match group: every part of the line on one board must be the same MPN (BOM-26)
    pub match_group: Option<String>,
    /// Real choices first, then candidates
    pub choices: Vec<ExportChoice>,
    /// Index into `choices`
    pub preferred: Option<usize>,
    /// An authorized seller has enough of the preferred part
    pub enough: bool,
    /// Why the line is risky, empty when it is not
    pub risks: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChoiceRole {
    /// The line's first real choice
    Mpn1,
    /// Another real choice
    Alternative,
    /// Suggested, not confirmed (BOM-24)
    Candidate,
}

impl ChoiceRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mpn1 => "Mpn1",
            Self::Alternative => "alternative",
            Self::Candidate => "candidate",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportChoice {
    pub part_id: PartId,
    pub mpn: String,
    /// Empty when unknown
    pub maker: String,
    pub role: ChoiceRole,
    /// No authorized seller has the order quantity
    pub short: bool,
    /// Per supplier column: stock and unit price at the order quantity, `None` when it doesn't offer the part
    pub supply: Vec<Option<SupplyAt>>,
    /// Sellers other than the supplier columns (market offers), authorized ones first, then by enough stock and
    /// price
    pub others: Vec<OtherOffer>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupplyAt {
    pub stock: i64,
    pub unit_price: Option<Money>,
}

#[derive(Clone, Debug)]
pub struct OtherOffer {
    pub seller: String,
    pub authorized: bool,
    pub stock: i64,
    pub unit_price: Option<Money>,
    pub lead_days: Option<i32>,
}

/// Other sellers listed per MPN on the Supply sheet
pub const OTHERS_PER_MPN: usize = 6;

/// Lines and parts for the header
impl BomExport {
    pub fn part_count(&self) -> u32 {
        self.lines.iter().map(|l| l.qty).sum()
    }

    /// The most choices any line has: the number of Mpn columns
    pub fn width(&self) -> usize {
        self.lines
            .iter()
            .map(|l| l.choices.len())
            .max()
            .unwrap_or(0)
    }

    /// `<product>_<board>_bom_<date>.xlsx`, lower case, anything but letters and digits as `_`
    pub fn file_name(&self) -> String {
        let stem = format!(
            "{} {} bom {}",
            self.product,
            self.board,
            self.date.format("%-d%b%Y")
        );
        let mut out = String::new();
        for c in stem.to_lowercase().chars() {
            if c.is_alphanumeric() {
                out.push(c);
            } else if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
        }
        format!("{}.xlsx", out.trim_end_matches('_'))
    }
}

/// The suppliers mx3 crawls itself, in this order, then any other with rows of its own
const SUPPLIER_ORDER: [&str; 3] = ["digikey", "mouser", "lcsc"];

fn key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// A part's description from its supplier, Digi-Key's when it has one
pub fn supplier_description(part: &Part) -> Option<String> {
    let descs: Vec<_> = part
        .parameters
        .iter()
        .filter(|p| !p.retracted && p.name.eq_ignore_ascii_case("description"))
        .collect();
    descs
        .iter()
        .find(|p| p.source == "digikey")
        .or(descs.first())
        .map(|p| p.raw.clone().unwrap_or_else(|| p.value.to_string()))
        .filter(|s| !s.trim().is_empty())
}

fn local(item: &BomItem, name: &str) -> String {
    item.find_parameter_only_local(name)
        .map(|v| v.to_string())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// The line's match group (BOM-26, `bom_item.match_group`), when it has one
pub fn match_group(item: &BomItem) -> Option<String> {
    item.match_group
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// The cheapest unit price for `need` over `rows`
fn price_at<'a>(rows: impl Iterator<Item = &'a PartSupply>, need: u32) -> Option<Money> {
    rows.filter_map(|s| s.find_price(need)).min()
}

/// The cheapest unit price for `need` among the authorized sellers that have that many; `None` when none has.
/// A seller with enough but no price for that quantity counts, priced last.
fn fits(part: &Part, need: u32) -> Option<Money> {
    part.supply_of(SupplyFilter::default())
        .filter(|s| s.stock >= i64::from(need))
        .map(|s| s.find_price(need).unwrap_or(Money::from_raw(i64::MAX)))
        .min()
}

impl BomExport {
    pub fn new(bom: &Bom, cx: &ExportContext) -> Self {
        let parts = &bom.referenced_parts;
        // supplier columns: suppliers with rows of their own (not market offers)
        let mut own: Vec<CompanyId> = parts
            .values()
            .flat_map(|p| p.supply.iter())
            .filter(|s| s.source.is_none())
            .map(|s| s.supplier_id)
            .collect();
        own.sort_unstable_by_key(|id| id.0);
        own.dedup();
        own.sort_by_key(|id| {
            let name = key(&bom.supplier_name(*id));
            (
                SUPPLIER_ORDER
                    .iter()
                    .position(|k| *k == name)
                    .unwrap_or(SUPPLIER_ORDER.len()),
                name,
            )
        });
        let suppliers: Vec<String> = own.iter().map(|id| bom.supplier_name(*id)).collect();
        let maker = |p: &Part| {
            cx.makers
                .get(&p.mfg)
                .filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case("unknown"))
                .cloned()
                .unwrap_or_default()
        };

        let mut lines = vec![];
        for (idx, item) in bom.items.iter().enumerate() {
            let qty = item.designators.len() as u32;
            let need = qty * cx.boards.max(1);
            let real = item.part_choices.iter().map(|id| (*id, false));
            let cands = item
                .candidates
                .iter()
                .filter(|id| !item.part_choices.contains(id))
                .map(|id| (*id, true));
            let mut choices = vec![];
            for (n, (id, candidate)) in real.chain(cands).enumerate() {
                let Some(p) = parts.get(&id) else { continue };
                let role = if candidate {
                    ChoiceRole::Candidate
                } else if n == 0 {
                    ChoiceRole::Mpn1
                } else {
                    ChoiceRole::Alternative
                };
                let supply = own
                    .iter()
                    .map(|sup| {
                        let rows: Vec<_> = p
                            .supply_of(SupplyFilter::supplier(*sup))
                            .collect::<Vec<_>>();
                        (!rows.is_empty()).then(|| SupplyAt {
                            stock: rows.iter().map(|s| s.stock).max().unwrap_or(0),
                            unit_price: price_at(rows.into_iter(), need),
                        })
                    })
                    .collect();
                let mut others: Vec<OtherOffer> = p
                    .supply
                    .iter()
                    .filter(|s| !own.contains(&s.supplier_id) && s.stock > 0)
                    .map(|s| OtherOffer {
                        seller: bom.supplier_name(s.supplier_id),
                        authorized: s.authorized,
                        stock: s.stock,
                        unit_price: s.find_price(need),
                        lead_days: s.lead_days,
                    })
                    .collect();
                others.sort_by_key(|o| {
                    (
                        !o.authorized,
                        o.stock < i64::from(need),
                        o.unit_price.unwrap_or(Money::from_raw(i64::MAX)),
                    )
                });
                others.truncate(OTHERS_PER_MPN);
                choices.push(ExportChoice {
                    part_id: id,
                    mpn: p.part_number.clone(),
                    maker: maker(p),
                    role,
                    short: fits(p, need).is_none(),
                    supply,
                    others,
                });
            }

            // the preferred part
            let best = |cand: bool| {
                choices
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| (c.role == ChoiceRole::Candidate) == cand)
                    .filter_map(|(i, c)| {
                        parts
                            .get(&c.part_id)
                            .and_then(|p| fits(p, need))
                            .map(|m| (m, i))
                    })
                    .min()
                    .map(|(_, i)| i)
            };
            let (preferred, enough) = if let Some(i) = best(false).or_else(|| best(true)) {
                (Some(i), true)
            } else {
                let most = choices
                    .iter()
                    .enumerate()
                    .max_by_key(|(i, c)| {
                        let stock = parts
                            .get(&c.part_id)
                            .map(|p| p.supplier_stock_of(SupplyFilter::default()))
                            .unwrap_or(0);
                        (stock, std::cmp::Reverse(*i))
                    })
                    .map(|(i, _)| i);
                (most, false)
            };

            let match_group = match_group(item);
            let mut risks = vec![];
            if match_group.is_some() {
                risks.push("ONE MPN PER BOARD, DO NOT MIX REELS".to_owned());
            }
            if !enough {
                risks.push(format!("no supplier has {need}"));
            }
            let chip_rc = item
                .designators
                .first()
                .is_some_and(|d| d.starts_with(['R', 'C']));
            if !chip_rc {
                let mut makers: Vec<&str> = choices
                    .iter()
                    .filter(|c| c.role != ChoiceRole::Candidate && !c.maker.is_empty())
                    .map(|c| c.maker.as_str())
                    .collect();
                makers.sort_unstable();
                makers.dedup();
                match makers.as_slice() {
                    [] => risks.push("single maker".into()),
                    [one] => risks.push(format!("single maker ({one})")),
                    _ => {}
                }
            }

            let description = preferred
                .into_iter()
                .chain(0..choices.len())
                .filter_map(|i| parts.get(&choices[i].part_id))
                .find_map(supplier_description)
                .unwrap_or_else(|| local(item, "Description"));
            let footprint = local(item, "Footprint");
            let footprint = footprint.rsplit(':').next().unwrap_or("").to_owned();
            lines.push(ExportLine {
                number: idx + 1,
                designators: item.designators.clone(),
                qty,
                need,
                value: local(item, "Value"),
                footprint,
                description,
                match_group,
                choices,
                preferred,
                enough,
                risks,
            });
        }

        let mut source = format!("BOM {}", bom.id.0);
        if !bom.header.description.trim().is_empty() {
            source.push_str(" · ");
            source.push_str(bom.header.description.trim());
        }
        let bom_param = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| crate::endpoints::params::find(&bom.parameters, n))
                .map(|p| p.value.to_string())
                .filter(|s| !s.trim().is_empty())
        };
        if let Some(file) = bom_param(&["source file", "source_file", "file"]) {
            source.push_str(" · ");
            source.push_str(&file);
        }
        if let Some(commit) = bom_param(&["git commit", "git_commit", "commit"]) {
            source.push_str(" @ ");
            source.push_str(&commit);
        }

        Self {
            product: cx.product.clone(),
            board: cx.board.clone(),
            title: cx.title.clone(),
            date: cx.date,
            source,
            boards: cx.boards.max(1),
            suppliers,
            lines,
        }
    }
}

/// A unit price as `$0.0012` (four decimals under a dollar cent, else three)
pub fn price(m: Money) -> String {
    let v = m.raw() as f64 / 1e9;
    if v < 0.01 {
        format!("${v:.4}")
    } else {
        format!("${v:.3}")
    }
}

impl ExportChoice {
    /// The maker and each supplier column's stock and unit price, for a cell's note
    pub fn note(&self, suppliers: &[String]) -> String {
        let supply: Vec<String> = suppliers
            .iter()
            .zip(&self.supply)
            .filter_map(|(name, s)| {
                s.map(|s| {
                    let p = s
                        .unit_price
                        .map(|p| format!(" {}", price(p)))
                        .unwrap_or_default();
                    format!("{name} {}{p}", human(s.stock))
                })
            })
            .collect();
        let maker = if self.maker.is_empty() {
            "maker unknown"
        } else {
            &self.maker
        };
        let supply = if supply.is_empty() {
            "no supplier data".to_owned()
        } else {
            supply.join("  ")
        };
        format!("{maker}\n{supply}")
    }
}

/// `12k`, `1.2M`: stock counts short enough for a cell comment
pub fn human(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 10_000 {
        format!("{:.0}k", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

/// A fixture BOM for the tests here and the backend's workbook tests: every case the sheets show (enough stock,
/// a candidate taking over, nobody having enough, a single-maker IC, a match group, a broker's market offer)
#[doc(hidden)]
pub mod fixture {
    use super::{
        Bom, BomItem, CompanyId, ExportContext, HashSet, Money, NaiveDate, Part, PartId, PartSupply,
    };
    use crate::endpoints::bom::{BomGroupId, BomHeader, BomId, BomItemId};
    use crate::endpoints::folder::FolderId;
    use crate::endpoints::parameter::ParameterDefId;
    use crate::endpoints::params::Resolved;
    use crate::endpoints::part::{PartCrawl, PartSupplyId, PriceBreak};
    use crate::endpoints::user::UserId;
    use chrono::Utc;
    use rvariant::Variant;
    use std::collections::BTreeMap;

    pub const DK: CompanyId = CompanyId(2);
    pub const MOUSER: CompanyId = CompanyId(3791);
    pub const LCSC: CompanyId = CompanyId(3807);
    pub const ARROW: CompanyId = CompanyId(5000);
    pub const BROKER: CompanyId = CompanyId(5001);

    fn text(name: &str, v: &str) -> Resolved {
        Resolved::new(ParameterDefId(1), name, Variant::Str(v.into()))
    }

    fn supply(part: i32, sup: CompanyId, stock: i64, price: f64) -> PartSupply {
        PartSupply {
            id: PartSupplyId(0),
            part_id: PartId(part),
            supplier_id: sup,
            spn: format!("SPN{part}"),
            stock,
            updated: Utc::now(),
            packaging: "Cut Tape".into(),
            price_breaks: vec![
                PriceBreak {
                    break_quantity: 1,
                    break_price: Money::from_f64(price * 2.0),
                },
                PriceBreak {
                    break_quantity: 100,
                    break_price: Money::from_f64(price),
                },
            ],
            url: None,
            source: None,
            authorized: true,
            expires: None,
            moq: None,
            order_multiple: None,
            lead_days: None,
        }
    }

    fn market(mut s: PartSupply, authorized: bool) -> PartSupply {
        s.source = Some("octopart".into());
        s.authorized = authorized;
        s.lead_days = Some(42);
        s
    }

    fn part(id: i32, mfg: i32, mpn: &str, desc: &str, supply: Vec<PartSupply>) -> Part {
        let mut d = text("Description", desc);
        d.source = "digikey".into();
        d.raw = Some(desc.into());
        Part {
            id: PartId(id),
            mfg: CompanyId(mfg),
            part_number: mpn.into(),
            supply,
            folder_id: FolderId::root(),
            created: Utc::now(),
            updated: Utc::now(),
            is_incomplete: false,
            parameters: vec![d],
            warehouse_stock: vec![],
            is_watched: false,
            files: vec![],
            crawl: PartCrawl::default(),
        }
    }

    fn item(
        id: i32,
        des: &[&str],
        value: &str,
        fp: &str,
        choices: &[i32],
        cands: &[i32],
    ) -> BomItem {
        BomItem {
            id: BomItemId(id),
            bom_id: BomId(31),
            designators: des.iter().map(|s| s.to_string()).collect(),
            group_id: BomGroupId(1),
            part_choices: choices.iter().map(|i| PartId(*i)).collect(),
            candidates: cands.iter().map(|i| PartId(*i)).collect(),
            parameters: vec![text("Value", value), text("Footprint", fp)],
            match_group: None,
            part_numbers: None,
            hand_choices: HashSet::new(),
        }
    }

    pub fn bom() -> Bom {
        let mut items = vec![
            // 1: two resistors, both makers in stock: the cheaper one is preferred
            item(
                1,
                &["R1", "R2"],
                "10k",
                "Resistor_SMD:R_0402_1005Metric",
                &[10, 11],
                &[],
            ),
            // 2: an IC nobody has enough of, a candidate that is in stock
            item(
                2,
                &["U1"],
                "TPS7A02",
                "Package_TO_SOT_SMD:SOT-23-5",
                &[20],
                &[21],
            ),
            // 3: a connector from one maker, short everywhere
            item(
                3,
                &["J1"],
                "USB-C",
                "Connector_USB:USB_C_Receptacle",
                &[30],
                &[],
            ),
            // 4: a crystal with a match group
            item(
                4,
                &["Y1", "Y2"],
                "12MHz",
                "Crystal:Crystal_SMD_2016-4Pin",
                &[40, 41],
                &[],
            ),
        ];
        items[3].match_group = Some("Y:crystal pair".into());
        let parts = vec![
            part(
                10,
                100,
                "RC0402FR-0710KL",
                "RES 10K OHM 1% 1/16W 0402",
                vec![
                    supply(10, DK, 500_000, 0.002),
                    supply(10, LCSC, 2_000_000, 0.0005),
                ],
            ),
            part(
                11,
                101,
                "ERJ-2RKF1002X",
                "RES SMD 10K OHM 1% 1/10W 0402",
                vec![supply(11, DK, 90_000, 0.004)],
            ),
            part(
                20,
                102,
                "TPS7A0233PDBVR",
                "IC REG LINEAR 3.3V 200MA SOT23-5",
                vec![supply(20, DK, 10, 0.3), supply(20, MOUSER, 5, 0.31)],
            ),
            part(
                21,
                103,
                "TLV75533PDBVR",
                "IC REG LINEAR 3.3V 500MA SOT23-5",
                vec![supply(21, MOUSER, 3000, 0.12)],
            ),
            part(
                30,
                104,
                "USB4105-GF-A",
                "USB-C RECEPTACLE",
                vec![
                    supply(30, DK, 20, 0.6),
                    market(supply(30, ARROW, 900, 0.5), true),
                    market(supply(30, BROKER, 50_000, 0.2), false),
                ],
            ),
            part(
                40,
                105,
                "X2016-12M",
                "CRYSTAL 12MHZ 8PF SMD",
                vec![supply(40, DK, 4000, 0.15)],
            ),
            part(
                41,
                106,
                "ABM10-12.000MHZ",
                "CRYSTAL 12.0000MHZ 10PF SMD",
                vec![supply(41, MOUSER, 8000, 0.25)],
            ),
        ];
        // Arrow has enough of J1 (authorized): it fits, but the Arrow row is listed under the MPN, not as a column
        let referenced_parts = parts.into_iter().map(|p| (p.id, p)).collect();
        let suppliers = [
            (DK, "Digi-Key"),
            (MOUSER, "Mouser"),
            (LCSC, "LCSC"),
            (ARROW, "Arrow Electronics"),
            (BROKER, "Brokerama"),
        ]
        .into_iter()
        .map(|(id, n)| (id, n.to_owned()))
        .collect();
        Bom {
            id: BomId(31),
            header: BomHeader {
                id: BomId(31),
                pcb_id: None,
                description: "B153A from KiCad".into(),
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
            items,
            parameters: vec![
                text("source file", "b153a.kicad_sch"),
                text("git commit", "61e54bd"),
            ],
            referenced_parts,
            designators_kinds: vec![],
            suppliers,
        }
    }

    pub fn context() -> ExportContext {
        ExportContext {
            product: "Portwright".into(),
            board: "B153 rev B".into(),
            title: "USB-C serial adapter".into(),
            makers: [
                (100, "YAGEO"),
                (101, "Panasonic"),
                (102, "Texas Instruments"),
                (103, "Texas Instruments"),
                (104, "GCT"),
                (105, "Unknown"),
                (106, "Abracon"),
            ]
            .into_iter()
            .map(|(id, n)| (CompanyId(id), n.to_owned()))
            .collect(),
            boards: 25,
            date: NaiveDate::from_ymd_opt(2026, 11, 11).expect("the fixture date is valid"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::*;
    use super::*;

    #[test]
    fn export_model_of_the_fixture() {
        let e = BomExport::new(&bom(), &context());
        assert_eq!(e.suppliers, ["Digi-Key", "Mouser", "LCSC"]);
        assert_eq!(e.file_name(), "portwright_b153_rev_b_bom_11nov2026.xlsx");
        assert_eq!(
            e.source,
            "BOM 31 · B153A from KiCad · b153a.kicad_sch @ 61e54bd"
        );
        assert_eq!(e.part_count(), 6);
        assert_eq!(e.width(), 2);
    }

    #[test]
    fn fixture_resistor_line() {
        let e = BomExport::new(&bom(), &context());
        // R: the cheapest that is in stock (LCSC's YAGEO), chip R is not a single-maker risk
        let r = &e.lines[0];
        assert_eq!(r.need, 50);
        assert_eq!(r.footprint, "R_0402_1005Metric");
        assert_eq!(r.preferred, Some(0));
        assert!(r.enough && r.risks.is_empty());
        assert_eq!(r.description, "RES 10K OHM 1% 1/16W 0402");
        assert_eq!(r.choices[0].supply[2].unwrap().stock, 2_000_000);
        assert!(r.choices[0].supply[1].is_none());
    }

    #[test]
    fn fixture_regulator_line() {
        let e = BomExport::new(&bom(), &context());
        // U: the real choice is short, the candidate takes over
        let u = &e.lines[1];
        assert_eq!(u.preferred, Some(1));
        assert_eq!(u.choices[1].role, ChoiceRole::Candidate);
        assert!(u.enough && u.choices[0].short && !u.choices[1].short);
        assert_eq!(u.description, "IC REG LINEAR 3.3V 500MA SOT23-5");
        assert_eq!(u.risks, ["single maker (Texas Instruments)"]);
    }

    #[test]
    fn fixture_connector_line() {
        let e = BomExport::new(&bom(), &context());
        // J: only Arrow (market, authorized) has 25; the broker is listed after it
        let j = &e.lines[2];
        assert!(j.enough);
        assert_eq!(j.choices[0].others.len(), 2);
        assert_eq!(j.choices[0].others[0].seller, "Arrow Electronics");
        assert!(!j.choices[0].others[1].authorized);
        assert_eq!(j.risks, ["single maker (GCT)"]);
    }

    #[test]
    fn fixture_crystal_line() {
        let e = BomExport::new(&bom(), &context());
        // Y: match group first among the risks; "Unknown" is no maker
        let y = &e.lines[3];
        assert_eq!(y.match_group.as_deref(), Some("Y:crystal pair"));
        assert_eq!(y.risks[0], "ONE MPN PER BOARD, DO NOT MIX REELS");
        assert_eq!(y.choices[0].maker, "");
        assert_eq!(y.preferred, Some(0));
    }

    #[test]
    fn nobody_has_enough() {
        let mut cx = context();
        cx.boards = 100_000;
        let e = BomExport::new(&bom(), &cx);
        let u = &e.lines[1];
        assert!(!u.enough);
        // the most stock: the candidate's 3000
        assert_eq!(u.preferred, Some(1));
        assert!(u.risks.iter().any(|r| r == "no supplier has 100000"));
    }

    #[test]
    fn human_counts() {
        assert_eq!(human(950), "950");
        assert_eq!(human(12_500), "12k");
        assert_eq!(human(2_000_000), "2.0M");
    }
}
