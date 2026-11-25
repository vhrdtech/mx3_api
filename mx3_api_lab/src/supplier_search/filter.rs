//! Parameter filters of a supplier search (SRCH-4): `Voltage - Rated>=25V`, `Capacitance=100n`,
//! `Dielectric=X7R,X5R`, `Resistance=1k..10k`, `Package / Case~0402`, read into a [ParamFilter], and which of a
//! supplier's value texts (`50V`, `0.1 µF`, `±10%`, `-55°C ~ 125°C`, `100mOhm @ 4.5V, 10A`) satisfy it. Digi-Key
//! filters only by value ids, so a range filter is the set of values whose text reads into a number in range.
//! Pure, no network.

use rvariant::si_dynamic::Quantity;
use rvariant::{NumberTy, Variant, VariantTy};
use serde::{Deserialize, Serialize};

/// One parameter filter: the parameter's name as the supplier writes it (matched ignoring case, a unique part of
/// it is enough) and what its value must be.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamFilter {
    pub name: String,
    pub op: FilterOp,
}

/// What a value must be. Thresholds and list items stay text: the matcher reads them with the value's unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterOp {
    /// Equal to one of these: as numbers within 0.1 % when both read as numbers, else as text ignoring case (a whole
    /// value, one item of a value's list, or the part before or inside its parentheses: `0402` is
    /// `0402 (1005 Metric)`)
    Eq(Vec<String>),
    /// Equal to none of these
    Ne(Vec<String>),
    /// Containing one of these texts, ignoring case
    Contains(Vec<String>),
    /// At least; a range value (`-55°C ~ 125°C`) by its upper end
    Ge(String),
    /// At most; a range value by its lower end, a tolerance (`±10%`) by its larger side
    Le(String),
    Gt(String),
    Lt(String),
    /// Between, both ends included; a range value must cover the whole range
    Between(String, String),
}

/// Relative tolerance of `=` between numbers (as BOM-20 compares values)
const EQ_REL: f64 = 1e-3;

impl ParamFilter {
    /// `Name OP value`: OP one of `>=` `<=` `!=` `=` `~` `>` `<`; `=`, `!=` and `~` take a list split by `,` or `|`;
    /// `=` with `a..b` is a range.
    pub fn parse(s: &str) -> Result<ParamFilter, String> {
        let at = s
            .find(['<', '>', '=', '!', '~'])
            .ok_or_else(|| format!("`{s}`: expected Name>=value (ops: = != ~ >= <= > <)"))?;
        let name = s[..at].trim();
        if name.is_empty() {
            return Err(format!("`{s}`: no parameter name before the operator"));
        }
        let rest = &s[at..];
        let (op, value) = ["<=", ">=", "!=", "=", "~", "<", ">"]
            .iter()
            .find_map(|op| rest.strip_prefix(op).map(|v| (*op, v.trim())))
            .ok_or_else(|| format!("`{s}`: unknown operator (= != ~ >= <= > <)"))?;
        if value.is_empty() {
            return Err(format!("`{s}`: no value after `{op}`"));
        }
        let list = || -> Vec<String> {
            value
                .split([',', '|'])
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .collect()
        };
        let op = match op {
            "=" => match value.split_once("..") {
                Some((lo, hi)) => FilterOp::Between(lo.trim().into(), hi.trim().into()),
                None => FilterOp::Eq(list()),
            },
            "!=" => FilterOp::Ne(list()),
            "~" => FilterOp::Contains(list()),
            ">=" => FilterOp::Ge(value.into()),
            "<=" => FilterOp::Le(value.into()),
            ">" => FilterOp::Gt(value.into()),
            "<" => FilterOp::Lt(value.into()),
            _ => unreachable!(),
        };
        Ok(ParamFilter {
            name: name.to_string(),
            op,
        })
    }

