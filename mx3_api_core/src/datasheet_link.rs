//! `mx3:` links: a `KiCad` `Datasheet` field naming part numbers whose library documents to open (CORE-3).
//!
//! The field reads `mx3:${Mpn1} ${Mpn2}`: the scheme (any case), an optional `//`, then part numbers apart by white
//! space. Percent escapes (`%20`, `%2F`, `%23`) are decoded before the split. A word holding `${` is a reference
//! `KiCad` left unresolved and is dropped, as is `KiCad`'s empty-field `~`; each part number counts once, any case. A
//! part number may hold anything but white space (`LM358/NOPB`, `74HC00#PBF`, `X,REEL`).
//!
//! `KiCad`'s schematic editor fills the `${Field}` references before it opens the link; [`resolve`] fills them the
//! same way for a reader of the schematic file, which sees them raw.

use std::fmt;

/// How a link starts (any case).
pub const SCHEME: &str = "mx3:";
/// A longer link is no `Datasheet` field.
pub const LINK_BYTES_MAX: usize = 4096;
/// Part numbers in one link at most: as many as one lookup by part number takes.
pub const LINK_MPNS_MAX: usize = 2000;

/// The part numbers a link names.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Link {
    /// The part numbers, once each, in the link's order
    pub mpns: Vec<String>,
    /// Words dropped as references left unresolved (`${Mpn2}`)
    pub unresolved: usize,
}

/// Why a text is no link to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// It does not start with [`SCHEME`].
    NotALink,
    /// Over [`LINK_BYTES_MAX`] bytes.
    TooLong { bytes: usize },
    /// Over [`LINK_MPNS_MAX`] part numbers.
    TooManyMpns { count: usize },
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotALink => write!(f, "not an {SCHEME} link"),
            Self::TooLong { bytes } => {
                write!(
                    f,
                    "an {SCHEME} link of {bytes} bytes (at most {LINK_BYTES_MAX})"
                )
            }
            Self::TooManyMpns { count } => write!(
                f,
                "an {SCHEME} link of {count} part numbers (at most {LINK_MPNS_MAX})"
            ),
        }
    }
}

impl std::error::Error for LinkError {}

/// Whether a text (a `Datasheet` field, a URI) is an `mx3:` link.
pub fn is_link(text: &str) -> bool {
    text.trim()
        .get(..SCHEME.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(SCHEME))
}

/// Reads a link: its part numbers, by the rules of the module docs.
pub fn parse(text: &str) -> Result<Link, LinkError> {
    if text.len() > LINK_BYTES_MAX {
        return Err(LinkError::TooLong { bytes: text.len() });
    }
    let text = text.trim();
    if !is_link(text) {
        return Err(LinkError::NotALink);
    }
    let rest = &text[SCHEME.len()..];
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let mut link = Link::default();
    for w in percent_decode(rest).split_whitespace() {
        if w.contains("${") {
            link.unresolved += 1;
        } else if w != "~" && !link.mpns.iter().any(|m| m.eq_ignore_ascii_case(w)) {
            link.mpns.push(w.to_owned());
        }
    }
    counted(link)
}

/// The link, unless it names over [`LINK_MPNS_MAX`] part numbers.
fn counted(link: Link) -> Result<Link, LinkError> {
    if link.mpns.len() > LINK_MPNS_MAX {
        return Err(LinkError::TooManyMpns {
            count: link.mpns.len(),
        });
    }
    Ok(link)
}

