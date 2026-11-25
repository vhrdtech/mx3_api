//! BOM import from a plain table of strings.
//!
//! BOM import (BOM-1) on a plain table of strings: column detection by header name, skipping DNM rows, test
//! points and empty columns, part number lookup and building the `CreateBom` request. The UI's importer works on
//! an `egui_tabular` table and uses the same rules through `ColumnRole`, `is_dnm`, `is_test_point` and
//! `split_designators`; `mx3 bom import` reads CSV/XLSX into a `BomTable` and runs all of it.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::Serialize;

use crate::bom::pick::Pick;
use crate::endpoints::bom::{CreateBom, CreateBomItem};
use crate::endpoints::companies::CompanyId;
use crate::endpoints::part::{
    CreatePart, LookupPartResult, LookupParts, LookupPartsResult, MpnString, PartId,
};
use crate::endpoints::pcb::PcbId;
use crate::endpoints::user::UserId;

pub const DESIGNATORS_DELIMITERS: [char; 4] = [',', ' ', ';', '\t'];

/// Group of lines without a variant column or with an empty one
pub const DEFAULT_GROUP: &str = "main";

/// What a BOM column holds, from its header
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
pub enum ColumnRole {
    Designators,
    Value,
    Quantity,
    VariantGroup,
    PartNumber,
    /// "DNM"/"DNP" flag column
    Dnm,
    /// Maker of the part number column with the same number (`Mfr1` for `Mpn1`), or of the only one (BOM-25)
    Manufacturer,
    /// BOM-26: lines whose text matches must use one MPN for the whole build, never mixed (a schematic field
    /// named "Match", e.g. "GPIO-FET: one MPN per board")
    Match,
}

impl ColumnRole {
    pub fn from_header(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().as_str() {
            "designator" | "designators" | "reference" | "reference(s)" | "part reference"
            | "refdes" => Some(Self::Designators),
            "value" | "comment" | "name" => Some(Self::Value),
            "quantity" | "amount" | "count" | "qty" => Some(Self::Quantity),
            "variant" | "group" => Some(Self::VariantGroup),
            "dnm" | "dnp" => Some(Self::Dnm),
            "match" | "match group" => Some(Self::Match),
            mpn if mpn.starts_with("mpn")
                || mpn.starts_with("manufacturer part")
                || mpn.starts_with("mfr part")
                || mpn.starts_with("mfr. part")
                || mpn.starts_with("mfr pn") =>
            {
                Some(Self::PartNumber)
            }
            mfr if mfr.starts_with("mfr")
                || mfr.starts_with("mfg")
                || mfr.starts_with("manufacturer") =>
            {
                Some(Self::Manufacturer)
            }
            _ => None,
        }
    }
}

/// Decodes the bytes of a text BOM file.
///
/// Decodes the bytes of a text BOM file: UTF-8 (BOM stripped) first, UTF-16 with a BOM, else Windows-1252, which is what
/// Excel and Altium write for "ANSI" CSVs. Never lossy: `µ` and `±` come out as `µ` and `±`, not as `\u{FFFD}` or `Âµ`.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, u16::from_be_bytes);
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| cp1252(b)).collect(),
    }
}

fn utf16(bytes: &[u8], word: fn([u8; 2]) -> u16) -> String {
    let words: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| word(*c)).collect();
    String::from_utf16_lossy(&words)
}

fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// "DNM" or "DNP" (do not mount / populate), in the value or the flag column
pub fn is_dnm(cell: &str) -> bool {
    let cell = cell.trim().to_lowercase();
    cell == "dnm" || cell == "dnp"
}

/// Any designator of the line is a test point
pub fn is_test_point(designators: &str) -> bool {
    designators
        .split(DESIGNATORS_DELIMITERS)
        .any(|d| d.starts_with("TP"))
}

pub fn split_designators(designators: &str) -> Vec<String> {
    designators
        .split(DESIGNATORS_DELIMITERS)
        .filter(|d| !d.is_empty())
        .map(|d| d.to_owned())
        .collect()
}

#[derive(Copy, Clone, Debug, Serialize)]
pub struct ImportOptions {
    pub skip_dnm: bool,
    pub skip_test_points: bool,
    pub skip_empty_columns: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            skip_dnm: true,
            skip_test_points: true,
            skip_empty_columns: true,
        }
    }
}

