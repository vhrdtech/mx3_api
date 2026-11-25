//! BOM logic shared by the UI and the CLI: editor filters and line lookups, cost analysis (BOM-6), import
//! (BOM-1) and assembly planning (PCB-2). Pure functions over the API types; no requests are sent from here.

pub mod assembly;
pub mod cost;
pub mod export;
pub mod import;
pub mod market;
pub mod pick;
pub mod revision;
pub mod suggest;

use std::collections::HashMap;

use rvariant::si_dynamic::BaseUnit;
use rvariant::util::to_base_unit_f32_opt;
use serde::{Deserialize, Serialize};

use crate::endpoints::bom::{Bom, BomGroupId, BomItem, ParameterSource};
use crate::endpoints::part::{
    LookupPartResult, LookupParts, LookupPartsResult, MpnString, Part, PartId,
};
use crate::endpoints::search::{ParametricSearch, ParametricSearchKind};

/// Parameters searched by the editor's text filter, besides designators and part numbers
const TEXT_FILTER_PARAMS: [&str; 9] = [
    "Value",
    "Voltage",
    "Voltage - Rated",
    "Capacitance",
    "Resistance",
    "Detailed description",
    "Description",
    "Temperature Coefficient",
    "Package / Case",
];

/// BOM editor filters (BOM-4)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BomFilter {
    /// Substring of a designator, a part number or a common parameter, case-insensitive
    pub text: String,
    /// Designator prefix ("R", "C", "U"...); "Any" or empty for all
    pub designator_kind: String,
    /// Only lines that are not R, L or C
    pub risky: bool,
    /// Only lines with fewer parts in the warehouse than needed
    pub non_warehouse: bool,
    /// Only lines with fewer parts at suppliers and the manufacturer than needed
    pub non_stocked: bool,
}

impl Default for BomFilter {
    fn default() -> Self {
        Self {
            text: "".into(),
            designator_kind: "Any".into(),
            risky: false,
            non_warehouse: false,
            non_stocked: false,
        }
    }
}

impl BomFilter {
    pub fn matches(
        &self,
        item: &BomItem,
        parts: &HashMap<PartId, Part>,
        assemble_count: u32,
    ) -> bool {
        if !self.text.is_empty() && !Self::text_matches(&self.text, item, parts) {
            return false;
        }
        let required_parts = assemble_count as u64 * item.designators.len() as u64;
        if self.non_warehouse && item.warehouse_stock(parts) >= required_parts {
            return false;
        }
        if self.non_stocked
            && item.supplier_stock(parts) + item.manufacturer_stock(parts) >= required_parts
        {
            return false;
        }
        if self.risky
            && item
                .designators
                .iter()
                .any(|d| d.starts_with(['R', 'L', 'C']))
        {
            return false;
        }
        if self.designator_kind != "Any"
            && !self.designator_kind.is_empty()
            && !item
                .designators
                .iter()
                .any(|d| d.starts_with(&self.designator_kind))
        {
            return false;
        }
        true
    }

    fn text_matches(text: &str, item: &BomItem, parts: &HashMap<PartId, Part>) -> bool {
        let text = text.to_lowercase();
        if item
            .designators
            .iter()
            .any(|d| d.to_lowercase().contains(&text))
        {
            return true;
        }
        let params =
            item.find_parameter_all(&TEXT_FILTER_PARAMS, parts, ParameterSource::PreferLocal);
        if params.iter().any(|p| p.to_lowercase().contains(&text)) {
            return true;
        }
        item.part_choices
            .iter()
            .filter_map(|id| parts.get(id))
            .any(|part| part.part_number.to_lowercase().contains(&text))
    }
}

impl Bom {
    /// Group by name, `None` for "All groups" or an unknown name
    pub fn group_id(&self, name: &str) -> Option<BomGroupId> {
        self.groups
            .iter()
            .find_map(|(id, group)| (group.name == name).then_some(*id))
    }

    /// Index of the line that has this designator (case-insensitive)
    pub fn find_line(&self, designator: &str) -> Option<usize> {
        self.items.iter().position(|item| {
            item.designators
                .iter()
                .any(|d| d.eq_ignore_ascii_case(designator))
        })
    }

    /// Lines whose "Value" is not a part number among their choices yet: looked up as MPNs, so an obvious
    /// choice ("+1*" in the editor) can be offered
    pub fn values_as_mpn_lookup(&self) -> LookupParts {
        let mut mpns = vec![];
        'outer: for item in self.items.iter() {
            let Some(value) = item.value() else {
                continue;
            };
            for choice in &item.part_choices {
                let Some(part) = self.referenced_parts.get(choice) else {
                    continue;
                };
                if part.part_number == value {
                    continue 'outer;
                }
            }
            mpns.push(MpnString {
                manufacturer_name: "".to_string(),
                part_number: value,
            });
        }
        LookupParts { mpns }
    }

    /// Part ids worth crawling again: supply data older than `hours`
    pub fn stale_parts(&self, hours: i64) -> std::collections::HashSet<PartId> {
        self.outdated_supply(|dt| dt.num_hours() >= hours)
    }
}

