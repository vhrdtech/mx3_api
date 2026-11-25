//! One part for an MPN that several parts share (BOM-25): `mx3 bom import` and `mx3 bom tidy` keep one of them as
//! the line's real part choice and turn the others into candidates (BOM-24).
//!
//! The rule narrows the parts step by step; a step that matches none of the parts left is skipped:
//! 1. the manufacturer the BOM file names for that MPN (`Mfr1`/`Manufacturer1` next to `Mpn1`);
//! 2. the part a BOM of the same PCB family (same PCB name, or a parent PCB) already uses for that MPN, where
//!    that line has only one real choice with the MPN, and only a part with a known maker: older BOMs often use
//!    the "Unknown" placeholder the import made, which says nothing about the maker;
//! 3. a part with stock in the warehouse;
//! 4. a part with a known manufacturer and an active lifecycle (not "Unknown", not obsolete, discontinued,
//!    last time buy or not for new designs);
//! 5. ties: the oldest part.

use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};

use chrono::{DateTime, Utc};
use serde::Serialize;

#[cfg(feature = "client")]
use crate::blocking::BlockingClient;
use crate::bom::import::ImportItems;
#[cfg(feature = "client")]
use crate::bom::import::{BomTable, PreparedBom};
#[cfg(feature = "client")]
use crate::endpoints::bom::BomItemCandidates;
use crate::endpoints::bom::{Bom, BomId, BomItemId};
use crate::endpoints::companies::{Company, CompanyId};
#[cfg(feature = "client")]
use crate::endpoints::part::LookupPartResult;
use crate::endpoints::part::{Part, PartId};
use crate::endpoints::pcb::{Pcb, PcbId};

/// The maker name of the placeholder company parts get when imported without one
pub const UNKNOWN_MAKER: &str = "Unknown";

/// What the rule knows about one part of an MPN
#[derive(Clone, Debug, Serialize)]
pub struct ChoiceFacts {
    pub part: PartId,
    pub mpn: String,
    pub maker: String,
    /// Other names of the maker (company synonyms)
    pub maker_synonyms: Vec<String>,
    pub created: DateTime<Utc>,
    /// Parts in the warehouse
    pub stock: u64,
    /// BOMs of the PCB family that use this part for its MPN
    pub family_boms: Vec<BomId>,
    /// Digi-Key's product status ("Active", "Obsolete", "Discontinued at Digi-Key"...)
    pub status: Option<String>,
    /// Digi-Key's Discontinued or End of life flag
    pub discontinued: bool,
}

impl ChoiceFacts {
    pub fn of_part(
        part: &Part,
        companies: &HashMap<CompanyId, Company>,
        family: &FamilyUse,
    ) -> ChoiceFacts {
        let company = companies.get(&part.mfg);
        let flag = |name: &str| {
            part.find_parameter(name)
                .and_then(|v| v.as_bool().ok())
                .unwrap_or(false)
        };
        ChoiceFacts {
            part: part.id,
            mpn: part.part_number.clone(),
            maker: company.map(|c| c.name.clone()).unwrap_or_default(),
            maker_synonyms: company
                .map(|c| c.synonyms.iter().map(|s| s.name.clone()).collect())
                .unwrap_or_default(),
            created: part.created,
            stock: part.warehouse_stock(),
            family_boms: family.get(&part.id).cloned().unwrap_or_default(),
            status: part
                .find_parameter("Product status")
                .map(|v| v.to_string())
                .filter(|s| !s.is_empty()),
            discontinued: flag("Discontinued") || flag("End of life"),
        }
    }

    pub fn maker_known(&self) -> bool {
        !self.maker.trim().is_empty() && !self.maker.eq_ignore_ascii_case(UNKNOWN_MAKER)
    }

    /// Why the part is not active, `None` when it is (or nothing is known against it)
    pub fn inactive(&self) -> Option<String> {
        if let Some(status) = &self.status {
            let s = status.to_lowercase();
            const INACTIVE: [&str; 6] = [
                "obsolete",
                "discontinued",
                "not for new designs",
                "nrnd",
                "last time buy",
                "end of life",
            ];
            if INACTIVE.iter().any(|w| s.contains(w)) {
                return Some(status.clone());
            }
        }
        self.discontinued.then(|| "discontinued".to_string())
    }

    /// Known maker and active lifecycle
    pub fn active(&self) -> bool {
        self.maker_known() && self.inactive().is_none()
    }