/// A BOM as read from a file: header row and data rows, all cells as text
#[derive(Clone, Debug, Default)]
pub struct BomTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// The table after column detection and skipping
#[derive(Clone, Debug, Serialize)]
pub struct PreparedBom {
    pub headers: Vec<String>,
    pub designators_col: Option<usize>,
    pub value_col: Option<usize>,
    pub quantity_col: Option<usize>,
    pub group_col: Option<usize>,
    pub dnm_col: Option<usize>,
    /// BOM-26: the "Match" column, if any
    pub match_col: Option<usize>,
    pub part_number_cols: Vec<usize>,
    /// Maker columns; also saved as line parameters
    pub manufacturer_cols: Vec<usize>,
    /// Columns saved as line parameters (not detected, not empty)
    pub parameter_cols: Vec<usize>,
    pub skipped_empty_cols: Vec<usize>,
    /// Data rows kept, by index into `BomTable::rows`
    pub rows: Vec<usize>,
    pub skipped_dnm: Vec<usize>,
    pub skipped_test_points: Vec<usize>,
    /// Missing designator or value column
    pub errors: Vec<String>,
}

impl BomTable {
    fn cell(&self, row: usize, col: Option<usize>) -> &str {
        col.and_then(|c| self.rows.get(row).and_then(|r| r.get(c)))
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn prepare(&self, opts: ImportOptions) -> PreparedBom {
        let mut p = PreparedBom {
            headers: self.headers.clone(),
            designators_col: None,
            value_col: None,
            quantity_col: None,
            group_col: None,
            dnm_col: None,
            match_col: None,
            part_number_cols: vec![],
            manufacturer_cols: vec![],
            parameter_cols: vec![],
            skipped_empty_cols: vec![],
            rows: vec![],
            skipped_dnm: vec![],
            skipped_test_points: vec![],
            errors: vec![],
        };
        for (col, name) in self.headers.iter().enumerate() {
            match ColumnRole::from_header(name) {
                Some(ColumnRole::Designators) => p.designators_col = Some(col),
                Some(ColumnRole::Value) => p.value_col = Some(col),
                Some(ColumnRole::Quantity) => p.quantity_col = Some(col),
                Some(ColumnRole::VariantGroup) => p.group_col = Some(col),
                Some(ColumnRole::Dnm) => {
                    if p.dnm_col.is_none() {
                        p.dnm_col = Some(col);
                    }
                }
                Some(ColumnRole::PartNumber) => p.part_number_cols.push(col),
                Some(ColumnRole::Manufacturer) => p.manufacturer_cols.push(col),
                Some(ColumnRole::Match) => p.match_col = Some(col),
                None => {}
            }
        }
        if p.designators_col.is_none() {
            p.errors.push("Designators column not found".into());
        }
        if p.value_col.is_none() {
            p.errors.push("Value column not found".into());
        }

        for row in 0..self.rows.len() {
            if self.rows[row].iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            if opts.skip_dnm
                && p.value_col.is_some()
                && (is_dnm(self.cell(row, p.value_col)) || is_dnm(self.cell(row, p.dnm_col)))
            {
                p.skipped_dnm.push(row);
                continue;
            }
            if opts.skip_test_points
                && p.designators_col.is_some()
                && is_test_point(self.cell(row, p.designators_col))
            {
                p.skipped_test_points.push(row);
                continue;
            }
            p.rows.push(row);
        }

        let detected = |col: usize| {
            Some(col) == p.designators_col
                || Some(col) == p.value_col
                || Some(col) == p.quantity_col
                || Some(col) == p.group_col
                || Some(col) == p.match_col
                || p.part_number_cols.contains(&col)
        };
        for col in 0..self.headers.len() {
            let empty = p
                .rows
                .iter()
                .all(|r| self.cell(*r, Some(col)).trim().is_empty());
            if opts.skip_empty_columns && empty {
                p.skipped_empty_cols.push(col);
            } else if !detected(col) && !self.headers[col].trim().is_empty() {
                p.parameter_cols.push(col);
            }
        }
        p
    }

    /// Every distinct part number of the kept rows, to look up before creating the BOM
    pub fn lookup_request(&self, p: &PreparedBom) -> LookupParts {
        let mut pns = BTreeSet::new();
        for row in &p.rows {
            for col in &p.part_number_cols {
                let pn = self.cell(*row, Some(*col)).trim();
                if !pn.is_empty() {
                    // every maker of the part number is looked up; the maker column picks among them (BOM-25)
                    pns.insert(pn.to_owned());
                }
            }
        }
        LookupParts {
            mpns: pns
                .into_iter()
                .map(|pn| MpnString {
                    manufacturer_name: String::new(),
                    part_number: pn,
                })
                .collect(),
        }
    }

    /// The BOM to create. Part numbers found by the lookup become choices; unknown ones are created as new parts
    /// (with the placeholder manufacturer, as the UI does). A part number several parts share gets one real
    /// choice, the one `choose` picks from them and the maker the file names for it, and the others become
    /// candidates (BOM-25); without a pick all of them are candidates.
    pub fn create_bom_items(
        &self,
        p: &PreparedBom,
        lookup: &LookupParts,
        result: Option<&LookupPartsResult>,
        choose: impl Fn(&[PartId], Option<&str>) -> Option<Pick>,
    ) -> ImportItems {
        let mut out = ImportItems::default();
        if p.designators_col.is_none() || p.value_col.is_none() {
            return out;
        }
        for row in &p.rows {
            let designators = split_designators(self.cell(*row, p.designators_col));
            let group = self.cell(*row, p.group_col).trim();
            let group = if group.is_empty() {
                DEFAULT_GROUP
            } else {
                group
            };

            let mut part_choices: Vec<PartId> = vec![];
            let mut candidates: Vec<PartId> = vec![];
            let mut part_choices_create: Vec<CreatePart> = vec![];
            let mut part_numbers: Vec<String> = vec![];
            for col in &p.part_number_cols {
                let pn = self.cell(*row, Some(*col)).trim();
                if pn.is_empty() {
                    continue;
                }
                part_numbers.push(pn.to_owned());
                let found = lookup
                    .find_idx(pn)
                    .and_then(|idx| result.and_then(|r| r.parts.get(idx)));
                match found {
                    Some(LookupPartResult::Found { id, .. }) => part_choices.push(*id),
                    Some(LookupPartResult::NotUnique(ids)) => {
                        let maker = self.cell(*row, p.manufacturer_col_for(*col)).trim();
                        let maker = (!maker.is_empty()).then_some(maker);
                        match choose(ids, maker) {
                            Some(pick) => {
                                part_choices.push(pick.part);
                                candidates.extend(&pick.candidates);
                                out.picks.push(ImportPick {
                                    item: out.items.len(),
                                    part_number: pn.to_owned(),
                                    pick,
                                });
                            }
                            None => candidates.extend(ids),
                        }
                    }
                    Some(LookupPartResult::NotFound) | None => {
                        part_choices_create.push(CreatePart {
                            mfg_id: CompanyId::hack(7),
                            part_number: pn.to_owned(),
                        });
                    }
                }
            }

            let mut parameters = HashMap::new();
            for col in &p.parameter_cols {
                let value = self.cell(*row, Some(*col));
                if !value.is_empty() {
                    parameters.insert(self.headers[*col].clone(), value.to_owned());
                }
            }
            parameters.insert("Value".to_owned(), self.cell(*row, p.value_col).to_owned());

            let match_group = self.cell(*row, p.match_col).trim();
            let match_group = (!match_group.is_empty()).then(|| match_group.to_owned());

            // a part another column of the line picked stays a real choice only
            let mut seen: BTreeSet<i32> = part_choices.iter().map(|p| p.0).collect();
            candidates.retain(|c| seen.insert(c.0));
            out.items.push(CreateBomItem {
                designators,
                group: group.to_owned(),
                part_choices,
                candidates,
                part_choices_create,
                parameters,
                match_group,
                part_numbers,
                manual_parameters: HashMap::new(),
                hand_choices: HashSet::new(),
            });
        }
        out
    }
}

impl PreparedBom {
    /// The maker column of a part number column: the one with the same trailing number (`Mfr2` for `Mpn2`),
    /// else the only maker column when there is only one part number column
    pub fn manufacturer_col_for(&self, pn_col: usize) -> Option<usize> {
        let number = |col: usize| {
            let h = self.headers[col].trim();
            let digits = h.len() - h.trim_end_matches(|c: char| c.is_ascii_digit()).len();
            h[h.len() - digits..].to_string()
        };
        let n = number(pn_col);
        if !n.is_empty()
            && let Some(col) = self.manufacturer_cols.iter().find(|c| number(**c) == n)
        {
            return Some(*col);
        }
        match (&self.part_number_cols[..], &self.manufacturer_cols[..]) {
            ([_], [mfr]) => Some(*mfr),
            _ => None,
        }
    }
}

/// `create_bom_items`' result: the lines and, for each part number several parts share, the part picked
#[derive(Debug, Default, Serialize)]
pub struct ImportItems {
    pub items: Vec<CreateBomItem>,
    pub picks: Vec<ImportPick>,
}

/// The real choice picked for one part number of a line (BOM-25)
#[derive(Debug, Serialize)]
pub struct ImportPick {
    /// Index into `ImportItems::items`
    pub item: usize,
    pub part_number: String,
    pub pick: Pick,
}

impl CreateBom {
    /// A BOM for `pcb` with the placeholder user and company the UI uses
    pub fn new(pcb_id: PcbId, description: String, items: Vec<CreateBomItem>) -> Self {
        Self {
            pcb_id,
            description,
            user_id: UserId::hack(1),
            company_id: CompanyId::hack(1),
            items,
            chain: Default::default(),
            has_match_column: false,
            dry_run: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_text_utf8_keeps_micro_and_plus_minus() {
        assert_eq!(decode_text("10µF ±5%".as_bytes()), "10µF ±5%");
        assert_eq!(decode_text("\u{FEFF}4.7µF".as_bytes()), "4.7µF");
    }

    #[test]
    fn decode_text_ansi_and_utf16() {
        assert_eq!(decode_text(b"10\xB5F \xB15% \x80"), "10µF ±5% €");
        let mut u16le = vec![0xFF, 0xFE];
        for w in "10µF".encode_utf16() {
            u16le.extend(w.to_le_bytes());
        }
        assert_eq!(decode_text(&u16le), "10µF");
    }

    fn table(headers: &[&str], rows: &[&[&str]]) -> BomTable {
        BomTable {
            headers: headers.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
        }
    }

    fn skipping_table() -> BomTable {
        table(
            &[
                "Reference",
                "Value",
                "Qty",
                "MPN",
                "Footprint",
                "Empty",
                "DNP",
            ],
            &[
                &["R1, R2", "10k", "2", "RC0402FR-0710KL", "R_0402", "", ""],
                &["C1", "DNM", "1", "", "C_0402", "", ""],
                &["TP1", "TestPoint", "1", "", "TP", "", ""],
                &[
                    "U1",
                    "STM32G474",
                    "1",
                    "STM32G474RET6",
                    "LQFP-64",
                    "",
                    "DNP",
                ],
                &["", "", "", "", "", "", ""],
                &["C2 C3", "100n", "2", "", "C_0402", "", ""],
            ],
        )
    }

    #[test]
    fn detects_columns_and_skips() {
        let t = skipping_table();
        let p = t.prepare(ImportOptions::default());
        assert_eq!(p.errors, Vec::<String>::new());
        assert_eq!(p.designators_col, Some(0));
        assert_eq!(p.value_col, Some(1));
        assert_eq!(p.quantity_col, Some(2));
        assert_eq!(p.part_number_cols, vec![3]);
        assert_eq!(p.rows, vec![0, 5]);
        assert_eq!(p.skipped_dnm, vec![1, 3]);
        assert_eq!(p.skipped_test_points, vec![2]);
        assert_eq!(p.skipped_empty_cols, vec![5, 6]);
        assert_eq!(p.parameter_cols, vec![4]);
    }

    #[test]
    fn skipping_table_becomes_bom_items() {
        let t = skipping_table();
        let p = t.prepare(ImportOptions::default());
        let lookup = t.lookup_request(&p);
        assert_eq!(lookup.mpns.len(), 1);
        let result = LookupPartsResult {
            parts: vec![LookupPartResult::Found {
                id: PartId(42),
                pn: "RC0402FR-0710KL".into(),
            }],
        };
        let items = t
            .create_bom_items(&p, &lookup, Some(&result), |_, _| None)
            .items;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].designators, vec!["R1", "R2"]);
        assert_eq!(items[0].part_choices, vec![PartId(42)]);
        assert_eq!(items[0].group, "main");
        assert_eq!(&items[0].parameters["Footprint"], "R_0402");
        assert_eq!(&items[0].parameters["Value"], "10k");
        assert_eq!(items[1].designators, vec!["C2", "C3"]);
    }

