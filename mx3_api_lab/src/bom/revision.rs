//! BOM-28: a new revision of a chain against the previous one. Lines are paired, each pair says what changed, and
//! the decisions people made on the previous revision's lines carry over to the new lines:
//!
//! - Pairing: a previous line whose part numbers (normalised) and value match the new line wins over one that
//!   merely shares more designators; among the rest, designators first (the previous line sharing the most),
//!   then a line whose designators all changed by the same part numbers and value. A previous line nothing
//!   pairs with, and whose designators aren't now entirely inside one new line (a merge, below), is removed; a
//!   new line without a pair is added.
//! - Merge: when a schematic change moves several previous lines' designators into one new line, the previous
//!   line whose part numbers and value still match pairs as above; every other previous line fully absorbed
//!   into that new line (its designators all now inside the new line's) carries its own part choices the same
//!   way, by its own part numbers and value, rather than being reported removed. The new line's change says
//!   which other previous lines it came from.
//! - Part choices: for a part number both files name, the previous line's parts of that number and their state
//!   (real choice or candidate, BOM-24/25) replace what the importer picked, so a confirmed candidate stays
//!   confirmed and a dropped one stays dropped. A part someone picked by hand (`BomItem::hand_choices`, BOM-29:
//!   add-choice, the UI, supplier search --add/--create, `bom suggest --add`, not matched by this import) is
//!   kept while the line's value and footprint stay the same; one the importer matched that the new file no
//!   longer names is dropped, whatever the line's value and footprint do. `by_hand` is read straight off each
//!   previous choice, never re-guessed, so it survives any number of revisions even as a part's own MPN changes.
//!   The new line's own `hand_choices` is the carried-over hand picks, so the next revision still knows.
//! - Match group (BOM-26): the file's Match column wins when the file has one; else the previous line's text stays.
//! - Line parameters set by hand (any source but the import) are kept unless the file names the same parameter.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::endpoints::bom::{Bom, BomId, BomItem, CreateBomItem};
use crate::endpoints::part::PartId;

/// The source of a line's file columns (the backend stores them under it)
pub const BOM_IMPORT_SOURCE: &str = "bom_import";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Same,
    Changed,
    Added,
    Removed,
}

/// A part that carried over from the previous line or was dropped
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChoiceCarry {
    pub part: PartId,
    pub mpn: String,
    pub candidate: bool,
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineChange {
    pub kind: LineKind,
    /// The new line's designators (the previous line's for a removed one)
    pub designators: Vec<String>,
    /// The paired previous line's designators
    pub previous_designators: Vec<String>,
    /// What differs, one entry each: `designators +C5 -C7`, `value 100n -> 1u`, `part numbers +X -Y`
    pub changes: Vec<String>,
    pub carried: Vec<ChoiceCarry>,
    pub dropped: Vec<ChoiceCarry>,
    pub match_group_carried: Option<String>,
    pub parameters_carried: Vec<String>,
    /// Designators of other previous lines fully absorbed into this one (a merge, BOM-29): each also contributed
    /// its carried/dropped choices above, but didn't set `previous_designators` or the designators/value/footprint
    /// changes, which are against the paired line only
    pub merged_from: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionDiff {
    pub previous: BomId,
    pub lines: Vec<LineChange>,
}

impl RevisionDiff {
    pub fn count(&self, kind: LineKind) -> usize {
        self.lines.iter().filter(|l| l.kind == kind).count()
    }

    pub fn carried(&self) -> usize {
        self.lines.iter().map(|l| l.carried.len()).sum()
    }

    pub fn dropped(&self) -> usize {
        self.lines.iter().map(|l| l.dropped.len()).sum()
    }
}

fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

fn param<'a>(map: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    map.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

fn old_param(item: &BomItem, name: &str) -> Option<String> {
    item.parameters
        .iter()
        .filter(|p| !p.retracted && p.name.eq_ignore_ascii_case(name))
        .find_map(|p| p.raw.clone().or_else(|| p.value.as_non_empty_string().ok()))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

struct Old<'a> {
    item: &'a BomItem,
    designators: BTreeSet<String>,
    value: Option<String>,
    footprint: Option<String>,
    /// The file's part numbers, when the line knows them (imported since BOM-28)
    file_mpns: Option<BTreeSet<String>>,
    /// (part, its MPN, candidate)
    parts: Vec<(PartId, String, bool)>,
    /// Which of `parts` were picked by hand rather than matched by the import (BOM-29,
    /// `BomItem::hand_choices`/`bom_item_part_choice.by_hand`)
    hand: HashSet<PartId>,
}

impl Old<'_> {
    /// The part numbers to compare with a new file: the file's, else the real choices'
    fn mpns(&self) -> BTreeSet<String> {
        match &self.file_mpns {
            Some(m) => m.clone(),
            None => self
                .parts
                .iter()
                .filter(|(_, _, c)| !c)
                .map(|(_, m, _)| norm(m))
                .collect(),
        }
    }
}

fn new_designators(item: &CreateBomItem) -> BTreeSet<String> {
    item.designators
        .iter()
        .map(|d| d.trim().to_uppercase())
        .collect()
}

fn new_mpns(item: &CreateBomItem) -> BTreeSet<String> {
    item.part_numbers.iter().map(|m| norm(m)).collect()
}

fn set_diff(label: &str, old: &BTreeSet<String>, new: &BTreeSet<String>) -> Option<String> {
    let added: Vec<&str> = new.difference(old).map(|s| s.as_str()).collect();
    let removed: Vec<&str> = old.difference(new).map(|s| s.as_str()).collect();
    if added.is_empty() && removed.is_empty() {
        return None;
    }
    let mut parts = vec![];
    if !added.is_empty() {
        parts.push(format!("+{}", added.join(" +")));
    }
    if !removed.is_empty() {
        parts.push(format!("-{}", removed.join(" -")));
    }
    Some(format!("{label} {}", parts.join(" ")))
}

fn text_change(label: &str, old: &Option<String>, new: Option<&str>) -> Option<String> {
    let o = old.as_deref().unwrap_or("");
    let n = new.unwrap_or("");
    (norm(o) != norm(n)).then(|| format!("{label} {} -> {}", or_dash(o), or_dash(n)))
}

fn or_dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}