    /// The maker is the one a BOM file names: the same name or synonym ignoring case, punctuation and company
    /// suffixes ("NXP" = "NXP USA Inc.", "Nexperia" = "Nexperia USA Inc.")
    pub fn matches_maker(&self, name: &str) -> bool {
        let want = maker_core(name);
        if want.is_empty() || !self.maker_known() {
            return false;
        }
        std::iter::once(&self.maker)
            .chain(&self.maker_synonyms)
            .map(|n| maker_core(n))
            .any(|have| !have.is_empty() && (have.starts_with(&want) || want.starts_with(&have)))
    }
}

/// A maker's name without punctuation, case and the words every company name carries
fn maker_core(name: &str) -> String {
    const SUFFIXES: [&str; 18] = [
        "inc",
        "incorporated",
        "ltd",
        "limited",
        "llc",
        "gmbh",
        "co",
        "corp",
        "corporation",
        "company",
        "usa",
        "semiconductor",
        "semiconductors",
        "electronics",
        "technology",
        "technologies",
        "international",
        "ag",
    ];
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !SUFFIXES.contains(w))
        .collect()
}

/// Why a part was picked: the first step of the rule that narrowed the parts
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum PickReason {
    /// The only part with the MPN
    Only,
    /// The manufacturer the BOM file names
    Maker { name: String },
    /// Used for the MPN on these BOMs of the PCB family
    Family { boms: Vec<BomId> },
    /// In the warehouse
    Stock { count: u64 },
    /// The only ones with a known maker and an active lifecycle
    Active,
    /// Nothing else told them apart
    Oldest,
}

impl Display for PickReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PickReason::Only => write!(f, "only part"),
            PickReason::Maker { name } => write!(f, "maker {name} from the file"),
            PickReason::Family { boms } => {
                let boms: Vec<String> = boms.iter().map(|b| b.0.to_string()).collect();
                write!(f, "used on BOM {}", boms.join(", "))
            }
            PickReason::Stock { count } => write!(f, "{count} in the warehouse"),
            PickReason::Active => write!(f, "known maker, active"),
            PickReason::Oldest => write!(f, "oldest"),
        }
    }
}

/// The part kept as the real choice and the ones that become candidates
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Pick {
    pub part: PartId,
    pub reason: PickReason,
    /// Several parts were left after `reason` and the oldest was taken
    pub oldest_of: Option<usize>,
    /// The maker the file named, when none of the parts has it
    pub maker_not_found: Option<String>,
    pub candidates: Vec<PartId>,
}

impl Display for Pick {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)?;
        if let Some(n) = self.oldest_of
            && self.reason != PickReason::Oldest
        {
            write!(f, ", oldest of {n}")?;
        }
        if let Some(m) = &self.maker_not_found {
            write!(f, " (no part by {m})")?;
        }
        Ok(())
    }
}

/// One part of `facts` by the rule in the module docs; `None` for no parts
pub fn pick(facts: &[ChoiceFacts], maker: Option<&str>) -> Option<Pick> {
    #[derive(Copy, Clone)]
    enum Step {
        Maker,
        Family,
        Stock,
        Active,
    }
    let maker = maker.map(str::trim).filter(|m| !m.is_empty());
    let mut left: Vec<&ChoiceFacts> = facts.iter().collect();
    let mut step = None;
    let mut maker_not_found = None;
    if let Some(m) = maker {
        if facts.iter().any(|f| f.matches_maker(m)) {
            if narrow(&mut left, |f| f.matches_maker(m)) {
                step = Some(Step::Maker);
            }
        } else if facts.len() > 1 {
            maker_not_found = Some(m.to_string());
        }
    }
    type Test = fn(&ChoiceFacts) -> bool;
    let steps: [(Step, Test); 3] = [
        // the placeholder maker is no choice of maker, even where a family BOM uses it
        (Step::Family, |f| {
            !f.family_boms.is_empty() && f.maker_known()
        }),
        (Step::Stock, |f| f.stock > 0),
        (Step::Active, ChoiceFacts::active),
    ];
    for (s, test) in steps {
        if narrow(&mut left, test) && step.is_none() {
            step = Some(s);
        }
    }
    let best = *left.iter().min_by_key(|f| (f.created, f.part.0))?;
    let reason = match step {
        _ if facts.len() == 1 => PickReason::Only,
        Some(Step::Maker) => PickReason::Maker {
            name: maker.unwrap_or_default().to_string(),
        },
        Some(Step::Family) => {
            let mut boms = best.family_boms.clone();
            boms.sort_unstable_by_key(|b| b.0);
            PickReason::Family { boms }
        }
        Some(Step::Stock) => PickReason::Stock { count: best.stock },
        Some(Step::Active) => PickReason::Active,
        None => PickReason::Oldest,
    };
    Some(Pick {
        part: best.part,
        reason,
        oldest_of: (left.len() > 1).then_some(left.len()),
        maker_not_found,
        candidates: facts
            .iter()
            .map(|f| f.part)
            .filter(|p| *p != best.part)
            .collect(),
    })
}

