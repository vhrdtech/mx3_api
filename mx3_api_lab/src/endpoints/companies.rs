#[cfg(feature = "client")]
use crate::MxClient;
#[cfg(feature = "client")]
use crate::promise::Promise;
#[cfg(feature = "client")]
use crate::request::RequestInitiator;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct CompanyId(pub i32);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Copy, Clone)]
pub struct DigiKeyMfgId(pub i32);

impl CompanyId {
    pub fn hack(id: i32) -> Self {
        Self(id)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Company {
    pub id: CompanyId,
    pub name: String,
    pub synonyms: Vec<Self>, // pub website: String,
    // pub logo: String,
    pub digikey_ids: Vec<DigiKeyMfgId>,
}

#[derive(Serialize, Deserialize)]
pub struct CompaniesMap(pub HashMap<CompanyId, Company>);

#[derive(Serialize, Deserialize)]
pub struct CreateCompany {
    pub name: String,
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn get_companies(&self, marker: &'static str) -> Promise<Vec<Company>> {
        let (req, rx) = RequestInitiator::get_no_args("get_companies".into());
        self.send_req(req, rx, marker)
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "public signature mx3's callers use; changing it is its own API change"
    )]
    pub fn create_company(
        &self,
        create_company: CreateCompany,
        marker: &'static str,
    ) -> Promise<CompanyId> {
        let (req, rx) = RequestInitiator::post_json_arg("create_company".into(), &create_company);
        self.send_req(req, rx, marker)
    }

    pub fn update_manufacturers(&self, marker: &'static str) -> Promise<()> {
        let (req, rx) = RequestInitiator::post_json_arg("update_manufacturers".into(), &());
        self.send_req(req, rx, marker)
    }
}

impl Company {
    pub fn is_name_matches(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
            || self
                .synonyms
                .iter()
                .any(|n| n.name.eq_ignore_ascii_case(name))
    }
}

/// Words a maker's name carries that say nothing about which maker it is.
const NAME_NOISE: &[&str] = &[
    "electronics",
    "electronic",
    "inc",
    "incorporated",
    "corp",
    "corporation",
    "co",
    "company",
    "ltd",
    "limited",
    "llc",
    "gmbh",
    "ag",
    "sa",
    "nv",
    "bv",
    "plc",
    "kg",
    "group",
    "international",
    "americas",
    "usa",
    "industries",
    "manufacturing",
    "mfg",
    "the",
];

/// A maker's name without case, punctuation or words like "Inc." and "Electronics": "Murata Electronics North America" stays
/// "muratanorthamerica", "Murata Electronics" and "Murata" are both "murata".
pub fn name_core(name: &str) -> String {
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let core: String = words
        .iter()
        .filter(|w| !NAME_NOISE.contains(w))
        .copied()
        .collect();
    if core.is_empty() {
        words.concat()
    } else {
        core
    }
}

impl Company {
    /// The maker another system names `theirs` is this company (LAB-7): its name or a synonym, exact ignoring
    /// case, else by [`name_core`] ("Nexperia USA Inc." is "Nexperia").
    pub fn maker_matches(&self, theirs: &str) -> bool {
        let theirs = theirs.trim();
        if theirs.is_empty() {
            return false;
        }
        if self.is_name_matches(theirs) {
            return true;
        }
        let t = name_core(theirs);
        std::iter::once(self.name.as_str())
            .chain(self.synonyms.iter().map(|s| s.name.as_str()))
            .any(|n| {
                let c = name_core(n);
                !c.is_empty() && c == t
            })
    }
}

impl CompaniesMap {
    /// The company a maker name means (LAB-7): an exact name or synonym first ([`Self::find_matching`]), else the
    /// one company whose [`Company::maker_matches`] takes it; `None` when none does or several do.
    pub fn find_maker(&self, name: &str) -> Option<CompanyId> {
        if let Some(id) = self.find_matching(name) {
            return Some(id);
        }
        let mut found = self.0.values().filter(|c| c.maker_matches(name));
        let first = found.next()?;
        found.next().is_none().then_some(first.id)
    }

    pub fn find_matching(&self, name: &str) -> Option<CompanyId> {
        if name.is_empty() {
            return None;
        }
        let name = name.trim();
        for company in self.0.values() {
            if company.is_name_matches(name) {
                return Some(company.id);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn company(id: i32, name: &str, synonyms: &[&str]) -> Company {
        Company {
            id: CompanyId(id),
            name: name.into(),
            synonyms: synonyms
                .iter()
                .map(|s| Company {
                    id: CompanyId(id + 1000),
                    name: (*s).to_owned(),
                    synonyms: vec![],
                    digikey_ids: vec![],
                })
                .collect(),
            digikey_ids: vec![],
        }
    }

    #[test]
    fn makers_match_by_their_core_name() {
        assert!(company(1, "Murata", &[]).maker_matches("Murata Electronics"));
        assert!(company(1, "Murata Electronics", &[]).maker_matches("Murata"));
        assert!(company(1, "Texas Instruments", &[]).maker_matches("Texas Instruments Inc."));
        assert!(company(1, "ON Semiconductor", &["onsemi"]).maker_matches("onsemi"));
        assert!(company(1, "Nexperia", &[]).maker_matches("Nexperia USA Inc."));
        assert!(!company(1, "Nexperia", &[]).maker_matches("onsemi"));
        assert!(!company(1, "Murata", &[]).maker_matches(""));
    }

    #[test]
    fn find_maker_takes_exact_then_loose_and_refuses_ambiguity() {
        let map = CompaniesMap(
            [
                company(1, "Nexperia", &["Nexperia USA Inc."]),
                company(2, "Texas Instruments", &["TI"]),
                company(3, "Murata", &[]),
                company(4, "Murata Inc", &[]),
            ]
            .into_iter()
            .map(|c| (c.id, c))
            .collect(),
        );
        assert_eq!(map.find_maker("nexperia usa inc."), Some(CompanyId(1)));
        assert_eq!(map.find_maker("Nexperia BV"), Some(CompanyId(1)));
        assert_eq!(map.find_maker("ti"), Some(CompanyId(2)));
        assert_eq!(map.find_maker("Murata"), Some(CompanyId(3)));
        assert_eq!(map.find_maker("Murata Electronics"), None);
        assert_eq!(map.find_maker("Goodwork"), None);
        assert_eq!(map.find_maker(""), None);
    }
}
