//! A BOM line as a Digi-Key search (SRCH-4, `mx3 bom suggest --digikey`): the category from the designator, the
//! value and ratings the line asks for as parameter filters, the package from the footprint. A starting point the
//! person narrows further; what the line doesn't say stays open.

use std::collections::HashMap;

use rvariant::si_dynamic::BaseUnit;
use rvariant::util::to_base_unit_f32_opt;

use crate::endpoints::bom::{BomItem, ParameterSource};
use crate::endpoints::part::{Part, PartId};
use crate::endpoints::supplier_search::PartSearch;
use crate::supplier_search::filter::{FilterOp, ParamFilter};

/// Digi-Key's category for a designator prefix, when the kind is clear from it
pub fn category_of(designator: &str) -> Option<&'static str> {
    let prefix: String = designator
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    match prefix.to_ascii_uppercase().as_str() {
        "C" => Some("Ceramic Capacitors"),
        "R" => Some("Chip Resistor - Surface Mount"),
        "L" => Some("Fixed Inductors"),
        "FB" => Some("Ferrite Beads and Chips"),
        _ => None,
    }
}

/// The imperial size code of a footprint name (`Capacitor_SMD:C_0402_1005Metric`, `R_0603`): the first part of
/// four digits.
pub fn package_of(footprint: &str) -> Option<String> {
    footprint
        .split([':', '_', ' '])
        .find(|t| t.len() == 4 && t.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
}

/// `value` in `unit` with an SI prefix, four significant digits at most: 1e-7 F is `100nF`.
pub fn si_text(value: f64, unit: &str) -> String {
    const PREFIXES: [(f64, &str); 9] = [
        (1e9, "G"),
        (1e6, "M"),
        (1e3, "k"),
        (1.0, ""),
        (1e-3, "m"),
        (1e-6, "u"),
        (1e-9, "n"),
        (1e-12, "p"),
        (1e-15, "f"),
    ];
    let abs = value.abs();
    let (scale, prefix) = PREFIXES
        .iter()
        .find(|(s, _)| abs >= *s * 0.9995)
        .copied()
        .unwrap_or((1.0, ""));
    let n = value / scale;
    let text = format!("{:.4}", n);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}{prefix}{unit}")
}

