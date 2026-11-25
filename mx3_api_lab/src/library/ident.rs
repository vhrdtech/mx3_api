//! Which document a file is (mx3 LIB-1, from tpm's LIB-4; LAB-5).
//!
//! The vendor's document number, its revision and date, read from the text the index already extracted. One rule per vendor whose layout is clear; a rule only answers when its
//! pattern repeats the way that vendor prints it (a footer on every page, a header, a colophon) and the
//! vendor's name is in the text. Anything else stays unknown: never a guess from a file name or a number
//! the text merely mentions ("see AN2606").
//!
//! Dates are ISO (`2026-09`, `2021-08-04`); a revision is the vendor's own word (`4`, `B`, `3.20`, `orig` for
//! TI's unlettered original, a build id for Raspberry Pi).
//!
//! Where no vendor rule answers, or it leaves the revision or the date open, the rules every vendor shares fill
//! them (LIB-9): a revision and date that repeat on the pages (a footer), the newest row of the document's
//! revision history table, a revision and date on the cover, and last the PDF's own date (Info `ModDate`, else
//! `CreationDate`), a weaker guess. Each value keeps where it came from ([`Src`]); still never a file name.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Bumped when a rule changes: rows parsed by an older one are parsed again from the stored text.
pub const VERSION: i64 = 2;

/// Pages read: footers repeat, so the first few are enough and a 2,000-page manual costs no more.
pub const PAGES: usize = 12;
/// And the last pages, where most revision history tables are.
pub const TAIL: usize = 8;

/// Where a revision or a date was read, strongest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Src {
    /// Repeated on the pages (a footer or a header), or a vendor rule's own place.
    Footer,
    /// The newest row of the revision history table.
    History,
    /// Once, on the first pages.
    Cover,
    /// The PDF's own date (Info dictionary): when the file was made, a guess at the release.
    PdfMeta,
}

impl Src {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Footer => "footer",
            Self::History => "history",
            Self::Cover => "cover",
            Self::PdfMeta => "pdfmeta",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Footer, Self::History, Self::Cover, Self::PdfMeta]
            .into_iter()
            .find(|x| x.as_str() == s)
    }

    /// A guess, shown with `~`.
    pub fn weak(self) -> bool {
        self == Self::PdfMeta
    }
}

/// The document a file is: the vendor's number, revision and date where its text says them clearly.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ident {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev_src: Option<Src>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_src: Option<Src>,
}

/// What a PDF says of itself (`pdfinfo -isodates`): its dates, ISO days.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    pub modified: Option<String>,
    pub created: Option<String>,
}

impl Meta {
    /// From `pdfinfo -isodates` output.
    pub fn from_pdfinfo(info: &str) -> Self {
        static D: LazyLock<Regex> =
            LazyLock::new(|| re(r"(?m)^(ModDate|CreationDate):\s*(\d{4}-\d{2}-\d{2})"));
        let mut m = Self::default();
        for c in D.captures_iter(info) {
            let day = Some(c[2].to_string()).filter(|d| sane_year(&d[..4]));
            if &c[1] == "ModDate" {
                m.modified = day;
            } else {
                m.created = day;
            }
        }
        m
    }
}

impl Ident {
    pub fn is_empty(&self) -> bool {
        self.doc.is_none() && self.rev.is_none() && self.date.is_none()
    }

    /// The date people read, `~` before a guess: `Dec 2024`, `~Mar 2023`.
    pub fn date_label(&self) -> Option<String> {
        let d = human_date(self.date.as_deref()?);
        Some(if self.date_src.is_some_and(Src::weak) {
            format!("~{d}")
        } else {
            d
        })
    }

    /// `RM0444 Rev 5 (Nov 2020)`, the parts that are known.
    pub fn label(&self) -> String {
        let mut s = self.doc.clone().unwrap_or_default();
        if let Some(r) = &self.rev {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str("Rev ");
            s.push_str(r);
        }
        if let Some(d) = self.date_label() {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push('(');
            s.push_str(&d);
            s.push(')');
        }
        s
    }
}

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

fn re(s: &str) -> Regex {
    Regex::new(s).expect("static regex")
}

const MONTH_RX: &str =
    "(?:January|February|March|April|May|June|July|August|September|October|November|December)";

fn month_no(name: &str) -> Option<u32> {
    let n = name.to_lowercase();
    MONTHS
        .iter()
        .position(|m| n.len() >= 3 && m.starts_with(&n))
        .map(|i| i as u32 + 1)
}

/// `September 2026` -> `2026-09`.
fn month_year(s: &str) -> Option<String> {
    let (m, y) = s.trim().rsplit_once(' ')?;
    let y: u32 = y.parse().ok()?;
    Some(format!("{y:04}-{:02}", month_no(m)?))
}

/// `4 August 2021`, `9-Aug-2023` -> `2021-08-04`.
fn day_month_year(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split([' ', '-']).filter(|p| !p.is_empty()).collect();
    let [d, m, y] = parts[..] else { return None };
    let (d, y): (u32, u32) = (d.parse().ok()?, y.parse().ok()?);
    (1..=31)
        .contains(&d)
        .then(|| Some(format!("{y:04}-{:02}-{d:02}", month_no(m)?)))
        .flatten()
}

/// `06/2021` -> `2021-06`.
fn slash_month(s: &str) -> Option<String> {
    let (m, y) = s.split_once('/')?;
    let (m, y): (u32, u32) = (m.parse().ok()?, y.parse().ok()?);
    ((1..=12).contains(&m) && y > 1900).then(|| format!("{y:04}-{m:02}"))
}

/// `2026-09` -> `Sep 2026`, `2021-08-04` -> `4 Aug 2021` (the date people read).
pub fn human_date(iso: &str) -> String {
    let p: Vec<&str> = iso.split('-').collect();
    let mon = |m: &str| {
        m.parse::<usize>()
            .ok()
            .and_then(|m| MONTHS.get(m.wrapping_sub(1)))
            .map(|n| {
                let mut c = n[..3].to_string();
                c[..1].make_ascii_uppercase();
                c
            })
    };
    match p[..] {
        [y, m] => mon(m).map(|m| format!("{m} {y}")),
        [y, m, d] => mon(m).map(|m| format!("{} {m} {y}", d.trim_start_matches('0'))),
        _ => None,
    }
    .unwrap_or_else(|| iso.to_owned())
}

/// The most common key with at least `min` hits; a tie between two different keys is no answer.
fn winner<K: Clone + Eq + std::hash::Hash + Ord>(hits: &[K], min: usize) -> Option<K> {
    let mut n: HashMap<&K, usize> = HashMap::new();
    for h in hits {
        *n.entry(h).or_default() += 1;
    }
    let mut v: Vec<(&K, usize)> = n.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let (k, c) = *v.first()?;
    if c < min || v.get(1).is_some_and(|x| x.1 == c) {
        return None;
    }
    Some(k.clone())
}

/// A vendor's rule: (the first pages, whitespace collapsed; all of them joined; the first page as it is; how
/// often a footer must repeat).
type Rule = fn(&[String], &str, &str, usize) -> Option<Ident>;

/// The identity of a document from its pages' text (the tests' short form of `identify`).
#[cfg(test)]
pub fn parse(pages: &[String]) -> Ident {
    identify(pages, None).0
}

/// Whitespace collapsed.
fn flat_of(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The identity of a document and the vendor whose document it is.
///
/// From its pages' text (the first [`PAGES`] and the last [`TAIL`] are read) and what the PDF says of itself; the vendor is the vendor rule's, else the maker the
/// first pages name most (a folder name of the datasheets share: `ST`, `TI`, `Nordic`).
pub fn identify(pages: &[String], meta: Option<&Meta>) -> (Ident, Option<&'static str>) {
    let head: Vec<String> = pages.iter().take(PAGES).map(|p| flat_of(p)).collect();
    let flat = head.join(" ");
    // a short document prints its footer once or twice; a long one on every page
    let min = if pages.len() <= 2 { 1 } else { 2 };
    let rules: [(&str, Rule); 7] = [
        ("ST", st),
        ("TI", ti),
        ("Microchip", microchip),
        ("NXP", nxp),
        ("Analog", analog),
        ("Renesas", renesas),
        ("Raspberry Pi", raspberry),
    ];
    let first = pages.first().map(String::as_str).unwrap_or("");
    let (mut id, vendor) = rules
        .iter()
        .find_map(|(v, r)| r(&head, &flat, first, min).map(|i| (i, Some(*v))))
        .unwrap_or_default();
    generic(&mut id, pages, &head);
    if id.date.is_none()
        && let Some(d) = meta.and_then(|m| m.modified.as_ref().or(m.created.as_ref()))
    {
        id.date = d.get(..7).map(String::from);
        id.date_src = Some(Src::PdfMeta);
    }
    (id, vendor.or_else(|| maker(&flat)))
}

impl Ident {
    /// A date only goes with a revision it was read with: none read, none kept.
    fn settled(mut self) -> Self {
        if self.date.is_none() {
            self.date_src = None;
        }
        if self.rev.is_none() {
            self.rev_src = None;
        }
        self
    }

    /// Takes what a rule read where it is still open; a revision other than the one known means the date is
    /// another revision's and is not taken.
    fn fill(&mut self, rev: Option<String>, date: Option<String>, src: Src) {
        if let Some(r) = rev {
            match &self.rev {
                None => {
                    self.rev = Some(r);
                    self.rev_src = Some(src);
                }
                Some(have) if !same_rev(have, &r) => return,
                _ => {}
            }
        }
        if self.date.is_none()
            && let Some(d) = date
        {
            self.date = Some(d);
            self.date_src = Some(src);
        }
    }
}

/// `5.0` and `v.5.0`, `B` and `b`: one revision.
fn same_rev(a: &str, b: &str) -> bool {
    norm_rev(a).eq_ignore_ascii_case(&norm_rev(b))
}

/// A revision without its marker: `v.5.0` -> `5.0`, `V1.03` -> `1.03`.
fn norm_rev(r: &str) -> String {
    let r = r.trim();
    let bare = r
        .strip_prefix(['v', 'V'])
        .map(|x| x.trim_start_matches('.'))
        .filter(|x| x.starts_with(|c: char| c.is_ascii_digit()));
    bare.unwrap_or(r).to_owned()
}

const MON: &str = r"jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?";

fn sane_year(y: &str) -> bool {
    y.parse::<u32>().is_ok_and(|y| (1980..=2099).contains(&y))
}

/// A date found in a text: where, ISO, and whether it is a loose form (`3/23`, `20230526`) that only a revision
/// history table's column makes a date.
struct Found {
    start: usize,
    end: usize,
    iso: String,
    loose: bool,
}

/// Every date of a text, in the forms vendors print them.
fn dates(s: &str) -> Vec<Found> {
    static RX: LazyLock<Regex> = LazyLock::new(|| {
        re(&format!(
            r"(?i)\b(?:(?P<d1>\d{{1,2}})[\s.-]*(?P<m1>{MON})\.?[\s,.-]*(?P<y1>\d{{4}})|(?P<m2>{MON})\.?\s+(?P<d2>\d{{1,2}}),?\s+(?P<y2>\d{{4}})|(?P<m3>{MON})\.?,?\s+(?P<y3>\d{{4}})|(?P<y4>\d{{4}})-(?P<mo4>\d{{2}})-(?P<d4>\d{{2}})|(?P<y5>\d{{4}})-(?P<m5>{MON})-(?P<d5>\d{{1,2}})|(?P<mo6>\d{{1,2}})/(?P<y6>\d{{4}})|(?P<mo7>\d{{1,2}})/(?P<y7>\d{{2}})|(?P<y8>(?:19|20)\d{{2}})(?P<mo8>[01]\d)(?P<d8>[0-3]\d))\b"
        ))
    });
    let mut out = vec![];
    for c in RX.captures_iter(s) {
        let g = |n: &str| c.name(n).map(|m| m.as_str());
        let num = |n: &str| g(n).and_then(|v| v.parse::<u32>().ok());
        let mon = |n: &str| g(n).and_then(month_no);
        let (y, m, d, loose): (Option<String>, Option<u32>, Option<u32>, bool) =
            if g("y1").is_some() {
                (g("y1").map(String::from), mon("m1"), num("d1"), false)
            } else if g("y2").is_some() {
                (g("y2").map(String::from), mon("m2"), num("d2"), false)
            } else if g("y3").is_some() {
                (g("y3").map(String::from), mon("m3"), None, false)
            } else if g("y4").is_some() {
                (g("y4").map(String::from), num("mo4"), num("d4"), false)
            } else if g("y5").is_some() {
                (g("y5").map(String::from), mon("m5"), num("d5"), false)
            } else if g("y6").is_some() {
                (g("y6").map(String::from), num("mo6"), None, false)
            } else if g("y7").is_some() {
                (g("y7").map(|y| format!("20{y}")), num("mo7"), None, true)
            } else {
                (g("y8").map(String::from), num("mo8"), num("d8"), true)
            };
        let (Some(y), Some(m)) = (y, m) else { continue };
        if !sane_year(&y) || !(1..=12).contains(&m) || d.is_some_and(|d| !(1..=31).contains(&d)) {
            continue;
        }
        let whole = c.get(0).expect("match");
        out.push(Found {
            start: whole.start(),
            end: whole.end(),
            iso: match d {
                Some(d) => format!("{y}-{m:02}-{d:02}"),
                None => format!("{y}-{m:02}"),
            },
            loose,
        });
    }
    out
}

/// `Rev. 3`, `Revision C`, `Version 1.2`, `Ver.1.9`, `v.5.0`: the revision a marker names, without the marker.
/// `Rev. A to Rev. B` names B.
static REV: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"\b(?:(?i:rev(?:ision)?|ver(?:sion)?)\.?\s?|v\.?)(?:(?:[vV]\.?\s?)?(?P<n>\d{1,3}(?:\.\d{1,3}){0,2})|(?P<l>[A-Z]{1,2}\d?))\b(?:\s+to\s+(?i:rev(?:ision)?)\.?\s?(?P<to>\d{1,3}(?:\.\d{1,3}){0,2}|[A-Z]{1,2}\d?)\b)?",
    )
});

