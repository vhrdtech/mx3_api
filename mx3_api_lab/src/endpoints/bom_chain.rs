//! BOM-28: revision chains.
//!
//! BOM-28: revision chains. A BOM is one revision of a chain: the PCB's `main` line of development, or a split-off
//! variant started from another revision. A revision is working or frozen (released, used by an assembled board, or
//! frozen by hand); a frozen one never changes again but for its description. References name a revision the way
//! people do (`35`, `B100 rev B`, `B100B:no-usb@v1`) and resolve against the headers.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::bom::revision::RevisionDiff;
use crate::endpoints::bom::{BomHeader, BomId};
use crate::endpoints::pcb::Pcb;

#[derive(
    Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone, PartialOrd, Ord,
)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct BomChainId(pub i32);

/// Why a revision is frozen
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Copy, Clone)]
#[cfg_attr(
    feature = "sqlx",
    derive(sqlx::Type),
    sqlx(type_name = "bom_freeze_reason", rename_all = "lowercase")
)]
#[serde(rename_all = "lowercase")]
pub enum FreezeReason {
    /// Promoted to a release (`mx3 bom release`), with a label
    Released,
    /// An assembled board was built from it
    Used,
    /// Frozen by hand (`mx3 bom freeze`)
    Manual,
}

impl fmt::Display for FreezeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Released => "released",
            Self::Used => "used",
            Self::Manual => "frozen",
        })
    }
}

/// Where an import puts the new BOM. A client that doesn't say gets a chain of its own, as every BOM was before
/// chains: carrying over into a chain is something to ask for (`mx3 bom import` does).
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChainTarget {
    /// The next revision of the PCB's `main` chain (created when the PCB has none), carrying over from its newest
    Main,
    /// The next revision of the PCB's chain of this name, carrying over from its newest
    Chain(String),
    /// A new chain of this name for the PCB, revision 1, nothing carried over
    NewChain(String),
    /// A chain of its own (`bom-<id>`, or the PCB's `main` when it has none yet), e.g. for the UI's importer,
    /// which doesn't know the PCB yet
    #[default]
    Own,
}

pub const EP_BOM_IMPORT: &str = "/bom/import";
pub const EP_BOM_SPLIT: &str = "/bom/split";
pub const EP_BOM_FREEZE: &str = "/bom/freeze";
pub const EP_BOM_DESCRIBE: &str = "/bom/describe";
pub const EP_BOM_PRUNE: &str = "/bom/prune";

/// The import's answer: the new revision, the one it carried over from and what changed against it
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub header: BomHeader,
    pub previous: Option<BomHeader>,
    pub diff: Option<RevisionDiff>,
    /// Nothing was kept: the answer shows what the import would do
    pub dry_run: bool,
}

/// Start a split-off chain of `bom`'s PCB named `name`: its revision 1 is a copy of `bom`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitBom {
    pub bom: BomId,
    pub name: String,
    /// The new revision's description; default the copied one's
    pub description: Option<String>,
}

/// Freeze a working revision: by hand, or as a release with a label (`v1`, `v2`... of the chain when not given).
/// A revision frozen by hand or used by a board may still be released.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeBom {
    pub bom: BomId,
    pub release: bool,
    pub label: Option<String>,
}

/// The one thing a frozen revision lets change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribeBom {
    pub bom: BomId,
    pub description: String,
}

