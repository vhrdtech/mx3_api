use crate::endpoints::companies::DigiKeyMfgId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct ApiKeyId(pub i32);

impl ApiKeyId {
    pub fn value(&self) -> i32 {
        self.0
    }

    pub const fn hack(id: i32) -> Self {
        Self(id)
    }
}

#[derive(Serialize, Deserialize)]
pub struct SearchDigiKeyProductDetails {
    pub api_key: ApiKeyId,
    pub part_number: String,
    pub manufacturer_id: Option<DigiKeyMfgId>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchDigiKeyProductDetailsResult {}
