//! Ids and helpers shared by the parameter API in [`crate::endpoints::params`]: the definition id, the owner of a
//! value and the one enum the code itself writes (`Part condition`).

use crate::endpoints::bom::{BomId, BomItemId};
use crate::endpoints::part::PartId;
use crate::endpoints::pcb::PcbId;
use crate::endpoints::serial::SerialId;
use crate::endpoints::warehouse::BoxItemId;
use rvariant::{Variant, VariantTy};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct ParameterDefId(pub i32);

impl ParameterDefId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }

    pub fn value(&self) -> i32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum ParameterFor {
    Bom(BomId),
    BomItem(BomItemId),
    Part(PartId),
    Pcb(PcbId),
    Serial(SerialId),
    BoxItem(BoxItemId), // Box(BoxId),
}

/// Name of the enum parameter that marks a box item as new, refurbished or broken.
pub const PART_CONDITION: &str = "Part condition";

/// The type of [`PART_CONDITION`]. Enum values travel as their selection only (rvariant 0.4), so whoever creates
/// the parameter definition from a received value takes the type from here.
pub fn part_condition_ty() -> VariantTy {
    VariantTy::Enum {
        name: PART_CONDITION.into(),
        variants: vec!["New".into(), "Refurbished".into(), "Broken".into()].into(),
    }
}

/// A [`PART_CONDITION`] value with its variants.
pub fn part_condition(selected: &str) -> Variant {
    Variant::Enum {
        name: PART_CONDITION.into(),
        selected: selected.into(),
        variants: vec!["New".into(), "Refurbished".into(), "Broken".into()].into(),
    }
}
