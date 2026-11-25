use crate::endpoints::params::Resolved;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct SerialId(pub i32);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct SeriesId(pub i32);

#[derive(Serialize, Deserialize)]
pub struct Serial {
    pub id: SerialId,
    pub series_id: SeriesId,
    pub value: i32,
    pub created: DateTime<Utc>,
    pub parameters: Vec<Resolved>,
}