/// Keeps the parts that pass `test` when some do and some don't; true when it narrowed
fn narrow(left: &mut Vec<&ChoiceFacts>, test: impl Fn(&ChoiceFacts) -> bool) -> bool {
    let n = left.iter().filter(|f| test(f)).count();
    if n == 0 || n == left.len() {
        return false;
    }
    left.retain(|f| test(f));
    true
}

/// The PCBs whose BOMs count as the same family as `pcb`'s: the same PCB name (any revision) and the parents,
/// with their names
pub fn family_pcbs(pcbs: &[Pcb], pcb: PcbId) -> HashSet<PcbId> {
    let by_id: HashMap<PcbId, &Pcb> = pcbs.iter().map(|p| (p.id, p)).collect();
    let mut names = HashSet::new();
    let mut at = by_id.get(&pcb).copied();
    let mut seen = HashSet::new();
    while let Some(p) = at {
        if !seen.insert(p.id) {
            break;
        }
        names.insert(p.name.to_lowercase());
        at = p.parent_id.and_then(|id| by_id.get(&id).copied());
    }
    let mut family: HashSet<PcbId> = pcbs
        .iter()
        .filter(|p| names.contains(&p.name.to_lowercase()))
        .map(|p| p.id)
        .collect();
    family.extend(seen);
    family
}

/// Parts the family's BOMs use for their MPN: part → BOMs
pub type FamilyUse = HashMap<PartId, Vec<BomId>>;

/// The parts `boms` use: a line's real choice counts when it is the only real choice with its MPN on that line,
/// so a line that holds several makers of one MPN (what this rule cleans up) decides nothing
pub fn family_use<'a>(boms: impl IntoIterator<Item = &'a Bom>) -> FamilyUse {
    let mut used: FamilyUse = HashMap::new();
    for bom in boms {
        for item in &bom.items {
            for parts in by_mpn(&item.part_choices, &bom.referenced_parts).values() {
                if let [part] = parts[..] {
                    let boms = used.entry(part).or_default();
                    if !boms.contains(&bom.id) {
                        boms.push(bom.id);
                    }
                }
            }
        }
    }
    used
}

/// Parts by MPN, ignoring case; parts not in `parts` are left out
fn by_mpn(choices: &[PartId], parts: &HashMap<PartId, Part>) -> HashMap<String, Vec<PartId>> {
    let mut map: HashMap<String, Vec<PartId>> = HashMap::new();
    for id in choices {
        if let Some(part) = parts.get(id) {
            map.entry(part.part_number.trim().to_uppercase())
                .or_default()
                .push(*id);
        }
    }
    map
}

/// One line of `mx3 bom tidy`: several real choices with one MPN, of which one stays
#[derive(Clone, Debug, Serialize)]
pub struct TidyLine {
    pub item_id: BomItemId,
    pub designators: Vec<String>,
    pub mpn: String,
    pub pick: Pick,
}

/// The lines of `bom` with several real choices of one MPN and what the rule keeps of each
pub fn tidy_plan(
    bom: &Bom,
    companies: &HashMap<CompanyId, Company>,
    family: &FamilyUse,
) -> Vec<TidyLine> {
    let mut plan = vec![];
    for item in &bom.items {
        let mut groups: Vec<(String, Vec<PartId>)> =
            by_mpn(&item.part_choices, &bom.referenced_parts)
                .into_iter()
                .filter(|(_, parts)| parts.len() > 1)
                .collect();
        groups.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, parts) in groups {
            let facts: Vec<ChoiceFacts> = parts
                .iter()
                .filter_map(|id| bom.referenced_parts.get(id))
                .map(|p| ChoiceFacts::of_part(p, companies, family))
                .collect();
            if let Some(pick) = pick(&facts, None) {
                plan.push(TidyLine {
                    item_id: item.id,
                    designators: item.designators.clone(),
                    mpn: facts[0].mpn.clone(),
                    pick,
                });
            }
        }
    }
    plan
}

