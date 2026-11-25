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

impl CompaniesMap {
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