/// Delete working revisions of a chain.
///
/// Delete working revisions of `bom`'s chain: all but the newest `keep` (at least 1), only those created before
/// `before` when given. Never a frozen revision, one a board refers to, or one a split-off chain started from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneBom {
    pub bom: BomId,
    pub keep: u32,
    pub before: Option<DateTime<Utc>>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneResult {
    pub deleted: Vec<BomHeader>,
    /// The chain's revisions that stay, each with why
    pub kept: Vec<KeptRevision>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeptRevision {
    pub header: BomHeader,
    pub why: String,
}

/// Which revision of a chain a reference names
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevSel {
    Newest,
    /// `@3` or `@r3`
    Rev(i32),
    /// `@released`: the newest released revision
    Released,
    /// `@v1`: the release with this label
    Label(String),
}

/// A reference to a BOM revision: `35` / `#35` (that BOM), or `PCB[:CHAIN][@REV]` where PCB is `B100 rev B`,
/// `B100/B`, `B100B` or a PCB name with one revision, CHAIN defaults to `main` and REV to the newest revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BomRef {
    Id(BomId),
    Chain {
        pcb: String,
        chain: Option<String>,
        rev: RevSel,
    },
}

impl FromStr for BomRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let digits = s.strip_prefix('#').unwrap_or(s);
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            return digits
                .parse()
                .map(|id| Self::Id(BomId(id)))
                .map_err(|e| format!("{s}: {e}"));
        }
        let (rest, rev) = match s.rsplit_once('@') {
            Some((rest, rev)) => {
                let rev = rev.trim();
                let num = rev.strip_prefix(['r', 'R']).unwrap_or(rev);
                let sel = if rev.eq_ignore_ascii_case("released") {
                    RevSel::Released
                } else if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                    RevSel::Rev(num.parse().map_err(|e| format!("{s}: {e}"))?)
                } else if rev.is_empty() {
                    return Err(format!("{s}: nothing after @"));
                } else {
                    RevSel::Label(rev.to_owned())
                };
                (rest, sel)
            }
            None => (s, RevSel::Newest),
        };
        let (pcb, chain) = match rest.rsplit_once(':') {
            Some((pcb, chain)) => (pcb.trim(), Some(chain.trim().to_owned())),
            None => (rest.trim(), None),
        };
        if pcb.is_empty() {
            return Err(format!("{s}: no PCB named"));
        }
        Ok(Self::Chain {
            pcb: pcb.to_owned(),
            chain: chain.filter(|c| !c.is_empty()),
            rev,
        })
    }
}

/// The PCB a reference's PCB text names: `B100 rev B`, `B100/B`, `B100B` (name and revision run together), or a
/// name with only one revision.
pub fn find_pcb<'a>(pcbs: &'a [Pcb], text: &str) -> Result<&'a Pcb, String> {
    let t = text.trim();
    let lower = t.to_lowercase();
    let (name, rev) = if let Some((n, r)) = lower.split_once(" rev ") {
        (n.trim().to_owned(), Some(r.trim().to_owned()))
    } else if let Some((n, r)) = lower.rsplit_once('/') {
        (n.trim().to_owned(), Some(r.trim().to_owned()))
    } else {
        (lower.clone(), None)
    };
    let exact: Vec<&Pcb> = pcbs
        .iter()
        .filter(|p| p.name.to_lowercase() == name)
        .filter(|p| rev.as_ref().is_none_or(|r| p.revision.to_lowercase() == *r))
        .collect();
    let joined: Vec<&Pcb> = if rev.is_none() {
        pcbs.iter()
            .filter(|p| format!("{}{}", p.name, p.revision).to_lowercase() == lower)
            .collect()
    } else {
        vec![]
    };
    match (exact.as_slice(), joined.as_slice()) {
        ([p], _) | ([], [p]) => Ok(p),
        ([], []) => Err(format!("no PCB {t} (`mx3 pcb ls`)")),
        (many, _) => {
            let names: Vec<String> = many
                .iter()
                .map(|p| format!("{} rev {}", p.name, p.revision))
                .collect();
            Err(format!("{t} is ambiguous: {}", names.join(", ")))
        }
    }
}