    #[test]
    fn unknown_part_numbers_are_created() {
        let t = table(
            &["Designator", "Comment", "MPN"],
            &[&["U2", "LDO", "XYZ123"]],
        );
        let p = t.prepare(ImportOptions::default());
        let lookup = t.lookup_request(&p);
        let result = LookupPartsResult {
            parts: vec![LookupPartResult::NotFound],
        };
        let items = t
            .create_bom_items(&p, &lookup, Some(&result), |_, _| None)
            .items;
        assert_eq!(items[0].part_choices, Vec::<PartId>::new());
        assert_eq!(items[0].part_choices_create[0].part_number, "XYZ123");
    }

    #[test]
    fn missing_columns_are_errors() {
        let t = table(&["Foo", "Bar"], &[&["1", "2"]]);
        let p = t.prepare(ImportOptions::default());
        assert_eq!(p.errors.len(), 2);
        assert!(
            t.create_bom_items(&p, &t.lookup_request(&p), None, |_, _| None)
                .items
                .is_empty()
        );
    }

    #[test]
    fn maker_columns_pair_with_part_number_columns() {
        let t = table(
            &[
                "Reference",
                "Value",
                "Mpn1",
                "Mpn2",
                "Mfr2",
                "Manufacturer1",
                "Manufacturer Part Number",
            ],
            &[&["D1", "BAT54", "A", "B", "NXP", "Nexperia", "C"]],
        );
        let p = t.prepare(ImportOptions::default());
        assert_eq!(p.part_number_cols, vec![2, 3, 6]);
        assert_eq!(p.manufacturer_cols, vec![4, 5]);
        assert_eq!(p.manufacturer_col_for(2), Some(5));
        assert_eq!(p.manufacturer_col_for(3), Some(4));
        assert_eq!(p.manufacturer_col_for(6), None);
        // still saved on the line
        assert_eq!(p.parameter_cols, vec![4, 5]);

        let one = table(
            &["Ref", "Value", "MPN", "Manufacturer"],
            &[&["D1", "x", "A", "NXP"]],
        );
        let p = one.prepare(ImportOptions::default());
        assert_eq!(p.manufacturer_col_for(2), Some(3));
    }