    /// Whether a value with this text passes; `None` when the text can't be compared (no number in the
    /// threshold's unit), so the caller can say how many values it left out.
    pub fn matches(&self, text: &str) -> Option<bool> {
        match &self.op {
            FilterOp::Eq(items) => Some(items.iter().any(|i| equal(i, text))),
            FilterOp::Ne(items) => Some(!items.iter().any(|i| equal(i, text))),
            FilterOp::Contains(items) => {
                let t = text.to_lowercase();
                Some(items.iter().any(|i| t.contains(&i.to_lowercase())))
            }
            FilterOp::Ge(x) => cmp(x, text, |v, x| v.hi >= x),
            FilterOp::Gt(x) => cmp(x, text, |v, x| v.hi > x),
            FilterOp::Le(x) => cmp(x, text, |v, x| v.lo_or_magnitude() <= x),
            FilterOp::Lt(x) => cmp(x, text, |v, x| v.lo_or_magnitude() < x),
            FilterOp::Between(a, b) => {
                let a = threshold_for(a, text)?;
                let b = threshold_for(b, text)?;
                let (a, b) = if a.1 <= b.1 { (a, b) } else { (b, a) };
                let v = read(text, &a.0)?;
                Some(if v.range {
                    v.lo <= a.1 && v.hi >= b.1
                } else {
                    v.lo >= a.1 - a.1.abs() * EQ_REL && v.hi <= b.1 + b.1.abs() * EQ_REL
                })
            }
        }
    }
}

impl std::fmt::Display for ParamFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = &self.name;
        match &self.op {
            FilterOp::Eq(l) => write!(f, "{n}={}", l.join(",")),
            FilterOp::Ne(l) => write!(f, "{n}!={}", l.join(",")),
            FilterOp::Contains(l) => write!(f, "{n}~{}", l.join(",")),
            FilterOp::Ge(x) => write!(f, "{n}>={x}"),
            FilterOp::Le(x) => write!(f, "{n}<={x}"),
            FilterOp::Gt(x) => write!(f, "{n}>{x}"),
            FilterOp::Lt(x) => write!(f, "{n}<{x}"),
            FilterOp::Between(a, b) => write!(f, "{n}={a}..{b}"),
        }
    }
}

/// A value read as numbers in the unit without prefix: a scalar has `lo == hi`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Read {
    lo: f64,
    hi: f64,
    /// A tolerance (`±10%`, `-20%, +80%`): compared by its larger side
    tolerance: bool,
    /// A range (`-55°C ~ 125°C`)
    range: bool,
}

impl Read {
    fn lo_or_magnitude(&self) -> f64 {
        if self.tolerance {
            self.lo.abs().max(self.hi.abs())
        } else {
            self.lo
        }
    }
}

/// The threshold's kind (a tolerance for `5%` or `±5%`, a number for a bare number, else an SI value of its unit)
/// and its value in that unit without prefix (a percentage as a fraction, as tolerances are read). `None` for text
/// that is no number or whose unit is no known unit (`1005 Metric`).
fn threshold(x: &str) -> Option<(VariantTy, f64)> {
    let x = ohms(x.trim());
    let x = x.as_str();
    let (plus_minus, x) = match x.strip_prefix('±') {
        Some(rest) => (true, rest.trim()),
        None => (false, x),
    };
    let q = Quantity::parse(x).ok()?;
    let value = q.as_f64().ok()?;
    let unit = q.unit.without_prefixes();
    unit.dim()?;
    let ty = if plus_minus || x.ends_with('%') {
        VariantTy::Tolerance(NumberTy::F64)
    } else if unit == rvariant::si_dynamic::UnitExpr::one() {
        VariantTy::Number(NumberTy::F64)
    } else {
        VariantTy::SI {
            number_ty: NumberTy::F64,
            unit,
        }
    };
    Some((ty, value))
}

/// [threshold] of `x`, or when `x` has no unit (`100n`, `25`, `4.7k`), with the unit of the value `text` (`0.1 µF`):
/// the value's unit without prefix appended.
fn threshold_for(x: &str, text: &str) -> Option<(VariantTy, f64)> {
    let t = threshold(x);
    if !matches!(t, None | Some((VariantTy::Number(_), _))) {
        return t;
    }
    let text = ohms(text);
    let head = text.split(['@', ',', '(', '~']).next().unwrap_or_default();
    let unit = Quantity::parse(head.trim()).ok()?.unit.without_prefixes();
    unit.dim()?;
    if unit == rvariant::si_dynamic::UnitExpr::one() {
        return t;
    }
    threshold(&format!("{}{unit}", x.trim())).or(t)
}