/// The revision `r` names among `headers` (every BOM's header, as `get_bom_headers` returns them).
pub fn resolve<'a>(
    r: &BomRef,
    pcbs: &[Pcb],
    headers: &'a [BomHeader],
) -> Result<&'a BomHeader, String> {
    match r {
        BomRef::Id(id) => headers
            .iter()
            .find(|h| h.id == *id)
            .ok_or_else(|| format!("no BOM {} (`mx3 bom ls`)", id.0)),
        BomRef::Chain { pcb, chain, rev } => {
            let p = find_pcb(pcbs, pcb)?;
            let chain_name = chain.as_deref().unwrap_or("main");
            let mut revs: Vec<&BomHeader> = headers
                .iter()
                .filter(|h| h.pcb_id == Some(p.id) && h.chain.eq_ignore_ascii_case(chain_name))
                .collect();
            if revs.is_empty() {
                return Err(format!(
                    "{} rev {} has no BOM chain {chain_name} (`mx3 bom ls`)",
                    p.name, p.revision
                ));
            }
            revs.sort_by_key(|h| h.rev);
            let found = match rev {
                RevSel::Newest => revs.last().copied(),
                RevSel::Rev(n) => revs.iter().find(|h| h.rev == *n).copied(),
                RevSel::Released => revs
                    .iter()
                    .rev()
                    .find(|h| h.freeze_reason == Some(FreezeReason::Released))
                    .copied(),
                RevSel::Label(l) => revs
                    .iter()
                    .find(|h| {
                        h.release
                            .as_deref()
                            .is_some_and(|r| r.eq_ignore_ascii_case(l))
                    })
                    .copied(),
            };
            found.ok_or_else(|| {
                format!(
                    "{} rev {}:{chain_name} has no revision {} (`mx3 bom ls`)",
                    p.name,
                    p.revision,
                    match rev {
                        RevSel::Newest => "at all".to_owned(),
                        RevSel::Rev(n) => format!("r{n}"),
                        RevSel::Released => "released".to_owned(),
                        RevSel::Label(l) => l.clone(),
                    }
                )
            })
        }
    }
}

impl BomHeader {
    /// `r3`, `r3 v1` for a release
    pub fn rev_label(&self) -> String {
        match &self.release {
            Some(l) => format!("r{} {l}", self.rev),
            None => format!("r{}", self.rev),
        }
    }

    /// `working`, `released`, `used`, `frozen`
    pub fn state(&self) -> String {
        match self.freeze_reason {
            Some(r) => r.to_string(),
            None if self.frozen => "frozen".into(),
            None => "working".into(),
        }
    }