    /// One MPN, three makers (unknown, discontinued listing, active): one real choice, the rest candidates
    #[test]
    fn shared_part_number_gets_one_choice_and_candidates() {
        use crate::bom::pick::{pick, tests::three_makers};
        let facts = three_makers();
        let ids: Vec<PartId> = facts.iter().map(|f| f.part).collect();
        let result = LookupPartsResult {
            parts: vec![LookupPartResult::NotUnique(ids)],
        };
        let run = |t: &BomTable| {
            let p = t.prepare(ImportOptions::default());
            let lookup = t.lookup_request(&p);
            t.create_bom_items(&p, &lookup, Some(&result), |ids, maker| {
                let f: Vec<_> = facts
                    .iter()
                    .filter(|f| ids.contains(&f.part))
                    .cloned()
                    .collect();
                pick(&f, maker)
            })
        };

        let without = run(&table(
            &["Reference", "Value", "Mpn1"],
            &[&["D1", "BAT54", "BAT54CW,115"]],
        ));
        assert_eq!(without.items[0].part_choices, vec![PartId(3)]);
        assert_eq!(without.items[0].candidates, vec![PartId(1), PartId(2)]);
        assert_eq!(without.picks[0].part_number, "BAT54CW,115");
        assert_eq!(without.picks[0].pick.to_string(), "known maker, active");

        let with = run(&table(
            &["Reference", "Value", "Mpn1", "Mfr1"],
            &[&["D1", "BAT54", "BAT54CW,115", "NXP"]],
        ));
        assert_eq!(with.items[0].part_choices, vec![PartId(2)]);
        assert_eq!(with.items[0].candidates, vec![PartId(1), PartId(3)]);
        assert_eq!(with.picks[0].pick.to_string(), "maker NXP from the file");
        assert_eq!(&with.items[0].parameters["Mfr1"], "NXP");
    }

