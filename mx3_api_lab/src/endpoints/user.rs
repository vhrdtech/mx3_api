use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct UserId(i32);

impl UserId {
    pub fn hack(id: i32) -> Self {
        Self(id)
    }
}
