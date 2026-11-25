//! `GET /version` (PLT-24, no token): what the backend runs, so a client can tell core-1's build without
//! guessing from which endpoints answer 404. Used by `mx3 version` and the UI's About window.

use serde::{Deserialize, Serialize};

#[cfg(feature = "client")]
use crate::request::RequestInitiator;
#[cfg(feature = "client")]
use crate::{MxClient, Promise};

pub const EP_VERSION: &str = "/version";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackendVersion {
    pub version: String,
    pub git_sha: String,
    pub build_time: String,
    /// This build's wire level (`mx3_api::API_LEVEL`)
    pub api_level: u32,
    /// The newest migration the database has applied, `None` on one not yet set up
    pub last_migration: Option<i64>,
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn version(&self) -> Promise<BackendVersion> {
        let (req, rx) = RequestInitiator::get_no_args(EP_VERSION.trim_start_matches('/').into());
        self.send_req(req, rx, "version")
    }
}