    /// BOM-26: a "Match" column sets each line's match-group text; empty cells and no column at all give `None`,
    /// and the column itself never becomes a generic parameter.
    #[test]
    fn match_column_sets_match_group() {
        let t = table(
            &["Reference", "Value", "Match"],
            &[
                &["Q703", "FDN340P", "GPIO-FET: one MPN per board"],
                &["Q706", "FDN340P", "GPIO-FET: one MPN per board"],
                &["R1", "10k", ""],
            ],
        );
        let p = t.prepare(ImportOptions::default());
        assert_eq!(p.match_col, Some(2));
        assert_eq!(p.parameter_cols, Vec::<usize>::new());
        let items = t
            .create_bom_items(&p, &t.lookup_request(&p), None, |_, _| None)
            .items;
        assert_eq!(
            items[0].match_group.as_deref(),
            Some("GPIO-FET: one MPN per board")
        );
        assert_eq!(items[1].match_group, items[0].match_group);
        assert_eq!(items[2].match_group, None);

        let without_column = table(&["Reference", "Value"], &[&["R1", "10k"]]);
        let p = without_column.prepare(ImportOptions::default());
        assert_eq!(p.match_col, None);
        let items = without_column
            .create_bom_items(&p, &without_column.lookup_request(&p), None, |_, _| None)
            .items;
        assert_eq!(items[0].match_group, None);
    }
}