    /// Why this revision can't be edited and how to change it anyway, for a disabled control; `None`: editable.
    pub fn frozen_why(&self) -> Option<String> {
        if !self.frozen {
            return None;
        }
        let when = self
            .frozen_at
            .map(|t| format!(" on {}", t.format("%Y-%m-%d")))
            .unwrap_or_default();
        let why = match (self.freeze_reason, &self.release) {
            (Some(FreezeReason::Released), Some(l)) => format!("released as {l}{when}"),
            (Some(FreezeReason::Released), None) => format!("released{when}"),
            (Some(FreezeReason::Used), _) => format!("an assembled board was built from it{when}"),
            (Some(FreezeReason::Manual) | None, _) => format!("frozen{when}"),
        };
        Some(format!(
            "{} is frozen: {why}. A frozen revision never changes; import the changes as a new revision \
             (`mx3 bom import`), or start a variant from it (`mx3 bom split`).",
            self.rev_label()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoints::companies::CompanyId;
    use crate::endpoints::pcb::PcbId;
    use crate::endpoints::user::UserId;

    fn pcb(id: i32, name: &str, rev: &str) -> Pcb {
        Pcb {
            id: PcbId::hack(id),
            name: name.into(),
            description: String::new(),
            git_link: String::new(),
            created: Utc::now(),
            modified: Utc::now(),
            revision: rev.into(),
            parent_id: None,
            product: String::new(),
        }
    }

    fn header(id: i32, pcb: i32, chain: &str, rev: i32, release: Option<&str>) -> BomHeader {
        BomHeader {
            id: BomId(id),
            pcb_id: Some(PcbId::hack(pcb)),
            description: String::new(),
            created: Utc::now(),
            updated: Utc::now(),
            user_id: UserId::hack(1),
            company_id: CompanyId(7),
            frozen: release.is_some(),
            chain_id: BomChainId(0),
            chain: chain.into(),
            rev,
            release: release.map(str::to_owned),
            freeze_reason: release.map(|_| FreezeReason::Released),
            frozen_at: None,
            head: false,
            split_from: None,
        }
    }

    #[test]
    fn references_parse() {
        assert_eq!("35".parse(), Ok(BomRef::Id(BomId(35))));
        assert_eq!("#35".parse(), Ok(BomRef::Id(BomId(35))));
        assert_eq!(
            "B100 rev B".parse(),
            Ok(BomRef::Chain {
                pcb: "B100 rev B".into(),
                chain: None,
                rev: RevSel::Newest
            })
        );
        assert_eq!(
            "B100B:no-usb@v1".parse(),
            Ok(BomRef::Chain {
                pcb: "B100B".into(),
                chain: Some("no-usb".into()),
                rev: RevSel::Label("v1".into())
            })
        );
        assert_eq!(
            "B100 rev B@r3".parse::<BomRef>().map(|r| match r {
                BomRef::Chain { rev, .. } => rev,
                BomRef::Id(_) => RevSel::Newest,
            }),
            Ok(RevSel::Rev(3))
        );
        assert_eq!(
            "B100B@released".parse::<BomRef>().map(|r| match r {
                BomRef::Chain { rev, .. } => rev,
                BomRef::Id(_) => RevSel::Newest,
            }),
            Ok(RevSel::Released)
        );
        assert!("B100B@".parse::<BomRef>().is_err());
    }

    #[test]
    fn references_resolve_to_a_chains_revision() {
        let pcbs = [
            pcb(14, "B100", "A"),
            pcb(20, "B100", "B"),
            pcb(5, "B101", "A"),
        ];
        let headers = [
            header(30, 20, "main", 1, None),
            header(31, 20, "main", 2, Some("v1")),
            header(35, 20, "main", 3, None),
            header(40, 20, "no-usb", 1, None),
            header(27, 14, "main", 1, None),
            header(12, 5, "main", 1, None),
        ];
        let id = |s: &str| resolve(&s.parse().unwrap(), &pcbs, &headers).map(|h| h.id.0);
        assert_eq!(id("31"), Ok(31));
        assert_eq!(id("B100 rev B"), Ok(35));
        assert_eq!(id("B100/B"), Ok(35));
        assert_eq!(id("b100b"), Ok(35));
        assert_eq!(id("B100B@released"), Ok(31));
        assert_eq!(id("B100B@v1"), Ok(31));
        assert_eq!(id("B100B@1"), Ok(30));
        assert_eq!(id("B100B:no-usb"), Ok(40));
        assert_eq!(id("B101"), Ok(12));
        assert_eq!(id("B100 rev A"), Ok(27));
        assert!(id("B100").unwrap_err().contains("ambiguous"));
        assert!(id("B100B:nope").is_err());
        assert!(id("B100B@v9").is_err());
    }

    #[test]
    fn frozen_why_says_the_reason_and_the_way_out() {
        let mut h = header(5, 3, "main", 2, None);
        assert_eq!(h.frozen_why(), None);
        h.frozen = true;
        h.freeze_reason = Some(FreezeReason::Released);
        h.release = Some("v1".into());
        let why = h.frozen_why().unwrap();
        assert!(why.starts_with("r2 v1 is frozen: released as v1."), "{why}");
        assert!(
            why.contains("mx3 bom import") && why.contains("mx3 bom split"),
            "{why}"
        );
        h.freeze_reason = Some(FreezeReason::Used);
        h.release = None;
        assert!(
            h.frozen_why()
                .unwrap()
                .contains("an assembled board was built from it")
        );
    }
}
