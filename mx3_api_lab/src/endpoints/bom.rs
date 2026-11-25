use crate::endpoints::bom_chain::{BomChainId, ChainTarget, FreezeReason};
use crate::endpoints::companies::CompanyId;
use crate::endpoints::params::{self, Resolved};
use crate::endpoints::part::{CreatePart, Part, PartId, PartSupply, SupplyFilter};
use crate::endpoints::pcb::PcbId;
use crate::endpoints::user::UserId;
#[cfg(feature = "client")]
use crate::promise::Error;
#[cfg(feature = "client")]
use crate::request::RequestInitiator;
#[cfg(feature = "client")]
use crate::{MxClient, Promise};
use chrono::{DateTime, TimeDelta, Utc};
use mx3_api_core::money::Money;
use rvariant::{Variant, VariantTy};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct BomId(pub i32);

impl BomId {
    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn hack(id: i32) -> Self {
        BomId(id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, Hash, Copy)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct BomGroupId(pub i32);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct BomItemId(pub i32);

impl BomItemId {
    pub fn value(&self) -> i32 {
        self.0
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct BomItemPartChoiceId(i32);

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateBom {
    pub pcb_id: PcbId,
    pub description: String,
    pub user_id: UserId,
    pub company_id: CompanyId,
    pub items: Vec<CreateBomItem>,
    /// BOM-28: the chain the new revision goes to (default: the next revision of the PCB's `main`)
    #[serde(default)]
    pub chain: ChainTarget,
    /// The file has a Match column, so its match groups win over the previous revision's (BOM-26)
    #[serde(default)]
    pub has_match_column: bool,
    /// Show what the import would do, keep nothing
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateBomItem {
    pub designators: Vec<String>,
    /// Variant name, empty for default
    pub group: String,
    pub part_choices: Vec<PartId>,
    /// Parts added as candidates (BOM-24), e.g. the other makers of a part number (BOM-25)
    #[serde(default)]
    pub candidates: Vec<PartId>,
    pub part_choices_create: Vec<CreatePart>,
    pub parameters: HashMap<String, String>,
    /// BOM-26: lines with the same text (from a "Match" column) must use one MPN for the whole build, never
    /// mixed; `None` for no such constraint.
    #[serde(default)]
    pub match_group: Option<String>,
    /// BOM-28: the part numbers the file names for the line, in column order (kept on the line, so the next
    /// revision can tell the file's choices from those added by hand)
    #[serde(default)]
    pub part_numbers: Vec<String>,
    /// Line parameters set by hand on the previous revision, carried over (stored as manual, not as imported)
    #[serde(default)]
    pub manual_parameters: HashMap<String, String>,
    /// BOM-29: which of `part_choices`/`candidates` were picked by hand (carried over from a previous line's
    /// own `BomItem::hand_choices`), rather than matched by this import; stored as `bom_item_part_choice.by_hand`.
    /// Empty for a line the importer alone filled.
    #[serde(default)]
    pub hand_choices: HashSet<PartId>,
}

/// BOM-24: make candidates of a line real part choices
pub const EP_CONFIRM_CANDIDATES: &str = "/confirm_bom_item_candidates";
/// BOM-24: remove candidates from a line (real choices stay)
pub const EP_DROP_CANDIDATES: &str = "/drop_bom_item_candidates";
/// BOM-25: turn real part choices of a line into candidates (`mx3 bom tidy`); one real choice must stay
pub const EP_MAKE_CANDIDATES: &str = "/make_bom_item_candidates";
/// BOM-29: remove real part choices from a line entirely, unlike BOM-25's demote to candidate; refused on a
/// frozen revision by the usual freeze guard
pub const EP_REMOVE_CHOICES: &str = "/remove_bom_item_part_choices";
/// BOM-26: set or clear a line's match-group text
pub const EP_SET_MATCH: &str = "/set_bom_item_match";

/// BOM-26: a line's match-group text, or `None` to clear it (`mx3 bom line --match`/`--no-match`)
#[derive(Debug, Serialize, Deserialize)]
pub struct SetBomItemMatch {
    pub bom_item_id: BomItemId,
    pub match_group: Option<String>,
}

/// BOM-27: the BOM as the VHRD spreadsheet (header, BOM and Supply sheets)
pub const EP_BOM_XLSX: &str = "/bom/xlsx";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BomXlsxRequest {
    pub bom_id: BomId,
    /// Boards to order for: stock and prices are judged at this many times each line's quantity
    pub boards: u32,
}

/// The workbook and the name to save it under (`<product>_<board>_bom_<date>.xlsx`)
#[derive(Clone, Serialize, Deserialize)]
pub struct BomXlsxFile {
    pub file_name: String,
    #[serde(with = "mx3_api_core::base64_bytes")]
    pub bytes: Vec<u8>,
    /// Lines, and the most part numbers a line has
    pub lines: usize,
    pub width: usize,
}

#[derive(Serialize, Deserialize)]
pub struct AddBomItemPartChoices {
    pub bom_item_id: BomItemId,
    pub choices: HashSet<PartId>,
    pub choices_by_mpn: HashSet<String>,
}

/// BOM-24: confirm candidates of a line (they become real part choices) or drop them.
#[derive(Debug, Serialize, Deserialize)]
pub struct BomItemCandidates {
    pub bom_item_id: BomItemId,
    pub parts: HashSet<PartId>,
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn create_bom(&self, create_bom: &CreateBom, marker: &'static str) -> Promise<BomId> {
        let (req, rx) = RequestInitiator::post_json_arg("create_bom".into(), create_bom);
        match self.request_tx.send(req) {
            Ok(_) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }

    pub fn get_bom(&self, id: BomId, marker: &'static str) -> Promise<Bom> {
        let (req, rx) = RequestInitiator::post_json_arg("get_bom".into(), &id);
        match self.request_tx.send(req) {
            Ok(_) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }

    pub fn get_bom_headers(&self, marker: &'static str) -> Promise<Vec<BomHeader>> {
        let (req, rx) = RequestInitiator::get_no_args("get_bom_headers".into());
        match self.request_tx.send(req) {
            Ok(_) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }

    pub fn confirm_bom_item_candidates(
        &self,
        q: &BomItemCandidates,
        marker: &'static str,
    ) -> Promise<()> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_CONFIRM_CANDIDATES.into(), q);
        match self.request_tx.send(req) {
            Ok(_) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }

    pub fn drop_bom_item_candidates(
        &self,
        q: &BomItemCandidates,
        marker: &'static str,
    ) -> Promise<()> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_DROP_CANDIDATES.into(), q);
        match self.request_tx.send(req) {
            Ok(_) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }

    /// BOM-29: remove real part choices from a line entirely; refused on a frozen revision
    pub fn remove_bom_item_part_choices(
        &self,
        q: &BomItemCandidates,
        marker: &'static str,
    ) -> Promise<()> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_REMOVE_CHOICES.into(), q);
        match self.request_tx.send(req) {
            Ok(_) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }

    pub fn bom_xlsx(&self, q: &BomXlsxRequest, marker: &'static str) -> Promise<BomXlsxFile> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_BOM_XLSX.into(), q);
        match self.request_tx.send(req) {
            Ok(_) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }

    pub fn add_bom_item_part_choices(
        &self,
        add: AddBomItemPartChoices,
        marker: &'static str,
    ) -> Promise<()> {
        let (req, rx) = RequestInitiator::post_json_arg("add_bom_item_part_choices".into(), &add);
        match self.request_tx.send(req) {
            Ok(_) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }

    /// BOM-26: set or clear a line's match-group text
    pub fn set_bom_item_match(&self, q: &SetBomItemMatch, marker: &'static str) -> Promise<()> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_SET_MATCH.into(), q);
        match self.request_tx.send(req) {
            Ok(_) => Promise::waiting(rx, marker),
            Err(_) => Promise::error(Error::RequestDispatcherNotAvailable, marker),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Bom {
    pub id: BomId,
    pub header: BomHeader,
    pub groups: BTreeMap<BomGroupId, BomGroup>,
    pub items: Vec<BomItem>,
    pub parameters: Vec<Resolved>,
    pub referenced_parts: HashMap<PartId, Part>,
    pub designators_kinds: Vec<String>,
    /// Names of the suppliers the referenced parts' supply options name (INT-12)
    #[serde(default)]
    pub suppliers: HashMap<CompanyId, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BomHeader {
    pub id: BomId,
    pub pcb_id: Option<PcbId>,
    pub description: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub user_id: UserId,
    pub company_id: CompanyId,
    pub frozen: bool,
    /// BOM-28: the chain this revision belongs to, its name and the revision's number in it
    #[serde(default)]
    pub chain_id: BomChainId,
    #[serde(default)]
    pub chain: String,
    #[serde(default)]
    pub rev: i32,
    /// The label of a released revision
    #[serde(default)]
    pub release: Option<String>,
    #[serde(default)]
    pub freeze_reason: Option<FreezeReason>,
    #[serde(default)]
    pub frozen_at: Option<DateTime<Utc>>,
    /// The newest revision of its chain
    #[serde(default)]
    pub head: bool,
    /// Revision 1 of a split-off chain: the revision it was copied from
    #[serde(default)]
    pub split_from: Option<BomId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BomGroup {
    pub id: BomGroupId,
    pub bom_id: BomId,
    pub name: String,
    pub add_groups: Vec<BomGroupId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BomItem {
    pub id: BomItemId,
    pub bom_id: BomId,
    pub designators: Vec<String>,
    pub group_id: BomGroupId,
    /// Real part choices: the only ones used for cost, purchase, pick lists, supply sums and checks
    pub part_choices: Vec<PartId>,
    /// Suggested parts not vetted yet (e.g. the crawler's INT-11 makers): shown on the line, used for nothing
    /// until confirmed (BOM-24). Their parts are in `Bom::referenced_parts` too.
    #[serde(default)]
    pub candidates: Vec<PartId>,
    pub parameters: Vec<Resolved>,
    /// BOM-26: lines with the same text must use one MPN for the whole build quantity of a board, never mixed
    /// between suppliers or reels of different part numbers (`mx3 bom line --match`, the KiCad "Match" column).
    #[serde(default)]
    pub match_group: Option<String>,
    /// BOM-28: the part numbers the BOM file named for the line, in column order; `None` for lines imported
    /// before revision chains
    #[serde(default)]
    pub part_numbers: Option<Vec<String>>,
    /// BOM-29: the subset of `part_choices` and `candidates` someone picked by hand (add-choice, the UI,
    /// supplier search --add/--create, `bom suggest --add`), rather than the importer matching the file's part
    /// numbers; `bom_item_part_choice.by_hand`.
    #[serde(default)]
    pub hand_choices: HashSet<PartId>,
}

#[derive(Copy, Clone)]
pub enum ParameterSource {
    /// Use only data imported from the board
    OnlyLocal,
    /// Use only data from supplier's API's
    OnlyGlobal,
    /// First search in imported data from a board, then from suppliers
    PreferLocal,
    /// First search in data from suppliers, then from a board
    PreferGlobal,
}

impl BomItem {
    /// Find a parameter value by name, prioritizing BOM parameters over global parameters if prefer_local = true and global otherwise
    pub fn find_parameter(
        &self,
        name: &str,
        parts: &HashMap<PartId, Part>,
        src: ParameterSource,
    ) -> Option<Variant> {
        match src {
            ParameterSource::OnlyLocal => self.find_parameter_only_local(name),
            ParameterSource::OnlyGlobal => self.find_parameter_only_global(name, parts),
            ParameterSource::PreferLocal => {
                if let Some(value) = self.find_parameter_only_local(name) {
                    return Some(value);
                }
                self.find_parameter_only_global(name, parts)
            }
            ParameterSource::PreferGlobal => {
                if let Some(value) = self.find_parameter_only_global(name, parts) {
                    return Some(value);
                }
                self.find_parameter_only_local(name)
            }
        }
    }

    /// Find a parameter value by name, considering only BOM parameters
    pub fn find_parameter_only_local(&self, name: &str) -> Option<Variant> {
        params::find(&self.parameters, name).map(|p| p.value.clone())
    }

    /// Find a parameter value by name, considering only Part's global parameters (pulled from e.g., Digi-Key API)
    pub fn find_parameter_only_global(
        &self,
        name: &str,
        parts: &HashMap<PartId, Part>,
    ) -> Option<Variant> {
        for id in &self.part_choices {
            let Some(part) = parts.get(id) else { continue };
            if let Some(p) = params::find(&part.parameters, name) {
                return Some(p.value.clone());
            }
        }
        None
    }

    pub fn find_parameter_many(
        &self,
        names: &[&str],
        parts: &HashMap<PartId, Part>,
        src: ParameterSource,
    ) -> Option<Variant> {
        for name in names {
            if let Some(value) = self.find_parameter(name, parts, src) {
                return Some(value);
            }
        }
        None
    }

    pub fn find_parameter_all(
        &self,
        names: &[&str],
        parts: &HashMap<PartId, Part>,
        src: ParameterSource,
    ) -> Vec<String> {
        let mut values = vec![];
        for name in names {
            if let Some(value) = self.find_parameter(name, parts, src)
                && let Ok(s) = value.as_string()
            {
                values.push(s);
            }
        }
        values
    }

    pub fn is_in_group(
        &self,
        group_id: BomGroupId,
        groups: &BTreeMap<BomGroupId, BomGroup>,
    ) -> bool {
        if self.group_id == group_id {
            true
        } else {
            is_in_group_recursive(self.group_id, group_id, groups)
        }
    }

    pub fn designators_range(&self, max_unelided: usize) -> String {
        if self.designators.len() <= max_unelided {
            self.designators.join(", ")
        } else {
            let head = &self.designators[..max_unelided - 2].join(", ");
            let tail = if let Some(last) = self.designators.last() {
                last.as_str()
            } else {
                ""
            };
            format!("{head} ... {tail}")
        }
    }

    pub fn best_price_for_break(
        &self,
        quantity: u32,
        parts: &HashMap<PartId, Part>,
    ) -> Option<Money> {
        self.best_offer(quantity, parts, SupplyFilter::default())
            .map(|o| o.unit_price)
    }

    /// The cheapest way to buy `quantity` parts of this line over its real part choices and the supply options
    /// `only` takes (one supplier's: INT-12; brokers or not: INT-16).
    pub fn best_offer<'a>(
        &self,
        quantity: u32,
        parts: &'a HashMap<PartId, Part>,
        only: SupplyFilter,
    ) -> Option<Offer<'a>> {
        self.part_choices
            .iter()
            .filter_map(|id| parts.get(id))
            .filter_map(|part| {
                part.best_offer(quantity, only)
                    .map(|(supply, unit_price)| Offer {
                        part,
                        supply,
                        unit_price,
                    })
            })
            .min_by(|a, b| {
                a.unit_price
                    .cmp(&b.unit_price)
                    .then(b.supply.stock.cmp(&a.supply.stock))
            })
    }

    /// The most any supply option `only` takes has in stock, summed over the real part choices
    pub fn supplier_stock_of(&self, parts: &HashMap<PartId, Part>, only: SupplyFilter) -> u64 {
        self.part_choices
            .iter()
            .filter_map(|id| parts.get(id))
            .map(|part| part.supplier_stock_of(only))
            .sum()
    }

    pub fn warehouse_stock(&self, parts: &HashMap<PartId, Part>) -> u64 {
        self.part_choices
            .iter()
            .filter_map(|id| parts.get(id))
            .map(|part| part.warehouse_stock())
            .sum()
    }

    /// BOM-26: one box of a real choice, across the whole line, with `required` or more of that choice's own
    /// part alone (never summed across boxes or MPNs); the one with the most stock wins ties. `None` when no
    /// single box has enough.
    pub fn single_box_for(&self, required: u32, parts: &HashMap<PartId, Part>) -> Option<String> {
        self.part_choices
            .iter()
            .filter_map(|id| parts.get(id))
            .flat_map(|part| part.warehouse_stock.iter().map(move |b| (part.id, b)))
            .filter(|(part_id, b)| b.part_count(*part_id) >= required)
            .max_by_key(|(part_id, b)| (b.part_count(*part_id), b.path_serial()))
            .map(|(_, b)| b.path_serial())
    }

    pub fn manufacturer_stock(&self, parts: &HashMap<PartId, Part>) -> u64 {
        self.part_choices
            .iter()
            .filter_map(|id| parts.get(id))
            .map(|part| part.manufacturer_stock())
            .sum()
    }

    pub fn supplier_stock(&self, parts: &HashMap<PartId, Part>) -> u64 {
        self.supplier_stock_of(parts, SupplyFilter::default())
    }

    pub fn total_stock(&self, parts: &HashMap<PartId, Part>) -> u64 {
        self.part_choices
            .iter()
            .filter_map(|id| parts.get(id))
            .map(|part| part.total_stock())
            .sum()
    }

    fn collect_parameter_only_global(
        &self,
        names: &[&str],
        parts: &HashMap<PartId, Part>,
    ) -> Vec<Variant> {
        let mut values = vec![];
        for id in &self.part_choices {
            let Some(part) = parts.get(id) else { continue };
            for p in part.parameters.iter().filter(|p| !p.retracted) {
                for name in names {
                    if p.name.eq_ignore_ascii_case(name) {
                        values.push(p.value.clone());
                    }
                }
            }
        }
        values
    }

    /// Compare the BOM line's own R, L, C parameters with the parts chosen for it, both against what a supplier
    /// states and against what each choice's own MPN decodes to (BOM-9): a part a supplier doesn't know, or whose
    /// supplier data doesn't cover a parameter, is checked too. Every distinct designator prefix on the line is
    /// checked; the mismatches are returned so the line can say what is wrong (BOM-20).
    pub fn check(&self, parts: &HashMap<PartId, Part>) -> Result<(), Vec<CheckMismatch>> {
        let mut kinds: Vec<char> = self
            .designators
            .iter()
            .filter_map(|d| d.chars().next())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        kinds.sort_unstable();
        kinds.dedup();
        let mut mismatches = vec![];
        for kind in kinds {
            match kind {
                'C' => {
                    self.compare_params(
                        "value",
                        &["capacitance"],
                        Some("capacitance"),
                        Rule::Equal,
                        parts,
                        &mut mismatches,
                    );
                    self.compare_params(
                        "voltage",
                        &["voltage - rated"],
                        Some("rated_voltage"),
                        Rule::AtLeast,
                        parts,
                        &mut mismatches,
                    );
                    self.compare_params(
                        "dielectric",
                        &["temperature coefficient"],
                        Some("dielectric"),
                        Rule::Equal,
                        parts,
                        &mut mismatches,
                    );
                }
                'R' => self.compare_params(
                    "value",
                    &["resistance"],
                    Some("resistance"),
                    Rule::Equal,
                    parts,
                    &mut mismatches,
                ),
                'L' => self.compare_params(
                    "value",
                    &["inductance"],
                    Some("inductance"),
                    Rule::Equal,
                    parts,
                    &mut mismatches,
                ),
                _ => {}
            }
            // Tolerance isn't compared yet: `compare_values` has no rule for `Variant::Tolerance` (next for BOM-9).
            if matches!(kind, 'C' | 'R' | 'L') {
                self.compare_params(
                    "case",
                    &["case"],
                    Some("case_eia"),
                    Rule::Equal,
                    parts,
                    &mut mismatches,
                );
            }
        }
        if mismatches.is_empty() {
            Ok(())
        } else {
            Err(mismatches)
        }
    }

    /// Compare the BOM's own `local_name` value against what a supplier states under `supply_names` and, when
    /// `mpn_key` is given, against the matching key of each choice's own decoded MPN (BOM-9) too.
    fn compare_params(
        &self,
        local_name: &str,
        supply_names: &[&str],
        mpn_key: Option<&str>,
        rule: Rule,
        parts: &HashMap<PartId, Part>,
        mismatches: &mut Vec<CheckMismatch>,
    ) {
        let Some(local) = self.find_parameter_only_local(local_name) else {
            return;
        };
        for supply in self.collect_parameter_only_global(supply_names, parts) {
            if let Err(m) = compare_values(local_name, &local, &supply, rule)
                && !mismatches.contains(&m)
            {
                mismatches.push(m);
            }
        }
        let Some(mpn_key) = mpn_key else { return };
        for value in self.collect_decoded(mpn_key, parts) {
            let Some(ty) = mpn_value_ty(&value) else {
                continue;
            };
            let decoded = Variant::from_str(value.to_string(), &ty);
            if let Err(m) = compare_values(local_name, &local, &decoded, rule)
                && !mismatches.contains(&m)
            {
                mismatches.push(m);
            }
        }
    }

    /// Every choice's own MPN decoded against the built-in schemes, the value of `key` where it names one.
    fn collect_decoded(&self, key: &str, parts: &HashMap<PartId, Part>) -> Vec<mpn_schemes::Value> {
        let mut values = vec![];
        for id in &self.part_choices {
            let Some(part) = parts.get(id) else { continue };
            for p in decode_mpn(&part.part_number) {
                if p.key == key {
                    values.push(p.value);
                }
            }
        }
        values
    }
}

/// The schemes built into `mpn_schemes`, loaded once. Errors are a broken build (the crate's own tests prove the
/// embedded files load); kept rather than panicking, so a client never crashes over it, just decodes nothing.
fn scheme_set() -> &'static mpn_schemes::SchemeSet {
    static SET: std::sync::OnceLock<mpn_schemes::SchemeSet> = std::sync::OnceLock::new();
    SET.get_or_init(|| {
        mpn_schemes::SchemeSet::from_texts(
            mpn_schemes::SchemeSet::embedded_files()
                .iter()
                .map(|(p, t)| (p.to_string(), t.to_string())),
            true,
        )
        .0
    })
}

/// A part's own MPN decoded against the built-in schemes (BOM-9, docs/mpn-schemes.md §4, §7), independent of
/// whatever `mpn_decode` fact the backend may or may not have written on the part: a part whose maker is the
/// "Unknown" placeholder (an MPN no supplier knows, straight from the BOM's own MPN column) is never decoded
/// there, and a fact the backend did write may be hidden behind a higher-ranked supplier value. Only the best
/// complete reading's parameters; an unparseable or merely partial MPN decodes to nothing, not a guess.
fn decode_mpn(part_number: &str) -> Vec<mpn_schemes::Param> {
    scheme_set()
        .parse(part_number, None)
        .into_iter()
        .find(|p| p.complete)
        .map(|p| p.params)
        .unwrap_or_default()
}

/// The [`VariantTy`] a decoded [`mpn_schemes::Value`] converts through, so it compares with the BOM's own value the
/// same way a supplier's fact does.
fn mpn_value_ty(value: &mpn_schemes::Value) -> Option<VariantTy> {
    use rvariant::NumberTy;
    use rvariant::si_dynamic::Registry;
    match value {
        mpn_schemes::Value::Quantity(q) => {
            Registry::new()
                .parse_unit(q.unit)
                .ok()
                .map(|unit| VariantTy::SI {
                    number_ty: NumberTy::F64,
                    unit,
                })
        }
        mpn_schemes::Value::Tolerance(_) => Some(VariantTy::Tolerance(NumberTy::F64)),
        mpn_schemes::Value::Text(_) => Some(VariantTy::Str),
    }
}

/// One BOM parameter that disagrees with the chosen part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckMismatch {
    pub what: String,
    pub bom: String,
    pub part: String,
}

impl std::fmt::Display for CheckMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: BOM {} but part {}", self.what, self.bom, self.part)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Rule {
    Equal,
    /// The part must be rated at least as high as the BOM asks (voltage).
    AtLeast,
}

/// Values closer than this (relative) are equal: the f32 round trip of `13k7` vs `13.7 kΩ` must not flag.
const RELATIVE_TOLERANCE: f64 = 1e-3;

/// Compare the BOM's value with the part's, both in the part value's own type: a number with a unit is parsed
/// into that unit and compared with a relative tolerance, a text (dielectric) compared ignoring case.
fn compare_values(
    what: &str,
    local: &Variant,
    supply: &Variant,
    rule: Rule,
) -> Result<(), CheckMismatch> {
    let mismatch = || CheckMismatch {
        what: what.to_string(),
        bom: local.to_string(),
        part: supply.to_string(),
    };
    if matches!(supply, Variant::Empty) || matches!(local, Variant::Empty) {
        return Ok(());
    }
    let ty = VariantTy::from(supply);
    let Ok(local_in_ty) = local.clone().convert_to(&ty) else {
        // The BOM text isn't a value of that kind at all (e.g. "DNP" in a value cell): nothing to compare.
        return Ok(());
    };
    match (&local_in_ty, supply) {
        (Variant::SI { .. }, Variant::SI { .. }) => {
            let (Some(l), Some(s)) = (local_in_ty.num_base_f64(), supply.num_base_f64()) else {
                return Ok(());
            };
            let close = (l - s).abs() <= RELATIVE_TOLERANCE * l.abs().max(s.abs());
            let ok = match rule {
                Rule::Equal => close,
                Rule::AtLeast => close || s > l,
            };
            if ok { Ok(()) } else { Err(mismatch()) }
        }
        (Variant::Str(l), Variant::Str(s)) => {
            if canonical_text(l) == canonical_text(s) {
                Ok(())
            } else {
                Err(mismatch())
            }
        }
        _ => Ok(()),
    }
}

/// Lowercase text with dielectric synonyms folded (NP0 is C0G).
fn canonical_text(s: &str) -> String {
    let s = s.trim().to_ascii_lowercase();
    match s.as_str() {
        "np0" => "c0g".to_string(),
        _ => s,
    }
}

fn is_in_group_recursive(
    group_id: BomGroupId,
    parent: BomGroupId,
    groups: &BTreeMap<BomGroupId, BomGroup>,
) -> bool {
    let Some(group) = groups.get(&parent) else {
        return false;
    };
    if group.id == group_id {
        true
    } else {
        for add_group in &group.add_groups {
            if is_in_group_recursive(group_id, *add_group, groups) {
                return true;
            }
        }
        false
    }
}

#[derive(Serialize)]
pub struct BomSearchListRow {
    pub line_number: u32,
    pub boxes_serials: String,
    pub count: u32,
    pub found: String,
    pub value: String,
    pub description: String,
    pub designators: String,
}

#[derive(Clone, Serialize)]
pub struct BomPurchaseRow {
    pub designators: String,
    pub value: String,
    pub description: String,
    pub supplier: String,
    #[serde(skip)]
    pub supplier_id: Option<CompanyId>,
    pub spn: String,
    pub quantity: u32,
    pub packaging: String,
    /// Unit price at this quantity, empty when no supplier has one
    pub unit_price: String,
    /// The supplier's product page, when it has one (Mouser)
    pub url: String,
    /// BOM-26: why a matched line has no supplier (none can cover the whole quantity alone); empty otherwise
    pub note: String,
}

/// One way to buy a line's parts: which part, from which supply option, at what unit price (INT-12).
#[derive(Clone, Copy)]
pub struct Offer<'a> {
    pub part: &'a Part,
    pub supply: &'a PartSupply,
    pub unit_price: Money,
}

/// BOM-26: what one real choice of a matched line would take to alone cover the whole required quantity.
enum MatchPick<'a> {
    /// Its own warehouse stock already covers it; nothing to buy
    InStock,
    /// Its own stock falls short by `to_buy`, topped up at `offer`'s price
    Buy { to_buy: u32, offer: Offer<'a> },
}

/// For a matched line (BOM-26), the real choice that alone can supply the whole `required` quantity — already
/// in the warehouse, or topped up from one supplier — cheapest topped-up choice first. `None` when no single
/// choice can, so the line is never bought from two MPNs.
fn match_group_pick<'a>(
    item: &BomItem,
    required: u32,
    parts: &'a HashMap<PartId, Part>,
    only: SupplyFilter,
) -> Option<MatchPick<'a>> {
    let mut best: Option<(u32, Offer<'a>)> = None;
    for part in item.part_choices.iter().filter_map(|id| parts.get(id)) {
        let own_stock = part.warehouse_stock();
        if own_stock >= required as u64 {
            return Some(MatchPick::InStock);
        }
        let to_buy = required - own_stock as u32;
        let Some((supply, unit_price)) = part.best_offer(to_buy, only) else {
            continue;
        };
        let offer = Offer {
            part,
            supply,
            unit_price,
        };
        if best.as_ref().is_none_or(|(_, b)| unit_price < b.unit_price) {
            best = Some((to_buy, offer));
        }
    }
    best.map(|(to_buy, offer)| MatchPick::Buy { to_buy, offer })
}

impl Bom {
    /// A supplier's name as the BOM knows it, else its company id
    pub fn supplier_name(&self, id: CompanyId) -> String {
        self.suppliers
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("#{}", id.0))
    }

    /// The supplier the BOM's supply options name by `name`, ignoring case, spaces and punctuation ("digikey" is
    /// "Digi-Key"); `None` when no supply option names it.
    pub fn supplier_by_name(&self, name: &str) -> Option<CompanyId> {
        let key = |s: &str| {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        };
        let want = key(name);
        self.suppliers
            .iter()
            .find(|(_, n)| key(n) == want)
            .map(|(id, _)| *id)
    }

    // TODO: show in a table first with search/filter and save to CSV/XLS from there
    pub fn search_list(&self, assemble_count: u32) -> Vec<BomSearchListRow> {
        let mut search_list = vec![];
        for item in &self.items {
            let required = item.designators.len() as u32 * assemble_count;
            let boxes_serials = if let Some(group) = &item.match_group {
                // BOM-26: the whole quantity from one box, never split across reels of different MPNs
                match item.single_box_for(required, &self.referenced_parts) {
                    Some(serial) => serial,
                    None => format!("⚠ one MPN per board ({group}): no single box has {required}"),
                }
            } else {
                let mut boxes_serials = vec![];
                for part_id in &item.part_choices {
                    let Some(part) = self.referenced_parts.get(part_id) else {
                        continue;
                    };
                    for wh_stock in &part.warehouse_stock {
                        let path_serial = wh_stock.path_serial();
                        if !boxes_serials.contains(&path_serial) {
                            boxes_serials.push(wh_stock.path_serial());
                        }
                    }
                }
                boxes_serials.join(", ")
            };
            search_list.push(BomSearchListRow {
                line_number: 0,
                boxes_serials,
                count: required,
                found: String::new(),
                value: item
                    .find_parameter(
                        "value",
                        &self.referenced_parts,
                        ParameterSource::PreferGlobal,
                    )
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                description: item
                    .find_parameter(
                        "description",
                        &self.referenced_parts,
                        ParameterSource::PreferGlobal,
                    )
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                designators: item.designators_range(4),
            });
        }
        search_list.sort_unstable_by(|a, b| a.boxes_serials.cmp(&b.boxes_serials));
        for (idx, row) in search_list.iter_mut().enumerate() {
            row.line_number = idx as u32 + 1;
        }
        search_list
    }

    /// What to buy for `pcba_count` boards: per line the parts the warehouse lacks, from the cheapest supply option
    /// at that quantity over every supplier, or one supplier's only (`only`, INT-12). Options that tie on the best
    /// price each get a row; a line no supplier prices still gets one, without supplier. A matched line (BOM-26)
    /// never gets more than one row: one MPN for its whole quantity, cheapest choice with enough stock, or a row
    /// that says none has enough instead of splitting the buy across MPNs.
    pub fn purchase_list(&self, pcba_count: u32, only: SupplyFilter) -> Vec<BomPurchaseRow> {
        let mut purchase_list = vec![];
        for item in &self.items {
            let required_parts = pcba_count * item.designators.len() as u32;
            let value = item
                .find_parameter(
                    "Value",
                    &self.referenced_parts,
                    ParameterSource::PreferGlobal,
                )
                .map(|v| v.to_string())
                .unwrap_or_default();
            let description = item
                .find_parameter(
                    "Description",
                    &self.referenced_parts,
                    ParameterSource::PreferGlobal,
                )
                .map(|v| v.to_string())
                .unwrap_or_default();
            let base_row = |quantity: u32| BomPurchaseRow {
                designators: item.designators_range(4),
                value: value.clone(),
                description: description.clone(),
                supplier: String::new(),
                supplier_id: None,
                spn: String::new(),
                quantity,
                packaging: String::new(),
                unit_price: String::new(),
                url: String::new(),
                note: String::new(),
            };

            if let Some(group) = &item.match_group {
                match match_group_pick(item, required_parts, &self.referenced_parts, only) {
                    Some(MatchPick::InStock) => {} // one MPN already covers the whole build
                    Some(MatchPick::Buy { to_buy, offer }) => {
                        let mut row = base_row(to_buy);
                        row.supplier_id = Some(offer.supply.supplier_id);
                        row.supplier = self.supplier_name(offer.supply.supplier_id);
                        row.spn = offer.supply.spn.clone();
                        row.packaging = offer.supply.packaging.clone();
                        row.unit_price = offer.unit_price.to_string();
                        row.url = offer.supply.url.clone().unwrap_or_default();
                        purchase_list.push(row);
                    }
                    None => {
                        let mut row = base_row(required_parts);
                        row.note = format!(
                            "one MPN per board ({group}): no single choice can supply {required_parts} alone"
                        );
                        purchase_list.push(row);
                    }
                }
                continue;
            }

            let in_warehouse = item.warehouse_stock(&self.referenced_parts) as u32;
            if in_warehouse >= required_parts {
                continue;
            }
            let missing_parts = required_parts - in_warehouse;
            let purchase_row = base_row(missing_parts);
            let Some(best) = item.best_offer(missing_parts, &self.referenced_parts, only) else {
                // no purchase options, but save the row anyway
                purchase_list.push(purchase_row);
                continue;
            };
            for part in item
                .part_choices
                .iter()
                .filter_map(|id| self.referenced_parts.get(id))
            {
                for supply in part.supply_of(only) {
                    // each option at the best price
                    if supply.find_price(missing_parts) == Some(best.unit_price) {
                        let mut row = purchase_row.clone();
                        row.supplier_id = Some(supply.supplier_id);
                        row.supplier = self.supplier_name(supply.supplier_id);
                        row.spn = supply.spn.clone();
                        row.packaging = supply.packaging.clone();
                        row.unit_price = best.unit_price.to_string();
                        row.url = supply.url.clone().unwrap_or_default();
                        purchase_list.push(row);
                    }
                }
            }
        }
        purchase_list
    }

    pub fn filter_supply<F: Fn(&PartSupply) -> bool>(&self, f: F) -> HashSet<PartId> {
        let mut outdated = HashSet::new();
        for part in self.referenced_parts.values() {
            for supply in &part.supply {
                if f(supply) {
                    outdated.insert(supply.part_id);
                }
            }
        }
        outdated
    }

    pub fn outdated_supply<F: Fn(&TimeDelta) -> bool>(&self, f: F) -> HashSet<PartId> {
        let now = Utc::now();
        self.filter_supply(|supply| {
            let updated_dt = now - supply.updated;
            f(&updated_dt)
        })
    }

    pub fn part_count(&self) -> u32 {
        let count: usize = self.items.iter().map(|i| i.designators.len()).sum();
        count as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoints::folder::FolderId;
    use crate::endpoints::parameter::ParameterDefId;
    use rvariant::Number;
    use rvariant::si_dynamic::{BaseUnit, Unit};

    fn param(id: i64, name: &str, value: Variant) -> Resolved {
        Resolved::new(ParameterDefId(id as i32), name, value)
    }

    fn si(value: f32, base: BaseUnit) -> Variant {
        Variant::SI {
            value: Number::F32(value.into()),
            unit: Unit::base(base).into(),
        }
    }

    fn part(params: Vec<Resolved>) -> Part {
        Part {
            id: PartId(1),
            mfg: CompanyId(1),
            part_number: "X".into(),
            supply: vec![],
            folder_id: FolderId::root(),
            created: Utc::now(),
            updated: Utc::now(),
            is_incomplete: false,
            parameters: params,
            warehouse_stock: vec![],
            is_watched: false,
            files: vec![],
            crawl: Default::default(),
        }
    }

    /// A line with `designator`, the BOM's own parameters and one chosen part with `supply` parameters.
    fn check(
        designator: &str,
        bom: Vec<(&str, Variant)>,
        supply: Vec<(&str, Variant)>,
    ) -> Result<(), Vec<CheckMismatch>> {
        let item = BomItem {
            id: BomItemId(1),
            bom_id: BomId(1),
            designators: vec![designator.to_string()],
            group_id: BomGroupId(1),
            part_choices: vec![PartId(1)],
            candidates: vec![],
            parameters: bom
                .into_iter()
                .enumerate()
                .map(|(i, (n, v))| param(i as i64, n, v))
                .collect(),
            match_group: None,
            part_numbers: None,
            hand_choices: HashSet::new(),
        };
        let supply = supply
            .into_iter()
            .enumerate()
            .map(|(i, (n, v))| param(100 + i as i64, n, v))
            .collect();
        item.check(&HashMap::from([(PartId(1), part(supply))]))
    }

    fn s(v: &str) -> Variant {
        Variant::Str(v.to_string())
    }

    /// A line with `designator` and the BOM's own parameters, one chosen part known only by `part_number` (no
    /// supplier facts at all): what BOM-9's MPN decode has to work with on its own.
    fn check_mpn(
        designator: &str,
        bom: Vec<(&str, Variant)>,
        part_number: &str,
    ) -> Result<(), Vec<CheckMismatch>> {
        let item = BomItem {
            id: BomItemId(1),
            bom_id: BomId(1),
            designators: vec![designator.to_string()],
            group_id: BomGroupId(1),
            part_choices: vec![PartId(1)],
            candidates: vec![],
            parameters: bom
                .into_iter()
                .enumerate()
                .map(|(i, (n, v))| param(i as i64, n, v))
                .collect(),
            match_group: None,
            part_numbers: None,
            hand_choices: HashSet::new(),
        };
        let mut chosen = part(vec![]);
        chosen.part_number = part_number.to_string();
        item.check(&HashMap::from([(PartId(1), chosen)]))
    }

    #[test]
    fn resistor_value_mismatch_is_flagged() {
        // B200A R201: the BOM says 13k3, the chosen part is 13k7.
        let r = check(
            "R201",
            vec![("value", s("13k3"))],
            vec![("resistance", si(13_700.0, BaseUnit::Ohm))],
        );
        let m = r.unwrap_err();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].what, "value");
    }

    #[test]
    fn retracted_part_value_is_not_compared() {
        // PRM-19: a value no source states any more is no value for the check
        let item = BomItem {
            id: BomItemId(1),
            bom_id: BomId(1),
            designators: vec!["R1".into()],
            group_id: BomGroupId(1),
            part_choices: vec![PartId(1)],
            candidates: vec![],
            parameters: vec![param(1, "value", s("10k"))],
            match_group: None,
            part_numbers: None,
            hand_choices: HashSet::new(),
        };
        let mut gone = param(2, "resistance", si(1_000.0, BaseUnit::Ohm));
        gone.retracted = true;
        item.check(&HashMap::from([(PartId(1), part(vec![gone]))]))
            .unwrap();
    }

    #[test]
    fn resistor_value_match_passes_in_any_notation() {
        for v in ["13k7", "13.7k", "13700", "13.7 kΩ"] {
            check(
                "R1",
                vec![("value", s(v))],
                vec![("resistance", si(13_700.0, BaseUnit::Ohm))],
            )
            .unwrap();
        }
    }

    #[test]
    fn capacitor_value_mismatch_is_flagged() {
        let r = check(
            "C5",
            vec![("value", s("100nF"))],
            vec![("capacitance", si(1e-6, BaseUnit::Farad))],
        );
        assert_eq!(r.unwrap_err()[0].what, "value");
        check(
            "C5",
            vec![("value", s("100nF"))],
            vec![("capacitance", si(1e-7, BaseUnit::Farad))],
        )
        .unwrap();
    }

    #[test]
    fn inductor_value_mismatch_is_flagged() {
        let r = check(
            "L1",
            vec![("value", s("10uH"))],
            vec![("inductance", si(4.7e-6, BaseUnit::Henry))],
        );
        assert!(r.is_err());
    }

    #[test]
    fn dielectric_mismatch_is_flagged() {
        let r = check(
            "C5",
            vec![("dielectric", s("X7R"))],
            vec![("temperature coefficient", s("C0G"))],
        );
        assert_eq!(r.unwrap_err()[0].what, "dielectric");
        check(
            "C5",
            vec![("dielectric", s("x7r"))],
            vec![("temperature coefficient", s("X7R"))],
        )
        .unwrap();
        check(
            "C5",
            vec![("dielectric", s("NP0"))],
            vec![("temperature coefficient", s("C0G"))],
        )
        .unwrap();
    }

    #[test]
    fn voltage_rating_must_be_at_least_the_bom_voltage() {
        let rated = |v| vec![("voltage - rated", si(v, BaseUnit::Volt))];
        assert!(check("C5", vec![("voltage", s("50V"))], rated(25.0)).is_err());
        check("C5", vec![("voltage", s("50V"))], rated(50.0)).unwrap();
        check("C5", vec![("voltage", s("50V"))], rated(100.0)).unwrap();
    }

    #[test]
    fn missing_or_unparseable_values_are_not_flagged() {
        check("R1", vec![], vec![("resistance", si(100.0, BaseUnit::Ohm))]).unwrap();
        check(
            "R1",
            vec![("value", s("DNP"))],
            vec![("resistance", si(100.0, BaseUnit::Ohm))],
        )
        .unwrap();
        check("R1", vec![("value", s("100"))], vec![]).unwrap();
    }

    #[test]
    fn other_designators_are_not_checked() {
        check(
            "U1",
            vec![("value", s("13k3"))],
            vec![("resistance", si(13_700.0, BaseUnit::Ohm))],
        )
        .unwrap();
    }

    // BOM-9: a part no supplier knows (no supply facts at all) is checked against its own MPN, decoded through
    // `mpn_schemes` (MPN-1) right here, independent of whatever the backend's decode worker may have done.

    #[test]
    fn wrong_resistor_is_caught_from_its_own_mpn_when_no_supplier_knows_it() {
        // B153A R219/R222 and B200A R201: the line wants 13k7, but the part actually placed (Yageo RC, ±1 %,
        // 0402) is 13k3 and no supplier has it, so the old check (supplier facts only) missed it.
        let r = check_mpn("R219", vec![("value", s("13k7"))], "RC0402FR-0713K3L");
        let m = r.unwrap_err();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].what, "value");
    }

    #[test]
    fn resistor_value_from_its_own_mpn_matches_when_the_right_part_is_chosen() {
        check_mpn("R219", vec![("value", s("13k7"))], "RC0402FR-0713K7L").unwrap();
    }

    #[test]
    fn capacitor_dielectric_and_voltage_are_caught_from_their_own_mpn() {
        // CL05B104KB5NNNC (Samsung CL): 100 nF, X7R, 50 V, 0402.
        let r = check_mpn("C9", vec![("dielectric", s("C0G"))], "CL05B104KB5NNNC");
        assert_eq!(r.unwrap_err()[0].what, "dielectric");
        let r = check_mpn("C9", vec![("voltage", s("100V"))], "CL05B104KB5NNNC");
        assert_eq!(r.unwrap_err()[0].what, "voltage");
        check_mpn("C9", vec![("voltage", s("25V"))], "CL05B104KB5NNNC").unwrap();
        check_mpn("C9", vec![("dielectric", s("X7R"))], "CL05B104KB5NNNC").unwrap();
    }

    #[test]
    fn case_mismatch_is_caught_from_its_own_mpn() {
        let r = check_mpn("R219", vec![("case", s("0603"))], "RC0402FR-0713K7L");
        assert_eq!(r.unwrap_err()[0].what, "case");
        check_mpn("R219", vec![("case", s("0402"))], "RC0402FR-0713K7L").unwrap();
    }

    #[test]
    fn unparseable_or_unknown_mpn_decodes_to_nothing() {
        check_mpn("R1", vec![("value", s("13k3"))], "NOT-A-REAL-MPN-1234").unwrap();
    }

    // BOM-26: lines whose designators must all be built from one MPN for the whole quantity of a board.

    use crate::endpoints::part::{PartSupplyId, PriceBreak};
    use crate::endpoints::warehouse::BoxId;
    use crate::warehouse::models::{BoxContents, BoxItem, BoxItemKind, WarehouseBox};

    fn wh_box(id: i32, serial: &str, part_id: PartId, count: i32) -> WarehouseBox {
        WarehouseBox {
            id: BoxId(id),
            serial: serial.into(),
            parent_chain: vec![],
            is_removed: false,
            last_counted: Utc::now(),
            items: BoxContents::new(vec![BoxItem {
                count,
                kind: BoxItemKind::Part { part_id },
                params: vec![],
            }]),
        }
    }

    fn supply_row(id: i32, supplier: i32, stock: i64, price: f32) -> PartSupply {
        PartSupply {
            id: PartSupplyId(id),
            part_id: PartId(0),
            supplier_id: CompanyId(supplier),
            spn: format!("SPN{id}"),
            stock,
            updated: Utc::now(),
            packaging: "Reel".into(),
            price_breaks: vec![PriceBreak {
                break_quantity: 1,
                break_price: Money::from_f32(price),
            }],
            url: None,
            source: None,
            authorized: true,
            expires: None,
            moq: None,
            order_multiple: None,
            lead_days: None,
        }
    }

    /// A part with its own warehouse boxes and supply rows, nothing else set.
    fn match_part(id: i32, pn: &str, boxes: Vec<WarehouseBox>, supply: Vec<PartSupply>) -> Part {
        Part {
            id: PartId(id),
            mfg: CompanyId(1),
            part_number: pn.into(),
            supply,
            folder_id: FolderId::root(),
            created: Utc::now(),
            updated: Utc::now(),
            is_incomplete: false,
            parameters: vec![],
            warehouse_stock: boxes,
            is_watched: false,
            files: vec![],
            crawl: Default::default(),
        }
    }

    /// A matched BOM-26 line with the given real choices and designator count.
    fn match_item(designators: usize, choices: &[i32], group: &str) -> BomItem {
        BomItem {
            id: BomItemId(1),
            bom_id: BomId(1),
            designators: (0..designators).map(|i| format!("Q{i}")).collect(),
            group_id: BomGroupId(1),
            part_choices: choices.iter().map(|id| PartId(*id)).collect(),
            candidates: vec![],
            parameters: vec![],
            match_group: Some(group.to_string()),
            part_numbers: None,
            hand_choices: HashSet::new(),
        }
    }

    #[test]
    fn single_box_for_needs_one_box_alone() {
        let item = match_item(5, &[1, 2], "GPIO-FET");
        // 3 in one box of part 1, 2 more in a second box of part 1: no single box has the 5 required
        let parts = HashMap::from([(
            PartId(1),
            match_part(
                1,
                "A",
                vec![wh_box(1, "B1", PartId(1), 3), wh_box(2, "B2", PartId(1), 2)],
                vec![],
            ),
        )]);
        assert_eq!(item.single_box_for(5, &parts), None);

        // one box alone has enough
        let parts = HashMap::from([(
            PartId(1),
            match_part(1, "A", vec![wh_box(1, "B1", PartId(1), 5)], vec![]),
        )]);
        assert_eq!(item.single_box_for(5, &parts).as_deref(), Some("B1"));
    }

    #[test]
    fn match_group_pick_prefers_cheapest_choice_that_alone_suffices() {
        let item = match_item(2, &[1, 2], "GPIO-FET");
        let parts = HashMap::from([
            (
                PartId(1),
                match_part(1, "A", vec![], vec![supply_row(1, 10, 1000, 0.50)]),
            ),
            (
                PartId(2),
                match_part(2, "B", vec![], vec![supply_row(2, 11, 1000, 0.20)]),
            ),
        ]);
        let pick = match_group_pick(&item, 2, &parts, SupplyFilter::default()).unwrap();
        let MatchPick::Buy { to_buy, offer } = pick else {
            panic!("expected a buy")
        };
        assert_eq!(to_buy, 2);
        assert_eq!(offer.part.id, PartId(2));
    }

    #[test]
    fn match_group_pick_in_stock_needs_no_purchase() {
        let item = match_item(2, &[1], "GPIO-FET");
        let parts = HashMap::from([(
            PartId(1),
            match_part(1, "A", vec![wh_box(1, "B1", PartId(1), 2)], vec![]),
        )]);
        let pick = match_group_pick(&item, 2, &parts, SupplyFilter::default());
        assert!(matches!(pick, Some(MatchPick::InStock)));
    }

    #[test]
    fn match_group_pick_none_when_no_single_choice_suffices() {
        // part 1 has 1 of 2 needed in stock and no supplier; part 2 has none and no supplier either
        let item = match_item(2, &[1, 2], "GPIO-FET");
        let parts = HashMap::from([
            (
                PartId(1),
                match_part(1, "A", vec![wh_box(1, "B1", PartId(1), 1)], vec![]),
            ),
            (PartId(2), match_part(2, "B", vec![], vec![])),
        ]);
        assert!(match_group_pick(&item, 2, &parts, SupplyFilter::default()).is_none());
    }

    fn match_bom(items: Vec<BomItem>, parts: HashMap<PartId, Part>) -> Bom {
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
            items,
            parameters: vec![],
            referenced_parts: parts,
            designators_kinds: vec![],
            suppliers: HashMap::new(),
        }
    }

    #[test]
    fn purchase_list_buys_one_mpn_for_a_matched_line() {
        let item = match_item(5, &[1, 2], "GPIO-FET");
        let parts = HashMap::from([
            (
                PartId(1),
                match_part(1, "A", vec![], vec![supply_row(1, 10, 1000, 0.50)]),
            ),
            (
                PartId(2),
                match_part(2, "B", vec![], vec![supply_row(2, 11, 1000, 0.20)]),
            ),
        ]);
        let bom = match_bom(vec![item], parts);
        let rows = bom.purchase_list(1, SupplyFilter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].quantity, 5);
        assert_eq!(rows[0].spn, "SPN2");
        assert!(rows[0].note.is_empty());
    }