struct RevAt {
    start: usize,
    end: usize,
    rev: String,
}

fn revs(s: &str) -> Vec<RevAt> {
    REV.captures_iter(s)
        .filter_map(|c| {
            let v = c.name("to").or(c.name("n")).or(c.name("l"))?;
            let whole = c.get(0)?;
            // a lone `v` marks a version only before a number: `v` is a word of its own too
            let marker = &s[whole.start()..];
            if (marker.starts_with('v') || marker.starts_with('V'))
                && !marker.to_lowercase().starts_with("ver")
                && c.name("n").is_none()
            {
                return None;
            }
            Some(RevAt {
                start: whole.start(),
                end: whole.end(),
                rev: v.as_str().to_owned(),
            })
        })
        .collect()
}

/// The revisions not right after a document number (`RM0444 Rev 5` names another document's).
fn own_revs(s: &str) -> impl Iterator<Item = RevAt> + '_ {
    static NO: LazyLock<Regex> = LazyLock::new(|| re(r"\b[A-Z]{2,}\d{3,}[A-Z]?\s*[-–—]?\s*$"));
    revs(s)
        .into_iter()
        .filter(move |r| !NO.is_match(&s[..r.start]))
}

/// Revision and date printed together (`Rev. 1 — 24 April 2023`, `30-May 2018 Ver.1.9`): at most 40 characters
/// apart, nothing that is another revision or date between them.
fn pairs_in(s: &str) -> Vec<(String, String)> {
    let ds: Vec<Found> = dates(s).into_iter().filter(|d| !d.loose).collect();
    let rs = revs(s);
    let mut out = vec![];
    for r in &rs {
        let best = ds
            .iter()
            .filter_map(|d| {
                let (a, b) = if d.start >= r.end {
                    (r.end, d.start)
                } else if r.start >= d.end {
                    (d.end, r.start)
                } else {
                    return None;
                };
                let between = |x: usize, y: usize| x >= a && y <= b;
                let clear = !rs.iter().any(|o| between(o.start, o.end))
                    && !ds.iter().any(|o| between(o.start, o.end));
                (b - a <= 40 && clear).then_some((b - a, d))
            })
            .min_by_key(|(gap, _)| *gap);
        if let Some((_, d)) = best {
            out.push((norm_rev(&r.rev), d.iso.clone()));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The rows of a revision history table: (revision, date). The table is what follows a heading that is not a
/// table of contents line; a row is a date with the revision nearest to it in its own stretch of text (a marked
/// one, else a short number or letter cell right before or after it). Loose dates (`3/23`) count only in a table
/// that has no other.
fn history(pages: &[&str]) -> Vec<(Option<String>, String)> {
    static HEAD: LazyLock<Regex> = LazyLock::new(|| {
        re(
            r"(?i)\b(?:document |change )?(?:revision|change|document) history\b|\brevision record\b",
        )
    });
    static LEADER: LazyLock<Regex> =
        LazyLock::new(|| re(r"^\s*(?:\.\s?){3,}|^\s*\d+\s+\d+(?:\.\d+)+\s"));
    static CELL: LazyLock<Regex> =
        LazyLock::new(|| re(r"^(?:[vV]\.?)?(?:\d{1,2}(?:\.\d{1,2}){0,2}|[A-Z]{1,2}\d?)$"));
    let mut rows = vec![];
    for page in pages {
        for h in HEAD.find_iter(page) {
            let rest = &page[h.end()..];
            if LEADER.is_match(rest) {
                continue;
            }
            let mut end = rest.len().min(1500);
            while !rest.is_char_boundary(end) {
                end -= 1;
            }
            let w = &rest[..end];
            let all = dates(w);
            let strict = all.iter().any(|d| !d.loose);
            let ds: Vec<&Found> = all.iter().filter(|d| d.loose != strict).collect();
            let rs = revs(w);
            for (i, d) in ds.iter().enumerate() {
                let lo = if i == 0 { 0 } else { ds[i - 1].end };
                let hi = ds.get(i + 1).map(|n| n.start).unwrap_or(w.len());
                let marked = rs
                    .iter()
                    .filter(|r| r.start >= lo && r.end <= hi)
                    .min_by_key(|r| {
                        if r.start >= d.end {
                            r.start - d.end
                        } else {
                            d.start.saturating_sub(r.end)
                        }
                    })
                    .map(|r| r.rev.clone());
                let cell = || {
                    let before: Vec<&str> =
                        w[lo..d.start].split_whitespace().rev().take(3).collect();
                    let after: Vec<&str> = w[d.end..hi].split_whitespace().take(2).collect();
                    before
                        .into_iter()
                        .chain(after)
                        .find(|t| CELL.is_match(t))
                        .map(String::from)
                };
                rows.push((marked.or_else(cell).map(|r| norm_rev(&r)), d.iso.clone()));
            }
        }
    }
    rows
}

/// The rules every vendor shares, each filling what is still open: footer, history table, cover.
fn generic(id: &mut Ident, pages: &[String], head: &[String]) {
    let n = pages.len();
    if n > 2 {
        // a footer: the same pair on two or more pages
        let mut hits = vec![];
        for p in head {
            hits.extend(pairs_in(p));
        }
        if let Some((r, d)) = winner(&hits, 2) {
            id.fill(Some(r), Some(d), Src::Footer);
        }
        // a revision or a date alone on three or more pages; `RM0444 Rev 5` is another document's
        if id.rev.is_none() {
            let mut hits = vec![];
            for p in head {
                let mut v: Vec<String> = own_revs(p).map(|r| norm_rev(&r.rev)).collect();
                v.sort();
                v.dedup();
                hits.extend(v);
            }
            if let Some(r) = winner(&hits, 3) {
                id.fill(Some(r), None, Src::Footer);
            }
        }
        if id.date.is_none() {
            let mut hits = vec![];
            for p in head {
                let mut v: Vec<String> = dates(p)
                    .into_iter()
                    .filter(|d| !d.loose)
                    .map(|d| d.iso)
                    .collect();
                v.sort();
                v.dedup();
                hits.extend(v);
            }
            if let Some(d) = winner(&hits, 3) {
                id.fill(None, Some(d), Src::Footer);
            }
        }
    }
    if id.rev.is_none() || id.date.is_none() {
        let tail = n.saturating_sub(TAIL).max(head.len());
        let flats: Vec<String> = pages[tail..].iter().map(|p| flat_of(p)).collect();
        let read: Vec<&str> = head.iter().chain(&flats).map(String::as_str).collect();
        let rows = history(&read);
        let newest = |rows: &mut dyn Iterator<Item = &(Option<String>, String)>| {
            rows.max_by(|a, b| a.1.get(..7).cmp(&b.1.get(..7)).then(a.1.cmp(&b.1)))
                .cloned()
        };
        let pick = match (&id.rev, &id.date) {
            (Some(r), _) => newest(
                &mut rows
                    .iter()
                    .filter(|x| x.0.as_deref().is_some_and(|x| same_rev(x, r))),
            ),
            (None, Some(d)) => newest(&mut rows.iter().filter(|x| &x.1 == d)),
            (None, None) => newest(&mut rows.iter()),
        };
        if let Some((r, d)) = pick {
            id.fill(r, Some(d), Src::History);
        }
    }
    if id.rev.is_none() || id.date.is_none() {
        cover(id, pages);
    }
}

/// The first two pages: a revision with its date, `Date: 2014-08-15`, the one marked revision, a line that is
/// only a date (the newest of them).
fn cover(id: &mut Ident, pages: &[String]) {
    static DATE_LABEL: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(?:release )?date:\s*"));
    static DOC_V: LazyLock<Regex> = LazyLock::new(|| {
        re(
            r"(?i)\b(?:specification|data ?sheet|manual|guide|note)\s+(?:v\.?\s?|version\s)(\d{1,3}(?:\.\d{1,3}){0,2})\b",
        )
    });
    let firsts: Vec<String> = pages.iter().take(2).map(|p| flat_of(p)).collect();
    let mut pairs = vec![];
    for p in &firsts {
        pairs.extend(pairs_in(p));
    }
    if let Some((r, d)) = pairs.first() {
        id.fill(Some(r.clone()), Some(d.clone()), Src::Cover);
    }
    for p in &firsts {
        for m in DATE_LABEL.find_iter(p) {
            if let Some(d) = dates(&p[m.end()..])
                .into_iter()
                .find(|d| d.start == 0 && !d.loose)
            {
                id.fill(None, Some(d.iso), Src::Cover);
            }
        }
    }
    let Some(first) = pages.first() else { return };
    let f = flat_of(first);
    if id.rev.is_none() {
        let mut v: Vec<String> = own_revs(&f)
            .filter(|r| {
                !f[r.start..].starts_with(['v', 'V'])
                    || f[r.start..].to_lowercase().starts_with("ver")
            })
            .map(|r| norm_rev(&r.rev))
            .chain(DOC_V.captures_iter(&f).map(|c| c[1].to_string()))
            .collect();
        v.sort();
        v.dedup();
        if let [r] = &v[..] {
            id.fill(Some(r.clone()), None, Src::Cover);
        }
    }
    if id.date.is_none() {
        let lines: Vec<String> = first
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .filter_map(|l| {
                let d = dates(l);
                match &d[..] {
                    [x] if !x.loose
                        && x.start == 0
                        && l[x.end..].trim_matches(['.', ',']).trim().is_empty() =>
                    {
                        Some(x.iso.clone())
                    }
                    _ => None,
                }
            })
            .collect();
        if let Some(d) = lines
            .into_iter()
            .max_by(|a, b| a.get(..7).cmp(&b.get(..7)).then(a.cmp(b)))
        {
            id.fill(None, Some(d), Src::Cover);
        }
    }
}

/// The maker whose name the first pages print most, at least twice and more than any other: the folder its
/// files go to below the datasheets share.
fn maker(flat: &str) -> Option<&'static str> {
    static MAKERS: LazyLock<Vec<(&str, Regex)>> = LazyLock::new(|| {
        [
            ("ST", r"STMicroelectronics|www\.st\.com"),
            ("TI", r"Texas Instruments|www\.ti\.com"),
            ("NXP", r"\bNXP\b"),
            ("Microchip", r"\bMicrochip\b"),
            ("Analog", r"Analog Devices|\banalog\.com\b"),
            ("Renesas", r"\bRenesas\b"),
            ("Raspberry Pi", r"\bRaspberry Pi\b"),
            ("Nordic", r"Nordic Semiconductor"),
            ("Infineon", r"\bInfineon\b"),
            ("Espressif", r"\bEspressif\b"),
            ("Trinamic", r"(?i)\bTrinamic\b"),
            ("onsemi", r"\bonsemi\b|ON Semiconductor"),
            ("Murata", r"\bMurata\b"),
            ("Bosch", r"Bosch Sensortec"),
            ("Semtech", r"\bSemtech\b"),
            ("Quectel", r"\bQuectel\b"),
            ("Vicor", r"\bVicor\b"),
            ("Cypress", r"\bCypress\b"),
        ]
        .into_iter()
        .map(|(k, r)| (k, re(r)))
        .collect()
    });
    let mut n: Vec<(usize, &str)> = MAKERS
        .iter()
        .map(|(k, r)| (r.find_iter(flat).count(), *k))
        .filter(|(c, _)| *c >= 2)
        .collect();
    n.sort_by_key(|a| std::cmp::Reverse(a.0));
    match &n[..] {
        [] => None,
        [(_, k)] => Some(k),
        [(a, k), (b, _), ..] => (a > b).then_some(*k),
    }
}

/// ST: `RM0444 Rev 5` in every footer, newer ones `ES0548 - Rev 4 - September 2026`; the date otherwise
/// stands alone on the cover page (`November 2020`).
fn st(_: &[String], flat: &str, first: &str, min: usize) -> Option<Ident> {
    static FOOT: LazyLock<Regex> = LazyLock::new(|| {
        re(&format!(
            r"\b((?:RM|ES|DS|AN|UM|TN|PM|DB)\d{{4,5}})(?: -)? Rev (\d+)(?: - ({MONTH_RX} \d{{4}}))?\b"
        ))
    });
    static COVER: LazyLock<Regex> =
        LazyLock::new(|| re(&format!(r"(?m)^\s*({MONTH_RX} \d{{4}})\s*$")));
    if !(flat.contains("STMicroelectronics") || flat.contains("www.st.com")) {
        return None;
    }
    let hits: Vec<(String, String)> = FOOT
        .captures_iter(flat)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();
    let (doc, rev) = winner(&hits, min)?;
    let dated: Vec<String> = FOOT
        .captures_iter(flat)
        .filter(|c| c[1] == doc && c[2] == rev)
        .filter_map(|c| c.get(3).and_then(|m| month_year(m.as_str())))
        .collect();
    let (date, date_src) = if let Some(d) = winner(&dated, 1) {
        (Some(d), Some(Src::Footer))
    } else {
        let d = COVER.captures(first).and_then(|c| month_year(&c[1]));
        let src = d.as_ref().map(|_| Src::Cover);
        (d, src)
    };
    Some(Ident {
        doc: Some(doc),
        rev: Some(rev),
        date,
        rev_src: Some(Src::Footer),
        date_src,
    })
}

/// TI: the literature number heads every page, `SLUSAT1A – MARCH 2013 – REVISED NOVEMBER 2015`; its last
/// letter is the revision (none: the original), the date the revised one.
fn ti(_: &[String], flat: &str, _: &str, min: usize) -> Option<Ident> {
    static HEAD: LazyLock<Regex> = LazyLock::new(|| {
        let m = MONTH_RX.to_uppercase();
        re(&format!(
            r"\b(S[A-Z]{{3}}[0-9A-Z]{{3}})([A-Z]?) [–-] ({m} \d{{4}})(?: [–-] REVISED ({m} \d{{4}}))?"
        ))
    });
    if !flat.contains("Texas Instruments") {
        return None;
    }
    let hits: Vec<(String, String, String)> = HEAD
        .captures_iter(flat)
        .map(|c| {
            let date = c
                .get(4)
                .or(c.get(3))
                .map(|m| m.as_str())
                .unwrap_or_default();
            (c[1].to_string(), c[2].to_string(), date.to_owned())
        })
        .collect();
    let (doc, rev, date) = winner(&hits, min)?;
    Some(
        Ident {
            doc: Some(doc),
            rev: Some(if rev.is_empty() { "orig".into() } else { rev }),
            date: month_year(&date),
            rev_src: Some(Src::Footer),
            date_src: Some(Src::Footer),
        }
        .settled(),
    )
}

/// Microchip: `DS20005678B-page 3` on every page: number and revision letter.
fn microchip(_: &[String], flat: &str, _: &str, min: usize) -> Option<Ident> {
    static FOOT: LazyLock<Regex> = LazyLock::new(|| re(r"\b(DS\d{5,8})([A-Z])-page \d+"));
    if !flat.contains("Microchip") {
        return None;
    }
    let hits: Vec<(String, String)> = FOOT
        .captures_iter(flat)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();
    let (doc, rev) = winner(&hits, min)?;
    Some(Ident {
        doc: Some(doc),
        rev: Some(rev),
        rev_src: Some(Src::Footer),
        ..Ident::default()
    })
}

/// NXP: `Rev. 1 — 4 August 2021` or `Rev. B, 06/2021` on every page; the number from `Document Number:`, or
/// the id that heads every page above `NXP Semiconductors` (`AN13698`, a data sheet's `PCA9451A`).
fn nxp(head: &[String], flat: &str, _: &str, min: usize) -> Option<Ident> {
    static REV: LazyLock<Regex> = LazyLock::new(|| {
        re(&format!(
            r"\bRev\. (\d{{1,3}}(?:\.\d+)?|[A-Z])(?:,| —| -) (\d{{1,2}} {MONTH_RX} \d{{4}}|\d{{2}}/\d{{4}})"
        ))
    });
    static TOP: LazyLock<Regex> =
        LazyLock::new(|| re(r"^([A-Z][A-Z0-9]{3,}) NXP Semiconductors\b"));
    static NUM: LazyLock<Regex> = LazyLock::new(|| re(r"\bDocument Number: ([A-Z][A-Z0-9]{3,})\b"));
    if !flat.contains("NXP") {
        return None;
    }
    let hits: Vec<(String, String)> = REV
        .captures_iter(flat)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();
    let (rev, date) = winner(&hits, min)?;
    let nums: Vec<String> = NUM.captures_iter(flat).map(|c| c[1].to_string()).collect();
    let tops: Vec<String> = head
        .iter()
        .filter_map(|p| TOP.captures(p).map(|c| c[1].to_string()))
        .collect();
    Some(
        Ident {
            doc: winner(&nums, 1).or_else(|| winner(&tops, min)),
            rev: Some(rev),
            date: slash_month(&date).or_else(|| day_month_year(&date)),
            rev_src: Some(Src::Footer),
            date_src: Some(Src::Footer),
        }
        .settled(),
    )
}

/// Analog Devices: `Rev. B | Page 2 of 21` on every page; the date from the revision history's line that
/// leads to this revision (`9/2019—Rev. A to Rev. B`). No document number: ADI names a data sheet by part.
fn analog(_: &[String], flat: &str, _: &str, min: usize) -> Option<Ident> {
    static FOOT: LazyLock<Regex> =
        LazyLock::new(|| re(r"\bRev\. ([0-9A-Z]{1,3}) \| (?:Page )?\d+ of \d+"));
    static HIST: LazyLock<Regex> = LazyLock::new(|| {
        re(
            r"\b(\d{1,2}/\d{4})\s?—\s?Rev(?:ision|\.)? (?:[0-9A-Z]{1,3} to Rev\. )?([0-9A-Z]{1,3})\b",
        )
    });
    if !flat.contains("Analog Devices") {
        return None;
    }
    let hits: Vec<String> = FOOT.captures_iter(flat).map(|c| c[1].to_string()).collect();
    let rev = winner(&hits, min)?;
    let date = HIST
        .captures_iter(flat)
        .find(|c| c[2] == rev)
        .and_then(|c| slash_month(&c[1]));
    Some(
        Ident {
            doc: None,
            rev: Some(rev),
            date,
            rev_src: Some(Src::Footer),
            date_src: Some(Src::History),
        }
        .settled(),
    )
}

/// Renesas (Dialog GreenPAK): `Revision 3.20 30-Nov-2023` in every footer.
fn renesas(_: &[String], flat: &str, _: &str, min: usize) -> Option<Ident> {
    static FOOT: LazyLock<Regex> =
        LazyLock::new(|| re(r"\bRevision (\d+(?:\.\d+)?) (\d{1,2}-[A-Z][a-z]{2}-\d{4})\b"));
    if !flat.contains("Renesas") {
        return None;
    }
    let hits: Vec<(String, String)> = FOOT
        .captures_iter(flat)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();
    let (rev, date) = winner(&hits, min)?;
    Some(
        Ident {
            doc: None,
            rev: Some(rev),
            date: day_month_year(&date),
            rev_src: Some(Src::Footer),
            date_src: Some(Src::Footer),
        }
        .settled(),
    )
}

/// Raspberry Pi: the colophon's `Build date: 2024-08-08` and `Build version: 1ca8b26-clean`, the title
/// `RP2350 Datasheet`.
fn raspberry(_: &[String], flat: &str, _: &str, _: usize) -> Option<Ident> {
    static DATE: LazyLock<Regex> = LazyLock::new(|| re(r"\bBuild date: (\d{4}-\d{2}-\d{2})\b"));
    static BUILD: LazyLock<Regex> =
        LazyLock::new(|| re(r"\bBuild version: ([0-9a-f]{7,40}(?:-clean|-dirty)?)\b"));
    static TITLE: LazyLock<Regex> =
        LazyLock::new(|| re(r"\b(RP\d{4}|RP1) (Datasheet|Product Brief)\b"));
    if !flat.contains("Raspberry Pi") {
        return None;
    }
    let date = DATE.captures(flat).map(|c| c[1].to_string())?;
    let rev = BUILD.captures(flat).map(|c| c[1].to_string())?;
    let titles: Vec<String> = TITLE
        .captures_iter(flat)
        .map(|c| format!("{} {}", &c[1], &c[2]))
        .collect();
    Some(Ident {
        doc: winner(&titles, 1),
        rev: Some(rev),
        date: Some(date),
        rev_src: Some(Src::Cover),
        date_src: Some(Src::Cover),
    })
}

/// Which of two revisions of one document is newer.
///
/// By date where both have one (to the month, then the
/// day), else by the revision word (numbers and dotted numbers by value, letters by letter, TI's `orig`
/// first). `None`: no telling (a build id without a date, mixed forms).
pub fn newer(a: &Ident, b: &Ident) -> Option<Ordering> {
    if let (Some(x), Some(y)) = (&a.date, &b.date) {
        let o = x.get(..7).cmp(&y.get(..7));
        if o != Ordering::Equal {
            return Some(o);
        }
        if x.len() == 10 && y.len() == 10 && x != y {
            return Some(x.cmp(y));
        }
    }
    let (x, y) = (a.rev.as_deref()?, b.rev.as_deref()?);
    if x == y {
        return Some(Ordering::Equal);
    }
    let num = |s: &str| -> Option<Vec<u64>> { s.split('.').map(|p| p.parse().ok()).collect() };
    if let (Some(p), Some(q)) = (num(x), num(y)) {
        return Some(p.cmp(&q));
    }
    let letters = |s: &str| {
        (s == "orig" || (!s.is_empty() && s.chars().all(|c| c.is_ascii_uppercase()))).then(|| {
            if s == "orig" {
                String::new()
            } else {
                s.to_owned()
            }
        })
    };
    match (letters(x), letters(y)) {
        (Some(p), Some(q)) => Some(p.len().cmp(&q.len()).then(p.cmp(&q))),
        _ => None,
    }
}

/// A sort key that orders revisions oldest first, total where [`newer`] is not.
///
/// (ops#13nov-1420b: `newer` with
/// "no telling" as equal is not transitive, and the standard sort panics on such a comparison): the month, the
/// day, then the revision word as `newer` reads it, an unknown date or word first. Where `newer` tells, the key
/// agrees with it whenever both dates are known or both missing.
pub fn order_key(i: &Ident) -> (Option<&str>, Option<&str>, RevKey<'_>) {
    let date = i.date.as_deref();
    (
        date.and_then(|d| d.get(..7)),
        date.filter(|d| d.len() == 10),
        RevKey::of(i.rev.as_deref()),
    )
}

/// A revision word in a total order: none, then words `newer` cannot compare (by their text), TI's `orig`, letters
/// (by length, then letter), numbers and dotted numbers (by value).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RevKey<'a> {
    None,
    Other(&'a str),
    Letters(usize, &'a str),
    Number(Vec<u64>),
}

impl<'a> RevKey<'a> {
    fn of(rev: Option<&'a str>) -> Self {
        let Some(s) = rev else {
            return RevKey::None;
        };
        if let Some(n) = s
            .split('.')
            .map(|p| p.parse().ok())
            .collect::<Option<Vec<u64>>>()
        {
            RevKey::Number(n)
        } else if s == "orig" {
            RevKey::Letters(0, "")
        } else if !s.is_empty() && s.chars().all(|c| c.is_ascii_uppercase()) {
            RevKey::Letters(s.len(), s)
        } else {
            RevKey::Other(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn st_footer_with_date() {
        let p = pages(&[
            "ES0548\nErrata sheet\nSTM32G0B1xB/xC/xE device errata\nIt gives a summary ... reference manual RM0444.\nES0548 - Rev 4 - September 2026\npage 1/30",
            "Silicon revision codes\nES0548 - Rev 4 - September 2026\npage 2/30",
            "ES0548 - Rev 4 - September 2026\nwww.st.com",
        ]);
        let i = parse(&p);
        assert_eq!(i.doc.as_deref(), Some("ES0548"));
        assert_eq!(i.rev.as_deref(), Some("4"));
        assert_eq!(i.date.as_deref(), Some("2026-09"));
        assert_eq!(i.label(), "ES0548 Rev 4 (Sep 2026)");
    }

    #[test]
    fn st_cover_date_and_mentions_ignored() {
        let p = pages(&[
            "RM0444\nReference manual\nSTM32G0x1 advanced Arm-based 32-bit MCUs\n• AN2606 application note on booting STM32 MCUs\nNovember 2020\nRM0444 Rev 5\n1/1390\nwww.st.com",
            "Contents RM0444\n2/1390 RM0444 Rev 5",
            "RM0444 Contents\nRM0444 Rev 5 3/1390 see DS13560 Rev 4",
        ]);
        let i = parse(&p);
        assert_eq!(
            (i.doc.as_deref(), i.rev.as_deref(), i.date.as_deref()),
            (Some("RM0444"), Some("5"), Some("2020-11"))
        );
    }

    #[test]
    fn st_needs_the_vendor_and_a_repeated_footer() {
        // no ST in the text: a third-party note quoting an ST manual stays unknown
        let p = pages(&["see RM0444 Rev 5", "RM0444 Rev 5", "RM0444 Rev 5"]);
        assert!(parse(&p).is_empty());
        // ST, but the footer only once in a long document: a mention, not the document's own
        let p = pages(&["STMicroelectronics app note quoting RM0444 Rev 5", "x", "y"]);
        assert!(parse(&p).is_empty());
        // one-page flyer: once is enough
        let p = pages(&["DB1234 Rev 2\nwww.st.com\nMarch 2024"]);
        assert_eq!(parse(&p).doc.as_deref(), Some("DB1234"));
    }

    #[test]
    fn ti_revised_and_original() {
        let head = "SLUSAT1A – MARCH 2013 – REVISED NOVEMBER 2015";
        let p = pages(&[
            &format!("bq27510-G3\n{head}\nTexas Instruments"),
            head,
            head,
        ]);
        let i = parse(&p);
        assert_eq!(
            (i.doc.as_deref(), i.rev.as_deref(), i.date.as_deref()),
            (Some("SLUSAT1"), Some("A"), Some("2015-11"))
        );
        let head = "SLUSC16 – NOVEMBER 2015";
        let p = pages(&[&format!("bq76200 {head} Texas Instruments"), head]);
        let i = parse(&p);
        assert_eq!(
            (i.doc.as_deref(), i.rev.as_deref(), i.date.as_deref()),
            (Some("SLUSC16"), Some("orig"), Some("2015-11"))
        );
    }

    #[test]
    fn nxp_both_forms() {
        let p = pages(&[
            "PCA9460\nRev. 1 — 4 August 2021\nProduct data sheet\nNXP",
            "Rev. 1 — 4 August 2021 © NXP B.V. 2021",
            "Rev. 1 — 4 August 2021",
        ]);
        let p2 = pages(&[
            "AN13698\nPCA9451A application note\nRev. 13 — 26 May 2023",
            "AN13698\n\nNXP Semiconductors\nPCA9451A application note\nRev. 13 — 26 May 2023\n© 2023 NXP B.V.",
            "AN13698\n\nNXP Semiconductors\nRev. 13 — 26 May 2023",
        ]);
        assert_eq!(parse(&p2).label(), "AN13698 Rev 13 (26 May 2023)");
        let i = parse(&p);
        assert_eq!(
            (i.doc, i.rev.as_deref(), i.date.as_deref()),
            (None, Some("1"), Some("2021-08-04"))
        );
        let p = pages(&[
            "Document Number: IMX8ULPCEC\nRev. B, 06/2021\nNXP Semiconductors",
            "i.MX 8ULP Applications Processor—Consumer Products, Rev. B, 06/2021",
            "i.MX 8ULP Applications Processor—Consumer Products, Rev. B, 06/2021",
        ]);
        let i = parse(&p);
        assert_eq!(
            (i.doc.as_deref(), i.rev.as_deref(), i.date.as_deref()),
            (Some("IMX8ULPCEC"), Some("B"), Some("2021-06"))
        );
    }

    #[test]
    fn microchip_analog_renesas_raspberry() {
        let p = pages(&[
            "Microchip Technology Inc. DS20005678B-page 1",
            "DS20005678B-page 2",
        ]);
        assert_eq!(parse(&p).label(), "DS20005678 Rev B");
        let p = pages(&[
            "Data Sheet ADM3055E Rev. B Analog Devices",
            "REVISION HISTORY 9/2019—Rev. A to Rev. B 11/2018—Rev. 0 to Rev. A Rev. B | Page 2 of 21",
            "Rev. B | Page 3 of 21",
        ]);
        let i = parse(&p);
        assert_eq!(
            (i.rev.as_deref(), i.date.as_deref()),
            (Some("B"), Some("2019-09"))
        );
        let p = pages(&[
            "SLG46824 Revision 3.20 30-Nov-2023 © 2023 Renesas Electronics Corporation",
            "Revision 3.20 30-Nov-2023",
            "Revision 3.20 30-Nov-2023",
        ]);
        assert_eq!(parse(&p).label(), "Rev 3.20 (30 Nov 2023)");
        let p = pages(&[
            "RP2350 Datasheet\nA microcontroller by Raspberry Pi.",
            "Colophon\nBuild date: 2024-08-08\nBuild version: 1ca8b26-clean",
        ]);
        let i = parse(&p);
        assert_eq!(
            (i.doc.as_deref(), i.rev.as_deref(), i.date.as_deref()),
            (
                Some("RP2350 Datasheet"),
                Some("1ca8b26-clean"),
                Some("2024-08-08")
            )
        );
    }

    #[test]
    fn unknown_stays_unknown() {
        assert!(parse(&pages(&["A book about motors, 2nd edition, 2019"])).is_empty());
        assert!(parse(&[]).is_empty());
        // two footers equally often: no telling which is the document's own
        let p = pages(&[
            "www.st.com RM0444 Rev 5 RM0440 Rev 7",
            "RM0444 Rev 5 RM0440 Rev 7",
            "x",
        ]);
        assert!(parse(&p).is_empty());
    }

    fn srcs(i: &Ident) -> (Option<&str>, Option<&str>, Option<Src>, Option<Src>) {
        (i.rev.as_deref(), i.date.as_deref(), i.rev_src, i.date_src)
    }

    #[test]
    fn revision_history_tables_newest_row() {
        let body = || "Some body text of a page.".to_owned();
        // ADI: revision numbers and dates as columns, a loose 3/23 form, the newest row last
        let mut p = vec![body(); 6];
        p.push("Revision History REVISION NUMBER 0 REVISION DATE 3/23 1 7/23 DESCRIPTION Release for Market Intro".into());
        let i = parse(&p);
        assert_eq!(
            srcs(&i),
            (
                Some("1"),
                Some("2023-07"),
                Some(Src::History),
                Some(Src::History)
            )
        );
        // Nordic: the date before its version
        let mut p = vec![body(); 4];
        p[2] =
            "Revision History Date Version October 2014 3.1 Description Added documentation".into();
        assert_eq!(parse(&p).label(), "Rev 3.1 (Oct 2014)");
        // NXP: the document's id with v.N and a compact date
        let mut p = vec![body(); 5];
        p[1] = "Revision history Rev Date Description v.2.0 20230526 Updated Figure 1 v.1.0 20220830 Initial version".into();
        assert_eq!(parse(&p).label(), "Rev 2.0 (26 May 2023)");
        // a table of contents line is no table
        let mut p = vec![body(); 5];
        p[0] = "17 Revision History ........ 158".into();
        assert!(parse(&p).is_empty());
        // a revision the footer says picks its own row
        let mut p = vec!["Doc X Rev. B | Page 1 of 9".to_owned(); 4];
        p.push("REVISION HISTORY 9/2019—Rev. A to Rev. B 11/2018—Rev. 0 to Rev. A".into());
        let i = parse(&p);
        assert_eq!(
            srcs(&i),
            (
                Some("B"),
                Some("2019-09"),
                Some(Src::Footer),
                Some(Src::History)
            )
        );
    }

    #[test]
    fn footers_covers_and_the_pdf_date() {
        // a revision and a date repeated on the pages, any vendor
        let foot = "Document Revision V1.03 • 2018-Sept-06";
        let p = pages(&[&format!("TMC4671 Datasheet {foot}"), foot, foot]);
        let i = parse(&p);
        assert_eq!(
            srcs(&i),
            (
                Some("1.03"),
                Some("2018-09-06"),
                Some(Src::Footer),
                Some(Src::Footer)
            )
        );
        // on the cover once
        let p = pages(&["30-May 2018 Ver.1.9 EYSGJNZXX Data Report", "x", "y"]);
        assert_eq!(
            srcs(&parse(&p)),
            (
                Some("1.9"),
                Some("2018-05-30"),
                Some(Src::Cover),
                Some(Src::Cover)
            )
        );
        let p = pages(&[
            "M66 AT Commands Manual\nDate: 2014-08-15\nwww.quectel.com",
            "x",
            "y",
        ]);
        assert_eq!(parse(&p).date.as_deref(), Some("2014-08-15"));
        let p = pages(&[
            "nRF51822\nProduct Specification v3.1\nKey Features",
            "x",
            "y",
        ]);
        assert_eq!(parse(&p).rev.as_deref(), Some("3.1"));
        // a line that is only a date
        let p = pages(&["Some application note\nMarch 2021\nIntro", "x", "y"]);
        assert_eq!(
            srcs(&parse(&p)),
            (None, Some("2021-03"), None, Some(Src::Cover))
        );
        // nothing in the text: the PDF's own date, a guess shown with ~
        let m = Meta::from_pdfinfo(
            "Title: x\nCreationDate:   2023-03-01T10:00:00+01\nModDate: 2023-03-14T09:00:00Z\n",
        );
        let (i, _) = identify(&pages(&["plain words", "more"]), Some(&m));
        assert_eq!(srcs(&i), (None, Some("2023-03"), None, Some(Src::PdfMeta)));
        assert_eq!(i.label(), "(~Mar 2023)");
        // a file name is never read: nothing but the text and the PDF's dates go in
        assert!(parse(&pages(&["A book about motors, 2nd edition, 2019"])).is_empty());
    }

    #[test]
    fn the_maker_by_its_name_in_the_text() {
        let (_, v) = identify(
            &pages(&[
                "ES0548 - Rev 4 - September 2026 STMicroelectronics",
                "ES0548 - Rev 4 - September 2026",
            ]),
            None,
        );
        assert_eq!(v, Some("ST"));
        let (_, v) = identify(
            &pages(&["Nordic Semiconductor ASA", "© Nordic Semiconductor"]),
            None,
        );
        assert_eq!(v, Some("Nordic"));
        // named once, or two makers as often: no telling
        assert_eq!(
            identify(&pages(&["see Nordic Semiconductor"]), None).1,
            None
        );
        let two = "Murata Murata Semtech Semtech";
        assert_eq!(identify(&pages(&[two]), None).1, None);
    }

    #[test]
    fn revision_order() {
        let i = |rev: &str, date: Option<&str>| Ident {
            doc: Some("X".into()),
            rev: Some(rev.into()),
            date: date.map(String::from),
            ..Ident::default()
        };
        assert_eq!(
            newer(&i("4", Some("2026-09")), &i("3", Some("2024-03"))),
            Some(Ordering::Greater)
        );
        assert_eq!(
            newer(&i("10", None), &i("9", None)),
            Some(Ordering::Greater)
        );
        assert_eq!(
            newer(&i("3.20", None), &i("3.18", None)),
            Some(Ordering::Greater)
        );
        assert_eq!(newer(&i("orig", None), &i("A", None)), Some(Ordering::Less));
        assert_eq!(
            newer(&i("AA", None), &i("Z", None)),
            Some(Ordering::Greater)
        );
        assert_eq!(newer(&i("abc1234", None), &i("def5678", None)), None);
        assert_eq!(
            newer(
                &i("abc1234", Some("2024-08-08")),
                &i("def5678", Some("2024-07-29"))
            ),
            Some(Ordering::Greater)
        );
        assert_eq!(human_date("2021-08-04"), "4 Aug 2021");
    }

    /// ops#13nov-1420b: `newer` with "no telling" as equal makes a cycle (dates for two, revisions for the third),
    /// and `sort_by` on it panicked in `tpm lib docs --json`. The key sorts them, agreeing with `newer` where
    /// both dates are known.
    #[test]
    fn order_key_is_total() {
        let i = |rev: Option<&str>, date: Option<&str>| Ident {
            doc: Some("X".into()),
            rev: rev.map(String::from),
            date: date.map(String::from),
            ..Ident::default()
        };
        // newer: a < b by date, b < c by revision, c < a by revision: no total order
        let (a, b, c) = (
            i(Some("5"), Some("2020-01")),
            i(Some("3"), Some("2021-01")),
            i(Some("4"), None),
        );
        assert_eq!(newer(&a, &b), Some(Ordering::Less));
        assert_eq!(newer(&b, &c), Some(Ordering::Less));
        assert_eq!(newer(&c, &a), Some(Ordering::Less));
        let mut rows = vec![
            a.clone(),
            b.clone(),
            c.clone(),
            i(Some("abc1234"), None),
            i(Some("orig"), None),
            i(Some("B"), Some("2021-01-05")),
            i(None, None),
            i(Some("3.18"), None),
        ];
        // every pair, every way round: the standard sort checks the order and panics on a broken one
        for _ in 0..rows.len() {
            rows.rotate_left(1);
            let mut r = rows.clone();
            r.sort_by(|x, y| order_key(x).cmp(&order_key(y)));
            let mut s = rows.clone();
            s.reverse();
            s.sort_by(|x, y| order_key(x).cmp(&order_key(y)));
            assert_eq!(r, s);
        }
        // with both dates known, the key says what newer says
        for (x, y) in [(&a, &b), (&b, &i(Some("4"), Some("2021-03")))] {
            assert_eq!(
                Some(order_key(x).cmp(&order_key(y))),
                newer(x, y),
                "{x:?} {y:?}"
            );
        }
        let v = |s| i(Some(s), None);
        for (x, y) in [("9", "10"), ("3.18", "3.20"), ("orig", "A"), ("Z", "AA")] {
            assert_eq!(order_key(&v(x)).cmp(&order_key(&v(y))), Ordering::Less);
        }
    }
}