impl BomItem {
    /// The BOM's own "Value" parameter of this line (as imported, not from suppliers)
    pub fn value(&self) -> Option<String> {
        for p in self.parameters.iter().filter(|p| !p.retracted) {
            if p.name.eq_ignore_ascii_case("value")
                && let Ok(v) = p.value.as_non_empty_string()
            {
                return Some(v);
            }
        }
        None
    }

    /// The part whose number equals this line's value, if the lookup found exactly that part
    pub fn value_as_part(&self, lookup: Option<&LookupPartsResult>) -> Option<PartId> {
        let value = self.value()?;
        lookup?.parts.iter().find_map(|p| match p {
            LookupPartResult::Found { id, pn } if pn == &value => Some(*id),
            _ => None,
        })
    }

    /// Any part choice sits in a box inside a "PnP" box (loaded into the pick and place)
    pub fn in_pnp(&self, parts: &HashMap<PartId, Part>) -> bool {
        self.part_choices
            .iter()
            .filter_map(|id| parts.get(id))
            .flat_map(|part| &part.warehouse_stock)
            .any(|b| {
                b.parent_chain
                    .iter()
                    .any(|c| c.to_lowercase().contains("pnp"))
            })
    }

    pub fn any_watched(&self, parts: &HashMap<PartId, Part>) -> bool {
        self.part_choices
            .iter()
            .filter_map(|id| parts.get(id))
            .any(|part| part.is_watched)
    }

    /// Parametric search for parts that fit this line: capacitors by capacitance and voltage, resistors by
    /// resistance. `None` for other designators or when the values can't be read.
    pub fn parametric_search(&self, parts: &HashMap<PartId, Part>) -> Option<ParametricSearch> {
        let designator = self.designators.first()?;
        let kind = if designator.starts_with('C') {
            self.capacitor_search(parts)?
        } else if designator.starts_with('R') {
            self.resistor_search(parts)?
        } else {
            return None;
        };
        Some(ParametricSearch {
            kind,
            warehouse: false,
            suppliers: false,
            mpn_contains: None,
        })
    }

    fn capacitor_search(&self, parts: &HashMap<PartId, Part>) -> Option<ParametricSearchKind> {
        let capacitance = self.find_parameter("capacitance", parts, ParameterSource::PreferLocal);
        let mut capacitance_farads = to_base_unit_f32_opt(&capacitance, BaseUnit::Farad);
        if capacitance_farads.is_err() {
            let value = self.find_parameter("value", parts, ParameterSource::PreferLocal);
            capacitance_farads = to_base_unit_f32_opt(&value, BaseUnit::Farad);
        }
        let voltage = self.find_parameter("voltage", parts, ParameterSource::PreferLocal);
        let mut voltage_volts = to_base_unit_f32_opt(&voltage, BaseUnit::Volt);
        if voltage_volts.is_err() {
            let voltage_from_parts = self.find_parameter_many(
                &["voltage", "voltage - rated"],
                parts,
                ParameterSource::PreferLocal,
            );
            voltage_volts = to_base_unit_f32_opt(&voltage_from_parts, BaseUnit::Volt);
        }
        Some(ParametricSearchKind::Capacitor {
            capacitance_farads: capacitance_farads.ok()?,
            voltage_volts: voltage_volts.ok()?,
            is_voltage_eq: true,
        })
    }

    fn resistor_search(&self, parts: &HashMap<PartId, Part>) -> Option<ParametricSearchKind> {
        let resistance = self.find_parameter("resistance", parts, ParameterSource::PreferLocal);
        let mut resistance_ohms = to_base_unit_f32_opt(&resistance, BaseUnit::Ohm);
        if resistance_ohms.is_err() {
            let value = self.find_parameter("value", parts, ParameterSource::PreferLocal);
            resistance_ohms = to_base_unit_f32_opt(&value, BaseUnit::Ohm);
        }
        Some(ParametricSearchKind::Resistor {
            resistance_ohms: resistance_ohms.ok()?,
        })
    }
}

impl ParametricSearch {
    /// The same capacitor search accepting any voltage at or above the required one, in the warehouse and at
    /// suppliers ("Relax voltage requirement" in the editor). `None` for other searches.
    pub fn relax_voltage(&self) -> Option<ParametricSearch> {
        let ParametricSearchKind::Capacitor {
            capacitance_farads,
            voltage_volts,
            is_voltage_eq: true,
        } = self.kind
        else {
            return None;
        };
        Some(ParametricSearch {
            kind: ParametricSearchKind::Capacitor {
                capacitance_farads,
                voltage_volts,
                is_voltage_eq: false,
            },
            warehouse: true,
            suppliers: true,
            mpn_contains: None,
        })
    }
}