impl BomItem {
    /// The Digi-Key search this line asks for: by the first designator's kind (C, R, L, FB) its category, its value
    /// (capacitance, resistance, inductance) equal, a capacitor's voltage at least the line's, the package from the
    /// footprint; any other line searches its value as keywords.
    pub fn supplier_search(&self, parts: &HashMap<PartId, Part>) -> PartSearch {
        let designator = self.designators.first().map(String::as_str).unwrap_or("");
        let local = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| self.find_parameter(n, parts, ParameterSource::OnlyLocal))
        };
        // a value with its unit, or as a BOM writes it: a bare prefix (`100n`), resistor notation (`4k7`)
        let value = |names: &[&str], unit: BaseUnit, symbol: &str| -> Option<f64> {
            names.iter().find_map(|n| {
                let v = self.find_parameter(n, parts, ParameterSource::PreferLocal);
                if let Ok(x) = to_base_unit_f32_opt(&v, unit.clone()) {
                    return Some(f64::from(x));
                }
                let text = v?.as_non_empty_string().ok()?;
                let text = text.trim();
                if unit == BaseUnit::Ohm
                    && let Ok(r) = rvariant::si_dynamic::OhmF32::parse(text)
                {
                    return Some(f64::from(r.0));
                }
                let with_unit = Some(rvariant::Variant::Str(format!("{text}{symbol}")));
                to_base_unit_f32_opt(&with_unit, unit.clone())
                    .ok()
                    .map(f64::from)
            })
        };
        let eq = |name: &str, text: String| ParamFilter {
            name: name.into(),
            op: FilterOp::Eq(vec![text]),
        };
        let mut q = PartSearch {
            category: category_of(designator).map(str::to_string),
            top: 10,
            ..Default::default()
        };
        match q.category.as_deref() {
            Some("Ceramic Capacitors") => {
                if let Some(c) = value(&["capacitance", "value"], BaseUnit::Farad, "F") {
                    q.params.push(eq("Capacitance", si_text(c, "F")));
                }
                if let Some(v) = value(&["voltage", "voltage - rated"], BaseUnit::Volt, "V") {
                    q.params.push(ParamFilter {
                        name: "Voltage - Rated".into(),
                        op: FilterOp::Ge(si_text(v, "V")),
                    });
                }
                if let Some(d) = local(&["dielectric", "temperature coefficient", "tc"])
                    && let Ok(d) = d.as_non_empty_string()
                {
                    q.params.push(eq("Temperature Coefficient", d));
                }
            }
            Some("Chip Resistor - Surface Mount") => {
                if let Some(r) = value(&["resistance", "value"], BaseUnit::Ohm, "Ω") {
                    q.params.push(eq("Resistance", si_text(r, "Ω")));
                }
            }
            Some("Fixed Inductors") => {
                if let Some(l) = value(&["inductance", "value"], BaseUnit::Henry, "H") {
                    q.params.push(eq("Inductance", si_text(l, "H")));
                }
            }
            _ => {}
        }
        if q.category.is_some()
            && let Some(f) = local(&["footprint", "package"])
            && let Ok(f) = f.as_non_empty_string()
            && let Some(p) = package_of(&f)
        {
            q.package.push(p);
        }
        // no kind, or a kind without a value to filter by: the value as words (often an MPN or a part name)
        if q.params.is_empty()
            && let Some(v) = self.value()
        {
            q.keywords = v;
            if q.category.is_some() && q.package.is_empty() {
                q.category = None;
            }
        }
        q
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn packages_from_footprints() {
        assert_eq!(
            package_of("Capacitor_SMD:C_0402_1005Metric").as_deref(),
            Some("0402")
        );
        assert_eq!(package_of("R_0603").as_deref(), Some("0603"));
        assert_eq!(package_of("Package_TO_SOT_SMD:SOT-23"), None);
    }

    #[test]
    fn si_texts() {
        assert_eq!(si_text(1e-7, "F"), "100nF");
        assert_eq!(si_text(4.7e-6, "F"), "4.7uF");
        assert_eq!(si_text(4700.0, "Ω"), "4.7kΩ");
        assert_eq!(si_text(49900.0, "Ω"), "49.9kΩ");
        assert_eq!(si_text(25.0, "V"), "25V");
        assert_eq!(si_text(0.0, "Ω"), "0Ω");
        assert_eq!(si_text(1e-3, "H"), "1mH");
    }

    #[test]
    fn categories_from_designators() {
        assert_eq!(category_of("C12"), Some("Ceramic Capacitors"));
        assert_eq!(category_of("R3"), Some("Chip Resistor - Surface Mount"));
        assert_eq!(category_of("FB1"), Some("Ferrite Beads and Chips"));
        assert_eq!(category_of("U1"), None);
    }

    /// A line as an import makes it: `100n` and `4k7` as written in KiCad, the voltage, the footprint.
    #[test]
    fn line_values_as_bom_writes_them() {
        use crate::endpoints::bom::BomItemId;
        use crate::endpoints::parameter::ParameterDefId;
        use crate::endpoints::params::Resolved;
        let line = |designator: &str, params: &[(&str, &str)]| BomItem {
            id: BomItemId(1),
            bom_id: crate::endpoints::bom::BomId(1),
            designators: vec![designator.into()],
            group_id: crate::endpoints::bom::BomGroupId(1),
            part_choices: vec![],
            candidates: vec![],
            parameters: params
                .iter()
                .enumerate()
                .map(|(i, (n, v))| {
                    Resolved::new(
                        ParameterDefId(i as i32),
                        n,
                        rvariant::Variant::Str(v.to_string()),
                    )
                })
                .collect(),
            match_group: None,
            part_numbers: None,
            hand_choices: HashSet::new(),
        };
        let parts = HashMap::new();
        let q = line(
            "C3",
            &[
                ("Value", "100n"),
                ("Voltage", "25V"),
                ("Footprint", "Capacitor_SMD:C_0402_1005Metric"),
            ],
        )
        .supplier_search(&parts);
        assert_eq!(q.category.as_deref(), Some("Ceramic Capacitors"));
        assert_eq!(q.package, vec!["0402".to_string()]);
        let f: Vec<String> = q.params.iter().map(|f| f.to_string()).collect();
        assert_eq!(f, vec!["Capacitance=100nF", "Voltage - Rated>=25V"]);
        let q = line("R7", &[("Value", "4k7"), ("Footprint", "R_0603")]).supplier_search(&parts);
        assert_eq!(q.params[0].to_string(), "Resistance=4.7kΩ");
        let q = line("U2", &[("Value", "STM32G0B1KET6")]).supplier_search(&parts);
        assert_eq!((q.category, q.keywords.as_str()), (None, "STM32G0B1KET6"));
    }

    /// The filters a prefilled search sends read the supplier's values as meant: the round trip through
    /// `si_text` and the matcher.
    #[test]
    fn prefilled_filters_match_digikey_values() {
        let c = ParamFilter {
            name: "Capacitance".into(),
            op: FilterOp::Eq(vec![si_text(1e-7, "F")]),
        };
        assert_eq!(c.matches("0.1 µF"), Some(true));
        let r = ParamFilter {
            name: "Resistance".into(),
            op: FilterOp::Eq(vec![si_text(4700.0, "Ω")]),
        };
        assert_eq!(r.matches("4.7 kOhms"), Some(true));
        assert_eq!(r.matches("47 kOhms"), Some(false));
    }
}
