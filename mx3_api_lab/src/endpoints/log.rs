use serde::{Deserialize, Serialize};
use strum::EnumDiscriminants;

#[derive(Debug, Serialize, Deserialize, PartialEq, EnumDiscriminants)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(type_name = "log_severity"))]
pub enum LogSeverity {
    Error,
    Warning,
    Info,
    Debug,
    Trace,
}