/// Ohms as Digi-Key writes them (`Ohms`, `mOhm`, `kOhms`) in the symbol the unit parser knows.
fn ohms(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if lower[i..].starts_with("ohms") {
            out.push('Ω');
            i += 4;
        } else if lower[i..].starts_with("ohm") {
            out.push('Ω');
            i += 3;
        } else {
            let c = s[i..].chars().next().unwrap_or_default();
            out.push(c);
            i += c.len_utf8().max(1);
        }
    }
    out
}

/// `text` read in the threshold's kind: a scalar, else a range of the same unit, else (for a percentage) a tolerance.
fn read(text: &str, ty: &VariantTy) -> Option<Read> {
    let scalar = |v: &Variant| {
        v.num_base_f64().map(|n| Read {
            lo: n,
            hi: n,
            tolerance: false,
            range: false,
        })
    };
    match ty {
        VariantTy::Tolerance(n) => {
            let v = Variant::parse_vendor(text, &VariantTy::Tolerance(*n))
                .ok()?
                .value;
            let (lo, hi) = v.bounds_base_f64()?;
            Some(Read {
                lo,
                hi,
                tolerance: true,
                range: false,
            })
        }
        VariantTy::SI { number_ty, unit } => {
            // `-55°C ~ 125°C` read as one value is its first end: a range only when it is written as one
            let ranged = text.contains('~') || text.contains(" to ");
            if !ranged
                && let Ok(v) = Variant::parse_vendor(text, ty)
                && let Some(r) = scalar(&v.value)
            {
                return Some(r);
            }
            let range = VariantTy::SIRange {
                number_ty: *number_ty,
                start_inclusive: true,
                end_inclusive: true,
                unit: unit.clone(),
            };
            let v = Variant::parse_vendor(text, &range).ok()?.value;
            if let Some(r) = scalar(&v) {
                return Some(r);
            }
            let (lo, hi) = v.bounds_base_f64()?;
            Some(Read {
                lo,
                hi,
                tolerance: false,
                range: true,
            })
        }
        _ => {
            let v = Variant::parse_vendor(text, ty).ok()?.value;
            scalar(&v)
        }
    }
}

fn cmp(x: &str, text: &str, f: impl Fn(Read, f64) -> bool) -> Option<bool> {
    let (ty, x) = threshold_for(x, text)?;
    let v = read(text, &ty)?;
    Some(f(v, x))
}