    #[test]
    fn purchase_list_refuses_to_split_a_matched_line() {
        // neither choice alone has stock or a supplier price: no split purchase, one row with a note instead
        let item = match_item(5, &[1, 2], "GPIO-FET");
        let parts = HashMap::from([
            (PartId(1), match_part(1, "A", vec![], vec![])),
            (PartId(2), match_part(2, "B", vec![], vec![])),
        ]);
        let bom = match_bom(vec![item], parts);
        let rows = bom.purchase_list(1, SupplyFilter::default());
        assert_eq!(rows.len(), 1);
        assert!(rows[0].supplier.is_empty());
        assert!(rows[0].note.contains("one MPN per board"));
    }

    #[test]
    fn purchase_list_skips_a_matched_line_already_in_stock_of_one_mpn() {
        let item = match_item(2, &[1], "GPIO-FET");
        let parts = HashMap::from([(
            PartId(1),
            match_part(1, "A", vec![wh_box(1, "B1", PartId(1), 2)], vec![]),
        )]);
        let bom = match_bom(vec![item], parts);
        assert!(bom.purchase_list(1, SupplyFilter::default()).is_empty());
    }

    #[test]
    fn search_list_picks_one_box_for_a_matched_line() {
        let item = match_item(5, &[1, 2], "GPIO-FET");
        let parts = HashMap::from([
            (
                PartId(1),
                match_part(1, "A", vec![wh_box(1, "B1", PartId(1), 3)], vec![]),
            ),
            (
                PartId(2),
                match_part(2, "B", vec![wh_box(2, "B2", PartId(2), 5)], vec![]),
            ),
        ]);
        let bom = match_bom(vec![item], parts);
        let rows = bom.search_list(1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].boxes_serials, "B2");
        assert_eq!(rows[0].count, 5);
    }

    #[test]
    fn search_list_refuses_to_split_a_matched_line_pick() {
        // 3 + 2 = 5 required, but spread over two boxes of two different MPNs: no single box has it
        let item = match_item(5, &[1, 2], "GPIO-FET");
        let parts = HashMap::from([
            (
                PartId(1),
                match_part(1, "A", vec![wh_box(1, "B1", PartId(1), 3)], vec![]),
            ),
            (
                PartId(2),
                match_part(2, "B", vec![wh_box(2, "B2", PartId(2), 2)], vec![]),
            ),
        ]);
        let bom = match_bom(vec![item], parts);
        let rows = bom.search_list(1);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].boxes_serials.contains("one MPN per board"));
    }
}