/// What the rule needs besides the parts: the companies with their synonyms and what the PCB family uses
#[derive(Default)]
pub struct PickContext {
    pub companies: HashMap<CompanyId, Company>,
    pub family: FamilyUse,
}

impl PickContext {
    pub fn maker(&self, part: Option<&Part>) -> &str {
        part.and_then(|p| self.companies.get(&p.mfg))
            .map(|c| c.name.as_str())
            .unwrap_or("?")
    }
}

/// `mx3 bom import`'s lines with the picks, and the parts several of them share
pub struct ImportRun {
    pub items: ImportItems,
    pub context: PickContext,
    pub shared: HashMap<PartId, Part>,
}

#[cfg(feature = "client")]
impl BlockingClient {
    /// Companies, and the parts the BOMs of `pcb`'s family other than `exclude` use
    pub fn pick_context(
        &self,
        pcb: Option<PcbId>,
        exclude: Option<BomId>,
    ) -> anyhow::Result<PickContext> {
        let companies = self
            .get_companies()?
            .into_iter()
            .map(|c| (c.id, c))
            .collect();
        let mut boms = vec![];
        if let Some(pcb) = pcb {
            let family = family_pcbs(&self.get_pcbs()?, pcb);
            for h in self.get_bom_headers()? {
                if h.pcb_id.is_some_and(|p| family.contains(&p)) && Some(h.id) != exclude {
                    boms.push(self.get_bom(h.id)?);
                }
            }
        }
        Ok(PickContext {
            companies,
            family: family_use(&boms),
        })
    }

    /// The lines of a BOM file for `pcb`: part numbers looked up, and one real choice picked where several parts
    /// share one, the others candidates
    pub fn import_items(
        &self,
        table: &BomTable,
        prepared: &PreparedBom,
        pcb: PcbId,
    ) -> anyhow::Result<ImportRun> {
        let lookup = table.lookup_request(prepared);
        let result = self.lookup_parts(&lookup)?;
        let mut shared: HashMap<PartId, Part> = HashMap::new();
        for r in &result.parts {
            if let LookupPartResult::NotUnique(ids) = r {
                for id in ids {
                    if !shared.contains_key(id) {
                        shared.insert(*id, self.get_part(*id)?);
                    }
                }
            }
        }
        let context = if shared.is_empty() {
            PickContext::default()
        } else {
            self.pick_context(Some(pcb), None)?
        };
        let items = table.create_bom_items(prepared, &lookup, Some(&result), |ids, maker| {
            let facts: Vec<ChoiceFacts> = ids
                .iter()
                .filter_map(|id| shared.get(id))
                .map(|p| ChoiceFacts::of_part(p, &context.companies, &context.family))
                .collect();
            pick(&facts, maker)
        });
        Ok(ImportRun {
            items,
            context,
            shared,
        })
    }

    /// `mx3 bom tidy`'s plan for a BOM; nothing is changed
    pub fn tidy_plan(&self, bom: BomId) -> anyhow::Result<(Bom, PickContext, Vec<TidyLine>)> {
        let bom = self.get_bom(bom)?;
        let context = self.pick_context(bom.header.pcb_id, Some(bom.id))?;
        let plan = tidy_plan(&bom, &context.companies, &context.family);
        Ok((bom, context, plan))
    }