/// `item` (from the filter) equals the value `text`: as numbers when both read so in the item's unit, else as text.
fn equal(item: &str, text: &str) -> bool {
    if let Some((ty, x)) = threshold_for(item, text)
        && !matches!(ty, VariantTy::Number(_))
        && let Some(v) = read(text, &ty)
    {
        let near = |a: f64| (a - x).abs() <= x.abs() * EQ_REL;
        return if v.tolerance {
            near(v.lo_or_magnitude())
        } else {
            v.lo == v.hi && near(v.lo)
        };
    }
    let item = item.trim();
    let same = |t: &str| t.trim().eq_ignore_ascii_case(item);
    if same(text) || text.split([',', ';']).any(same) {
        return true;
    }
    // `0402 (1005 Metric)`: the part before the parentheses or inside them
    if let Some((head, tail)) = text.split_once('(') {
        return same(head) || same(tail.trim_end().trim_end_matches(')'));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(s: &str) -> ParamFilter {
        ParamFilter::parse(s).unwrap()
    }

    #[test]
    fn parses_ops_lists_and_ranges() {
        assert_eq!(
            f("Voltage - Rated>=25V"),
            ParamFilter {
                name: "Voltage - Rated".into(),
                op: FilterOp::Ge("25V".into())
            }
        );
        assert_eq!(
            f("Temperature Coefficient = X7R, X5R").op,
            FilterOp::Eq(vec!["X7R".into(), "X5R".into()])
        );
        assert_eq!(
            f("Resistance=1k..10k").op,
            FilterOp::Between("1k".into(), "10k".into())
        );
        assert_eq!(
            f("Package / Case~0402").op,
            FilterOp::Contains(vec!["0402".into()])
        );
        assert_eq!(
            f("Rds On (Max) @ Id, Vgs<=50mOhm").name,
            "Rds On (Max) @ Id, Vgs"
        );
        assert_eq!(
            f("Status!=Obsolete|NRND").op,
            FilterOp::Ne(vec!["Obsolete".into(), "NRND".into()])
        );
        assert!(ParamFilter::parse("Voltage").is_err());
        assert!(ParamFilter::parse(">=5V").is_err());
        assert!(ParamFilter::parse("Voltage>=").is_err());
        assert_eq!(
            f("Voltage - Rated>=25V").to_string(),
            "Voltage - Rated>=25V"
        );
    }

    #[test]
    fn voltage_at_least() {
        let v = f("Voltage - Rated>=25V");
        assert_eq!(v.matches("50V"), Some(true));
        assert_eq!(v.matches("25V"), Some(true));
        assert_eq!(v.matches("16V"), Some(false));
        assert_eq!(v.matches("1kV"), Some(true));
        assert_eq!(v.matches("6.3V"), Some(false));
        // another dimension can't be compared
        assert_eq!(v.matches("10A"), None);
    }

    #[test]
    fn capacitance_equal_and_between() {
        // a bare prefix takes the value's unit
        let c = f("Capacitance=100n");
        assert_eq!(c.matches("0.1 µF"), Some(true));
        assert_eq!(f("Voltage - Rated>=25").matches("50V"), Some(true));
        assert_eq!(f("Resistance=4.7k").matches("4.7 kOhms"), Some(true));
        let c = f("Capacitance=100nF");
        assert_eq!(c.matches("0.1 µF"), Some(true));
        assert_eq!(c.matches("100 nF"), Some(true));
        assert_eq!(c.matches("1 µF"), Some(false));
        let r = f("Capacitance=1uF..10uF");
        assert_eq!(r.matches("4.7 µF"), Some(true));
        assert_eq!(r.matches("10 µF"), Some(true));
        assert_eq!(r.matches("22 µF"), Some(false));
    }

    #[test]
    fn resistance_with_conditions() {
        let r = f("Rds On (Max) @ Id, Vgs<=50mOhm");
        assert_eq!(r.matches("45mOhm @ 4.5A, 10V"), Some(true));
        assert_eq!(r.matches("120mOhm @ 1A, 4.5V"), Some(false));
        assert_eq!(r.matches("1.2Ohm @ 100mA, 10V"), Some(false));
        let r = f("Resistance=10kOhms");
        assert_eq!(r.matches("10 kOhms"), Some(true));
        assert_eq!(r.matches("1 kOhms"), Some(false));
    }

    #[test]
    fn tolerance_at_most() {
        let t = f("Tolerance<=5%");
        assert_eq!(t.matches("±1%"), Some(true));
        assert_eq!(t.matches("±5%"), Some(true));
        assert_eq!(t.matches("±10%"), Some(false));
        assert_eq!(t.matches("-20%, +80%"), Some(false));
        let t = f("Tolerance=±1%");
        assert_eq!(t.matches("±1%"), Some(true));
        assert_eq!(t.matches("±5%"), Some(false));
    }

    #[test]
    fn temperature_range_by_its_ends() {
        let hot = f("Operating Temperature>=125°C");
        assert_eq!(hot.matches("-55°C ~ 125°C"), Some(true));
        assert_eq!(hot.matches("-40°C ~ 85°C"), Some(false));
        let cold = f("Operating Temperature<=-40°C");
        assert_eq!(cold.matches("-55°C ~ 125°C"), Some(true));
        assert_eq!(cold.matches("-25°C ~ 85°C"), Some(false));
        let span = f("Operating Temperature=-40°C..85°C");
        assert_eq!(span.matches("-55°C ~ 125°C"), Some(true));
        assert_eq!(span.matches("-25°C ~ 85°C"), Some(false));
    }

    #[test]
    fn text_values() {
        let d = f("Temperature Coefficient=C0G");
        assert_eq!(d.matches("C0G, NP0"), Some(true));
        assert_eq!(d.matches("X7R"), Some(false));
        let p = f("Package / Case=0402");
        assert_eq!(p.matches("0402 (1005 Metric)"), Some(true));
        assert_eq!(p.matches("0603 (1608 Metric)"), Some(false));
        let m = f("Package / Case=1005 Metric");
        assert_eq!(m.matches("0402 (1005 Metric)"), Some(true));
        let n = f("Mounting Type!=Through Hole");
        assert_eq!(n.matches("Surface Mount"), Some(true));
        assert_eq!(n.matches("Through Hole"), Some(false));
    }
}
