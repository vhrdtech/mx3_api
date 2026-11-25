//! An amount of money in fixed point, nine decimal places.

use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display, Formatter};

#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
pub struct Money(i64);

impl Money {
    pub fn to_f32(&self) -> f32 {
        self.0 as f32 / 1e9
    }

    pub fn from_f32(value: f32) -> Self {
        Self((value * 1e9) as i64)
    }

    pub fn to_f64(&self) -> f64 {
        self.0 as f64 / 1e9
    }

    pub fn from_f64(value: f64) -> Self {
        Self((value * 1e9) as i64)
    }

    pub fn raw(&self) -> i64 {
        self.0
    }

    pub fn from_raw(value: i64) -> Self {
        Self(value)
    }

    pub const fn precision() -> u8 {
        9
    }
}

impl Display for Money {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.04}$", self.to_f32())
    }
}

impl Debug for Money {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

#[cfg(feature = "sqlx")]
impl From<sqlx::postgres::types::PgMoney> for Money {
    fn from(value: sqlx::postgres::types::PgMoney) -> Self {
        Self(value.0)
    }
}

#[cfg(feature = "sqlx")]
impl From<Money> for sqlx::postgres::types::PgMoney {
    fn from(m: Money) -> Self {
        Self(m.0)
    }
}