/// A link with its `${Field}` references filled from `field`, as `KiCad`'s schematic editor fills them.
///
/// `field` maps a field's name (any case) to its value, used trimmed; a reference with no such field stays. `None`
/// when the text is no link or too long to be one.
pub fn resolve<'a>(text: &str, field: impl Fn(&str) -> Option<&'a str>) -> Option<String> {
    let text = text.trim();
    if !is_link(text) || text.len() > LINK_BYTES_MAX {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("${") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let Some(end) = after.find('}') else {
            rest = &rest[at..];
            break;
        };
        match field(&after[..end]) {
            Some(v) => out.push_str(v.trim()),
            None => out.push_str(&rest[at..=at + 2 + end]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// [`parse`] of the link [`resolve`] fills from `field`.
pub fn parse_resolved<'a>(
    text: &str,
    field: impl Fn(&str) -> Option<&'a str>,
) -> Result<Link, LinkError> {
    match resolve(text, field) {
        Some(r) => parse(&r),
        None => parse(text),
    }
}

/// `%XX` escapes as their bytes; a `%` without two hex digits after it stays.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        let hex = b
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        if let (b'%', Some(v)) = (c, hex) {
            out.push(v);
            i += 3;
        } else {
            out.push(c);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mpns(text: &str) -> Vec<String> {
        parse(text).unwrap().mpns
    }

    fn fields(name: &str) -> Option<&'static str> {
        match name.to_ascii_lowercase().as_str() {
            "mpn1" => Some("LM358/NOPB"),
            "mpn2" => Some(" 74HC00#PBF "),
            "empty" => Some(""),
            _ => None,
        }
    }

    #[test]
    fn a_link_s_part_numbers() {
        assert_eq!(mpns("mx3:TPS2116DRLR"), ["TPS2116DRLR"]);
        assert_eq!(
            mpns("MX3://TPS2116DRLR  CL05A105KQ5NNNC "),
            ["TPS2116DRLR", "CL05A105KQ5NNNC"]
        );
        // what part numbers hold stays: `/`, `#`, `,`, `+`
        assert_eq!(
            mpns("mx3:LM358/NOPB 74HC00#PBF X,REEL A+B"),
            ["LM358/NOPB", "74HC00#PBF", "X,REEL", "A+B"]
        );
        // escaped on the way, or by hand
        assert_eq!(mpns("mx3:LM358%2FNOPB%20A%23B"), ["LM358/NOPB", "A#B"]);
        // a % that is no escape stays
        assert_eq!(mpns("mx3:50%OFF %4"), ["50%OFF", "%4"]);
        // once each, any case
        assert_eq!(mpns("mx3:abc ABC"), ["abc"]);
    }

    #[test]
    fn unresolved_references_and_empty_fields_are_dropped() {
        let l = parse("mx3:${Mpn1} PART ${Mpn2} ~").unwrap();
        assert_eq!(l.mpns, ["PART"]);
        assert_eq!(l.unresolved, 2);
        assert_eq!(
            parse("mx3:${Mpn1}").unwrap(),
            Link {
                mpns: vec![],
                unresolved: 1
            }
        );
    }

    #[test]
    fn not_a_link() {
        assert!(is_link(" MX3:x"));
        assert!(!is_link("docs/mx3:x.pdf"));
        assert_eq!(parse("https://x/ds.pdf"), Err(LinkError::NotALink));
        assert_eq!(parse("mx"), Err(LinkError::NotALink));
        let long = format!("mx3:{}", "A".repeat(LINK_BYTES_MAX));
        assert_eq!(
            parse(&long),
            Err(LinkError::TooLong {
                bytes: LINK_BYTES_MAX + 4
            })
        );
    }

    #[test]
    fn part_numbers_over_the_lookup_s_limit() {
        let mut link = Link {
            mpns: (0..LINK_MPNS_MAX).map(|i| format!("P{i}")).collect(),
            unresolved: 0,
        };
        assert!(counted(link.clone()).is_ok());
        link.mpns.push("ONE-MORE".to_owned());
        assert_eq!(
            counted(link),
            Err(LinkError::TooManyMpns {
                count: LINK_MPNS_MAX + 1
            })
        );
    }

    #[test]
    fn references_are_filled_from_the_fields() {
        assert_eq!(
            resolve("mx3:${Mpn1} ${MPN2} ${Mpn3}", fields).as_deref(),
            Some("mx3:LM358/NOPB 74HC00#PBF ${Mpn3}")
        );
        assert_eq!(
            parse_resolved("mx3:${Mpn1} ${MPN2} ${Mpn3} ${Empty} ~", fields)
                .unwrap()
                .mpns,
            ["LM358/NOPB", "74HC00#PBF"]
        );
        assert_eq!(
            resolve("mx3:A ${Mpn1", fields).as_deref(),
            Some("mx3:A ${Mpn1")
        );
        assert_eq!(resolve("/lib/x.pdf", fields), None);
        assert_eq!(parse_resolved("~", fields), Err(LinkError::NotALink));
    }
}