    /// Carries out a tidy plan: each line's other parts become candidates
    pub fn tidy_apply(&self, plan: &[TidyLine]) -> anyhow::Result<()> {
        for l in plan {
            self.make_bom_item_candidates(&BomItemCandidates {
                bom_item_id: l.item_id,
                parts: l.pick.candidates.iter().copied().collect(),
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use chrono::TimeZone;

    /// One MPN, three makers: a part without maker, a discontinued NXP listing and an active Nexperia part
    pub(crate) fn three_makers() -> Vec<ChoiceFacts> {
        let at = |d| Utc.with_ymd_and_hms(2026, 1, d, 0, 0, 0).unwrap();
        let base = |id, maker: &str, day| ChoiceFacts {
            part: PartId(id),
            mpn: "BAT54CW,115".into(),
            maker: maker.into(),
            maker_synonyms: vec![],
            created: at(day),
            stock: 0,
            family_boms: vec![],
            status: Some("Active".into()),
            discontinued: false,
        };
        vec![
            ChoiceFacts {
                status: None,
                ..base(1, "Unknown", 1)
            },
            ChoiceFacts {
                status: Some("Discontinued at Digi-Key".into()),
                discontinued: true,
                ..base(2, "NXP USA Inc.", 2)
            },
            base(3, "Nexperia USA Inc.", 3),
        ]
    }

    #[test]
    fn active_known_maker_wins_without_hint() {
        let p = pick(&three_makers(), None).unwrap();
        assert_eq!(p.part, PartId(3));
        assert_eq!(p.reason, PickReason::Active);
        assert_eq!(p.candidates, vec![PartId(1), PartId(2)]);
        assert_eq!(p.to_string(), "known maker, active");
    }

    #[test]
    fn maker_from_the_file_wins() {
        let p = pick(&three_makers(), Some("NXP")).unwrap();
        assert_eq!(p.part, PartId(2));
        assert_eq!(p.reason, PickReason::Maker { name: "NXP".into() });
        assert_eq!(p.candidates, vec![PartId(1), PartId(3)]);
    }

    #[test]
    fn unknown_maker_hint_falls_through() {
        let p = pick(&three_makers(), Some("Diodes")).unwrap();
        assert_eq!(p.part, PartId(3));
        assert_eq!(p.maker_not_found.as_deref(), Some("Diodes"));
        assert_eq!(p.to_string(), "known maker, active (no part by Diodes)");
    }

    #[test]
    fn family_beats_stock_beats_active() {
        let mut f = three_makers();
        f[0].stock = 100;
        let p = pick(&f, None).unwrap();
        assert_eq!(p.part, PartId(1));
        assert_eq!(p.reason, PickReason::Stock { count: 100 });

        f[1].family_boms = vec![BomId(29), BomId(30)];
        let p = pick(&f, None).unwrap();
        assert_eq!(p.part, PartId(2));
        assert_eq!(p.to_string(), "used on BOM 29, 30");

        // the unknown placeholder on a family BOM does not count
        let mut f = three_makers();
        f[0].family_boms = vec![BomId(29)];
        assert_eq!(pick(&f, None).unwrap().part, PartId(3));
    }

    #[test]
    fn ties_take_the_oldest() {
        let mut f = three_makers();
        f[1].status = Some("Active".into());
        f[1].discontinued = false;
        let p = pick(&f, None).unwrap();
        assert_eq!(p.part, PartId(2));
        assert_eq!(p.oldest_of, Some(2));
        assert_eq!(p.to_string(), "known maker, active, oldest of 2");

        let all_unknown: Vec<_> = three_makers()
            .into_iter()
            .map(|f| ChoiceFacts {
                maker: "Unknown".into(),
                ..f
            })
            .collect();
        let p = pick(&all_unknown, None).unwrap();
        assert_eq!((p.part, p.reason), (PartId(1), PickReason::Oldest));
    }

    #[test]
    fn single_part() {
        let f = &three_makers()[2..];
        let p = pick(f, Some("Diodes")).unwrap();
        assert_eq!(p.reason, PickReason::Only);
        assert!(p.candidates.is_empty());
        assert!(pick(&[], None).is_none());
    }

    #[test]
    fn maker_names_match_loosely() {
        let f = &three_makers()[2];
        assert!(f.matches_maker("Nexperia"));
        assert!(f.matches_maker("nexperia b.v."));
        assert!(!f.matches_maker("NXP"));
        assert!(!three_makers()[0].matches_maker("Unknown"));
        assert!(three_makers()[1].matches_maker("NXP Semiconductors"));
    }

    fn pcb(id: i32, name: &str, parent: Option<i32>) -> Pcb {
        Pcb {
            id: PcbId::hack(id),
            name: name.into(),
            description: "".into(),
            git_link: "".into(),
            created: Utc::now(),
            modified: Utc::now(),
            revision: "A".into(),
            parent_id: parent.map(PcbId::hack),
            product: String::new(),
        }
    }

    #[test]
    fn family_is_same_name_and_parents() {
        let pcbs = [
            pcb(1, "B153", None),
            pcb(2, "B153", None),
            pcb(3, "B153X", Some(1)),
            pcb(4, "B200", None),
        ];
        let f = family_pcbs(&pcbs, PcbId::hack(3));
        let mut f: Vec<_> = f.into_iter().map(|p| p.value()).collect();
        f.sort();
        assert_eq!(f, vec![1, 2, 3]);
        assert_eq!(family_pcbs(&pcbs, PcbId::hack(4)).len(), 1);
    }
}