/// Pair the lines of `new` with those of `prev`, carry the previous decisions into `new` (in place) and say what
/// changed. `mpn_of` gives the part number of a part `new` names (the importer's picks); `has_match_column`:
/// the file has a Match column, so its (possibly empty) match groups win.
pub fn carry_over(
    prev: &Bom,
    new: &mut [CreateBomItem],
    has_match_column: bool,
    mpn_of: impl Fn(PartId) -> Option<String>,
) -> RevisionDiff {
    let olds: Vec<Old> = prev
        .items
        .iter()
        .map(|item| {
            let mpn = |p: &PartId| {
                prev.referenced_parts
                    .get(p)
                    .map(|p| p.part_number.clone())
                    .unwrap_or_default()
            };
            let mut parts: Vec<(PartId, String, bool)> = item
                .part_choices
                .iter()
                .map(|p| (*p, mpn(p), false))
                .collect();
            parts.extend(item.candidates.iter().map(|p| (*p, mpn(p), true)));
            Old {
                item,
                designators: item
                    .designators
                    .iter()
                    .map(|d| d.trim().to_uppercase())
                    .collect(),
                value: item.value(),
                footprint: old_param(item, "Footprint"),
                file_mpns: item
                    .part_numbers
                    .as_ref()
                    .map(|m| m.iter().map(|s| norm(s)).collect()),
                parts,
                hand: item.hand_choices.clone(),
            }
        })
        .collect();

    // pairing: a previous line whose part numbers and value match wins over one merely sharing more designators
    // (a merge moves most designators to the line that isn't the real continuation, BOM-29); among the rest,
    // designators first, then same part numbers and value among the previous lines left
    let mut pair: Vec<Option<usize>> = vec![None; new.len()];
    for (n, item) in new.iter().enumerate() {
        let d = new_designators(item);
        let mpns = new_mpns(item);
        let value = param(&item.parameters, "Value").map(norm);
        pair[n] = olds
            .iter()
            .enumerate()
            .map(|(o, old)| {
                let exact = old.mpns() == mpns && old.value.as_deref().map(norm) == value;
                (o, exact, old.designators.intersection(&d).count())
            })
            .filter(|(_, _, shared)| *shared > 0)
            .max_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)).then(b.0.cmp(&a.0)))
            .map(|(o, _, _)| o);
    }
    let mut used: HashSet<usize> = pair.iter().flatten().copied().collect();
    for (n, item) in new.iter().enumerate() {
        if pair[n].is_some() {
            continue;
        }
        let mpns = new_mpns(item);
        let value = param(&item.parameters, "Value").map(norm);
        if mpns.is_empty() {
            continue;
        }
        if let Some(o) = olds.iter().enumerate().position(|(o, old)| {
            !used.contains(&o) && old.mpns() == mpns && old.value.as_deref().map(norm) == value
        }) {
            pair[n] = Some(o);
            used.insert(o);
        }
    }

    // merge (BOM-29): a previous line nothing paired with, but whose designators now all sit inside one new
    // line's, was absorbed into it rather than removed. Ambiguous cases (none, or more than one, new line
    // containing it) are left alone, reported removed as before.
    let new_designators_of: Vec<BTreeSet<String>> = new.iter().map(new_designators).collect();
    let mut merges: Vec<Vec<usize>> = vec![vec![]; new.len()];
    for (o, old) in olds.iter().enumerate() {
        if used.contains(&o) || old.designators.is_empty() {
            continue;
        }
        let hosts: Vec<usize> = pair
            .iter()
            .enumerate()
            .filter(|(n, p)| p.is_some() && old.designators.is_subset(&new_designators_of[*n]))
            .map(|(n, _)| n)
            .collect();
        if let [host] = hosts[..] {
            merges[host].push(o);
            used.insert(o);
        }
    }

    let mut lines = vec![];
    for (n, item) in new.iter_mut().enumerate() {
        let Some(o) = pair[n] else {
            lines.push(LineChange {
                kind: LineKind::Added,
                designators: item.designators.clone(),
                previous_designators: vec![],
                changes: vec![],
                carried: vec![],
                dropped: vec![],
                match_group_carried: None,
                parameters_carried: vec![],
                merged_from: vec![],
            });
            continue;
        };
        let old = &olds[o];
        let contributors: Vec<&Old> = std::iter::once(old)
            .chain(merges[n].iter().map(|&o2| &olds[o2]))
            .collect();
        let mpns = new_mpns(item);
        let value = param(&item.parameters, "Value").map(str::to_string);
        let footprint = param(&item.parameters, "Footprint").map(str::to_string);
        let mut changes = vec![];
        changes.extend(set_diff(
            "designators",
            &old.designators,
            &new_designators(item),
        ));
        changes.extend(text_change("value", &old.value, value.as_deref()));
        changes.extend(text_change(
            "footprint",
            &old.footprint,
            footprint.as_deref(),
        ));
        changes.extend(set_diff("part numbers", &old.mpns(), &mpns));

        // the previous lines' parts (the paired line, plus any merged into it): carried with their state, or
        // dropped; "by hand" (BOM-29) and "same value/footprint" are judged against each part's own previous
        // line, not necessarily the paired one
        let mut carried = vec![];
        let mut dropped = vec![];
        let mut hand_choices = vec![];
        for src in &contributors {
            let same_value = norm(src.value.as_deref().unwrap_or(""))
                == norm(value.as_deref().unwrap_or(""))
                && norm(src.footprint.as_deref().unwrap_or(""))
                    == norm(footprint.as_deref().unwrap_or(""));
            for (part, mpn, candidate) in &src.parts {
                let by_hand = src.hand.contains(part);
                let carry = |why: &str| ChoiceCarry {
                    part: *part,
                    mpn: mpn.clone(),
                    candidate: *candidate,
                    why: why.into(),
                };
                if mpns.contains(&norm(mpn)) {
                    carried.push(carry("same part number"));
                    if by_hand {
                        hand_choices.push(*part);
                    }
                    continue;
                }
                if by_hand && same_value {
                    carried.push(carry("added by hand, value and footprint unchanged"));
                    hand_choices.push(*part);
                } else if by_hand {
                    dropped.push(carry("added by hand, value or footprint changed"));
                } else {
                    dropped.push(carry("part number no longer in the file"));
                }
            }
        }
        // the previous decisions replace the importer's picks for the part numbers any contributing line had
        let decided: HashSet<String> = contributors
            .iter()
            .flat_map(|o| o.parts.iter().map(|(_, m, _)| norm(m)))
            .collect();
        let decided_by_old = |p: &PartId| mpn_of(*p).is_some_and(|m| decided.contains(&norm(&m)));
        item.part_choices.retain(|p| !decided_by_old(p));
        item.candidates.retain(|p| !decided_by_old(p));
        item.part_choices_create
            .retain(|c| !decided.contains(&norm(&c.part_number)));
        for c in &carried {
            let list = if c.candidate {
                &mut item.candidates
            } else {
                &mut item.part_choices
            };
            if !list.contains(&c.part) {
                list.push(c.part);
            }
        }
        item.candidates.retain(|c| !item.part_choices.contains(c));
        item.hand_choices = hand_choices.into_iter().collect();

        let mut match_group_carried = None;
        if !has_match_column
            && item.match_group.is_none()
            && let Some(g) = &old.item.match_group
        {
            item.match_group = Some(g.clone());
            match_group_carried = Some(g.clone());
        }
        if has_match_column && item.match_group != old.item.match_group {
            changes.push(format!(
                "match group {} -> {}",
                or_dash(old.item.match_group.as_deref().unwrap_or("")),
                or_dash(item.match_group.as_deref().unwrap_or(""))
            ));
        }

        let mut parameters_carried = vec![];
        for p in &old.item.parameters {
            if p.retracted
                || p.source == BOM_IMPORT_SOURCE
                || param(&item.parameters, &p.name).is_some()
                || item
                    .manual_parameters
                    .keys()
                    .any(|k| k.eq_ignore_ascii_case(&p.name))
            {
                continue;
            }
            let text = p
                .raw
                .clone()
                .or_else(|| p.value.as_non_empty_string().ok())
                .unwrap_or_default();
            if text.trim().is_empty() {
                continue;
            }
            item.manual_parameters.insert(p.name.clone(), text);
            parameters_carried.push(p.name.clone());
        }

        let merged_from: Vec<Vec<String>> = merges[n]
            .iter()
            .map(|&o2| olds[o2].item.designators.clone())
            .collect();
        if !merged_from.is_empty() {
            let from: Vec<String> = merged_from.iter().map(|d| d.join(",")).collect();
            changes.push(format!("merged from {}", from.join("; ")));
        }

        let kind = if changes.is_empty() && dropped.is_empty() {
            LineKind::Same
        } else {
            LineKind::Changed
        };
        lines.push(LineChange {
            kind,
            designators: item.designators.clone(),
            previous_designators: old.item.designators.clone(),
            changes,
            carried,
            dropped,
            match_group_carried,
            parameters_carried,
            merged_from,
        });
    }
    for (o, old) in olds.iter().enumerate() {
        if !used.contains(&o) {
            lines.push(LineChange {
                kind: LineKind::Removed,
                designators: old.item.designators.clone(),
                previous_designators: old.item.designators.clone(),
                changes: vec![],
                carried: vec![],
                dropped: old
                    .parts
                    .iter()
                    .map(|(part, mpn, candidate)| ChoiceCarry {
                        part: *part,
                        mpn: mpn.clone(),
                        candidate: *candidate,
                        why: "line removed".into(),
                    })
                    .collect(),
                match_group_carried: None,
                parameters_carried: vec![],
                merged_from: vec![],
            });
        }
    }
    RevisionDiff {
        previous: prev.id,
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoints::bom::{BomGroupId, BomHeader, BomItemId};
    use crate::endpoints::bom_chain::BomChainId;
    use crate::endpoints::companies::CompanyId;
    use crate::endpoints::parameter::ParameterDefId;
    use crate::endpoints::params::Resolved;
    use crate::endpoints::part::Part;
    use crate::endpoints::user::UserId;
    use chrono::Utc;
    use rvariant::Variant;
    use std::collections::BTreeMap;

    fn part(id: i32, mpn: &str) -> Part {
        Part {
            id: PartId(id),
            mfg: CompanyId(7),
            part_number: mpn.into(),
            supply: vec![],
            folder_id: crate::endpoints::folder::FolderId::root(),
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

    fn param(name: &str, value: &str, source: &str) -> Resolved {
        let mut r = Resolved::new(ParameterDefId(1), name, Variant::Str(value.into()));
        r.raw = Some(value.into());
        r.source = source.into();
        r
    }

    struct OldLine<'a> {
        designators: &'a [&'a str],
        value: &'a str,
        choices: &'a [i32],
        candidates: &'a [i32],
        file: Option<&'a [&'a str]>,
        match_group: Option<&'a str>,
        manual: &'a [(&'a str, &'a str)],
        /// Ids of `choices`/`candidates` picked by hand rather than matched by the import (BOM-29)
        hand: &'a [i32],
    }

    fn prev(lines: &[OldLine], parts: &[(i32, &str)]) -> Bom {
        let items = lines
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let mut parameters = vec![
                    param("Value", l.value, BOM_IMPORT_SOURCE),
                    param("Footprint", "C_0402", BOM_IMPORT_SOURCE),
                ];
                parameters.extend(l.manual.iter().map(|(n, v)| param(n, v, "manual")));
                BomItem {
                    id: BomItemId(i as i32 + 100),
                    bom_id: BomId(30),
                    designators: l.designators.iter().map(|s| s.to_string()).collect(),
                    group_id: BomGroupId(1),
                    part_choices: l.choices.iter().map(|p| PartId(*p)).collect(),
                    candidates: l.candidates.iter().map(|p| PartId(*p)).collect(),
                    parameters,
                    match_group: l.match_group.map(str::to_string),
                    part_numbers: l.file.map(|f| f.iter().map(|s| s.to_string()).collect()),
                    hand_choices: l.hand.iter().map(|p| PartId(*p)).collect(),
                }
            })
            .collect();
        Bom {
            id: BomId(30),
            header: BomHeader {
                id: BomId(30),
                pcb_id: None,
                description: String::new(),
                created: Utc::now(),
                updated: Utc::now(),
                user_id: UserId::hack(1),
                company_id: CompanyId(7),
                frozen: false,
                chain_id: BomChainId(1),
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
            referenced_parts: parts
                .iter()
                .map(|(id, m)| (PartId(*id), part(*id, m)))
                .collect(),
            designators_kinds: vec![],
            suppliers: HashMap::new(),
        }
    }

    fn line(
        designators: &[&str],
        value: &str,
        file: &[&str],
        choices: &[i32],
        candidates: &[i32],
    ) -> CreateBomItem {
        CreateBomItem {
            designators: designators.iter().map(|s| s.to_string()).collect(),
            group: "main".into(),
            part_choices: choices.iter().map(|p| PartId(*p)).collect(),
            candidates: candidates.iter().map(|p| PartId(*p)).collect(),
            part_choices_create: vec![],
            parameters: [("Value", value), ("Footprint", "C_0402")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            match_group: None,
            part_numbers: file.iter().map(|s| s.to_string()).collect(),
            manual_parameters: HashMap::new(),
            hand_choices: HashSet::new(),
        }
    }

    const PARTS: &[(i32, &str)] = &[
        (1, "GRM155R71C104KA88D"),
        (2, "CL05B104KO5NNNC"),
        (3, "CL05B104KO5NNNC"),
        (4, "RC0402FR-0710KL"),
        (5, "SS34"),
        (6, "SS34"),
        (7, "LMR51430"),
        (8, "2N7002"),
        (9, "2N7002K"),
        (10, "BSS138W-7-F"),
        (11, "BSS138BKW,115"),
    ];

    fn mpn_of(p: PartId) -> Option<String> {
        PARTS
            .iter()
            .find(|(id, _)| *id == p.0)
            .map(|(_, m)| m.to_string())
    }

    #[test]
    fn same_line_keeps_the_confirmed_and_dropped_choices() {
        // C1: file named both MPNs; part 3 (another maker of CL05...) was a candidate someone confirmed, part 2
        // was dropped; the importer now picks 2 again and makes 3 a candidate
        let p = prev(
            &[OldLine {
                designators: &["C1", "C2"],
                value: "100n",
                choices: &[1, 3],
                candidates: &[],
                file: Some(&["GRM155R71C104KA88D", "CL05B104KO5NNNC"]),
                match_group: Some("decoupling"),
                manual: &[("Note", "keep X7R")],
                hand: &[],
            }],
            PARTS,
        );
        let mut new = vec![line(
            &["C2", "C1"],
            "100n",
            &["GRM155R71C104KA88D", "CL05B104KO5NNNC"],
            &[1, 2],
            &[3],
        )];
        let diff = carry_over(&p, &mut new, false, mpn_of);
        assert_eq!(diff.lines.len(), 1);
        assert_eq!(diff.lines[0].kind, LineKind::Same, "{:?}", diff.lines[0]);
        let mut choices = new[0].part_choices.clone();
        choices.sort_by_key(|p| p.0);
        assert_eq!(choices, vec![PartId(1), PartId(3)]);
        assert!(new[0].candidates.is_empty());
        assert_eq!(new[0].match_group.as_deref(), Some("decoupling"));
        assert_eq!(
            new[0].manual_parameters.get("Note").map(String::as_str),
            Some("keep X7R")
        );
        assert_eq!(diff.carried(), 2);
    }

    #[test]
    fn changed_lines_say_what_and_drop_what_no_longer_fits() {
        let p = prev(
            &[
                // C1..C3 lose C3, gain C4: designators changed, MPN kept
                OldLine {
                    designators: &["C1", "C2", "C3"],
                    value: "100n",
                    choices: &[1],
                    candidates: &[],
                    file: Some(&["GRM155R71C104KA88D"]),
                    match_group: None,
                    manual: &[],
                    hand: &[],
                },
                // R1: part 7 was added by hand (its MPN was never in the file), and the value changes: dropped
                OldLine {
                    designators: &["R1"],
                    value: "10k",
                    choices: &[4, 7],
                    candidates: &[],
                    file: Some(&["RC0402FR-0710KL"]),
                    match_group: None,
                    manual: &[],
                    hand: &[7],
                },
                // D1 removed
                OldLine {
                    designators: &["D1"],
                    value: "SS34",
                    choices: &[5],
                    candidates: &[6],
                    file: None,
                    match_group: None,
                    manual: &[],
                    hand: &[],
                },
            ],
            PARTS,
        );
        let mut new = vec![
            line(
                &["C1", "C2", "C4"],
                "100n",
                &["GRM155R71C104KA88D"],
                &[1],
                &[],
            ),
            line(&["R1"], "13k3", &["RC0402FR-0713K3L"], &[], &[]),
            line(&["U1"], "LMR51430", &["LMR51430"], &[7], &[]),
        ];
        let diff = carry_over(&p, &mut new, false, mpn_of);
        let c = &diff.lines[0];
        assert_eq!(c.kind, LineKind::Changed);
        assert_eq!(c.changes, vec!["designators +C4 -C3".to_string()]);
        let r = &diff.lines[1];
        assert_eq!(r.kind, LineKind::Changed);
        assert!(
            r.changes.contains(&"value 10k -> 13k3".to_string()),
            "{:?}",
            r.changes
        );
        assert_eq!(r.dropped.len(), 2);
        assert!(new[1].part_choices.is_empty());
        assert_eq!(diff.lines[2].kind, LineKind::Added);
        assert_eq!(diff.lines[3].kind, LineKind::Removed);
        assert_eq!(diff.lines[3].designators, vec!["D1".to_string()]);
        assert_eq!(diff.count(LineKind::Removed), 1);
    }

    #[test]
    fn hand_choice_stays_while_value_and_footprint_stay() {
        // part 5 was added by hand
        let p = prev(
            &[OldLine {
                designators: &["U1"],
                value: "LMR51430",
                choices: &[7, 5],
                candidates: &[],
                file: None,
                match_group: None,
                manual: &[],
                hand: &[5],
            }],
            PARTS,
        );
        let mut new = vec![line(&["U1"], "LMR51430", &["LMR51430"], &[7], &[])];
        let diff = carry_over(&p, &mut new, false, mpn_of);
        let mut choices = new[0].part_choices.clone();
        choices.sort_by_key(|p| p.0);
        assert_eq!(choices, vec![PartId(5), PartId(7)]);
        assert_eq!(diff.lines[0].carried.len(), 2);
    }

    #[test]
    fn renamed_designators_pair_by_part_numbers_and_match_column_wins() {
        let p = prev(
            &[OldLine {
                designators: &["Q703", "Q706"],
                value: "BSS138",
                choices: &[5],
                candidates: &[6],
                file: Some(&["SS34"]),
                match_group: Some("gpio"),
                manual: &[],
                hand: &[],
            }],
            PARTS,
        );
        let mut new = vec![line(&["Q1", "Q2"], "BSS138", &["SS34"], &[5, 6], &[])];
        let diff = carry_over(&p, &mut new, true, mpn_of);
        assert_eq!(diff.lines.len(), 1, "{:?}", diff.lines);
        assert_eq!(diff.lines[0].previous_designators, vec!["Q703", "Q706"]);
        assert_eq!(new[0].part_choices, vec![PartId(5)]);
        assert_eq!(new[0].candidates, vec![PartId(6)]);
        // the file has a Match column and leaves it empty: the group goes
        assert_eq!(new[0].match_group, None);
        assert!(
            diff.lines[0]
                .changes
                .iter()
                .any(|c| c.starts_with("match group gpio"))
        );
    }

    #[test]
    fn merge_pairs_by_part_numbers_not_designator_overlap() {
        // B153 rev B r7 -> r8: Q201,Q203,Q701,Q1003 (2N7002 family) and Q501,Q601 (BSS138W) move into one new
        // line, BSS138W value. Designator overlap alone would pair the new line with the bigger 2N7002 line;
        // part numbers and value must win instead, so the BSS138W line's choices carry and the 2N7002 ones
        // drop, and the 2N7002 line is reported merged in, not removed (BOM-29).
        let p = prev(
            &[
                OldLine {
                    designators: &["Q201", "Q203", "Q701", "Q1003"],
                    value: "2N7002",
                    choices: &[8],
                    candidates: &[],
                    file: Some(&["2N7002", "2N7002K", "2SK3018"]),
                    match_group: None,
                    manual: &[],
                    hand: &[],
                },
                OldLine {
                    designators: &["Q501", "Q601"],
                    value: "BSS138W",
                    choices: &[10],
                    candidates: &[11],
                    file: Some(&["BSS138W-7-F", "BSS138BKW,115"]),
                    match_group: None,
                    manual: &[],
                    hand: &[],
                },
            ],
            PARTS,
        );
        let mut new = vec![line(
            &["Q201", "Q203", "Q501", "Q601", "Q701", "Q1003"],
            "BSS138W",
            &["BSS138W-7-F", "BSS138BKW,115"],
            &[10],
            &[11],
        )];
        let diff = carry_over(&p, &mut new, false, mpn_of);
        assert_eq!(diff.lines.len(), 1, "{:?}", diff.lines);
        let l = &diff.lines[0];
        assert_eq!(l.kind, LineKind::Changed, "{:?}", l);
        assert_eq!(l.previous_designators, vec!["Q501", "Q601"]);
        assert_eq!(
            l.merged_from,
            vec![vec!["Q201", "Q203", "Q701", "Q1003"]],
            "{l:?}"
        );
        assert_eq!(
            diff.count(LineKind::Removed),
            0,
            "the 2N7002 line merged in, it wasn't removed"
        );
        let mut choices = new[0].part_choices.clone();
        choices.sort_by_key(|p| p.0);
        assert_eq!(choices, vec![PartId(10)]);
        assert_eq!(new[0].candidates, vec![PartId(11)]);
        assert_eq!(l.dropped.len(), 1, "{:?}", l.dropped);
        assert_eq!(l.dropped[0].part, PartId(8));
        assert_eq!(l.dropped[0].why, "part number no longer in the file");
    }

    #[test]
    fn split_line_carries_each_half_its_own_parts() {
        // the reverse of a merge: Q1,Q2,Q3 (2N7002) splits into Q1,Q2 (still 2N7002) and Q3 (now BSS138); each
        // new line pairs with the one previous line and only carries the parts that still fit it.
        let p = prev(
            &[OldLine {
                designators: &["Q1", "Q2", "Q3"],
                value: "2N7002",
                choices: &[8],
                candidates: &[9],
                file: Some(&["2N7002", "2N7002K"]),
                match_group: None,
                manual: &[],
                hand: &[],
            }],
            PARTS,
        );
        let mut new = vec![
            line(&["Q1", "Q2"], "2N7002", &["2N7002", "2N7002K"], &[8], &[9]),
            line(&["Q3"], "BSS138", &["SS34"], &[], &[]),
        ];
        let diff = carry_over(&p, &mut new, false, mpn_of);
        assert_eq!(diff.lines.len(), 2, "{:?}", diff.lines);
        assert_eq!(
            diff.count(LineKind::Removed),
            0,
            "fully split, nothing removed"
        );
        for l in &diff.lines {
            assert_eq!(l.previous_designators, vec!["Q1", "Q2", "Q3"]);
            assert!(l.merged_from.is_empty(), "a split, not a merge: {l:?}");
        }
        // Q1,Q2: still 2N7002, keeps its choice and candidate
        assert_eq!(new[0].part_choices, vec![PartId(8)]);
        assert_eq!(new[0].candidates, vec![PartId(9)]);
        assert!(diff.lines[0].dropped.is_empty());
        // Q3: value changed, the 2N7002 parts no longer fit and are dropped, not carried
        assert!(new[1].part_choices.is_empty());
        assert!(new[1].candidates.is_empty());
        assert_eq!(
            diff.lines[1].dropped.len(),
            2,
            "{:?}",
            diff.lines[1].dropped
        );
    }
}
