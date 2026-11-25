//! The library's documents of a part number, found three ways, all by content hash, so a file that moved or was
//! renamed is still found:
//!
//! 1. The MPN's links: a fetch by part number records which content each MPN led to, saved or found there
//!    already, with the URL it came from ([`By::Link`]).
//! 2. The files whose origin (source or referrer) is one of the part's datasheet URLs, a redirect page seen
//!    through ([`By::Source`]). A series catalogue then covers every MPN whose facts name it.
//! 3. Last, a file with no origin URL whose name is the name of one of those URLs (`part1-spec.pdf`), never a name
//!    every site uses (`datasheet.pdf`) ([`By::Name`]).
//!
//! Several documents for one MPN come best first: a surer way, then the URL's rank (the order of the part's URLs),
//! then the newest revision. [`rows`] turns the answer into one flat row per document with the path on this
//! computer, the JSON contract of `mx3 lib mpn --json`.
//!
//! The URLs and makers come from parts databases and distributors: untrusted text, only compared, never opened.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::ident::{self, Ident};
use super::roots::Roots;
use super::site;
use crate::endpoints::library::{By, LibPath, MpnDoc, MpnDocs};

/// The file extensions of library documents.
pub const EXTS: [&str; 7] = ["pdf", "djvu", "epub", "fb2", "docx", "txt", "md"];

/// One MPN's content that a fetch by part number saved or found, and the URL it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub mpn: String,
    pub hash: String,
    pub url: String,
}

/// A file of the catalog, with its origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogFile {
    pub at: LibPath,
    pub hash: String,
    pub ident: Ident,
    pub source: Option<String>,
    pub referrer: Option<String>,
}

/// What mx3 knows of a part number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// No such part.
    Unknown,
    /// One part, or several by that number: their maker and their datasheet URLs, best first.
    Known {
        maker: Option<String>,
        urls: Vec<String>,
    },
}

/// A URL's file name, lower case, when it names one document: a library file's extension and a stem that is not
/// a word every site uses (`datasheet.pdf`).
pub fn name_of(url: &str) -> Option<String> {
    const GENERIC: [&str; 8] = [
        "datasheet",
        "data-sheet",
        "data_sheet",
        "download",
        "document",
        "spec",
        "specification",
        "index",
    ];
    let u = url::Url::parse(url).ok()?;
    let name = u.path_segments()?.next_back()?.to_lowercase();
    let (stem, ext) = name.rsplit_once('.')?;
    (EXTS.contains(&ext) && stem.len() >= 4 && !GENERIC.contains(&stem)).then_some(name)
}

/// The URLs whose files [`lookup`] matches by origin: each as recorded and as a redirect seen through.
pub fn origin_urls(urls: &[String]) -> Vec<String> {
    let mut out: Vec<String> = urls
        .iter()
        .flat_map(|u| [u.clone(), site::direct(u)])
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The file names [`lookup`] matches files without an origin by.
pub fn url_names(urls: &[String]) -> Vec<String> {
    let mut out: Vec<String> = urls
        .iter()
        .filter_map(|u| name_of(&site::direct(u)))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A catalog file's name, lower case.
fn file_name(p: &LibPath) -> String {
    p.path.rsplit('/').next().unwrap_or(&p.path).to_lowercase()
}

/// How a content was found: a surer way first, then the better URL.
type Want = (By, usize, Option<String>);

/// One MPN's documents. `links` and `files` may hold other MPNs' too; `files` are the catalog's current ones.
pub fn lookup(mpn: &str, part: &Part, links: &[Link], files: &[CatalogFile]) -> MpnDocs {
    let (maker, urls) = match part {
        Part::Unknown => (None, &[][..]),
        Part::Known { maker, urls } => (maker.clone(), &urls[..]),
    };
    // each URL as recorded and as a redirect seen through: rank by the first form that names it
    let rank: HashMap<String, usize> = urls
        .iter()
        .enumerate()
        .flat_map(|(i, u)| [(u.clone(), i), (site::direct(u), i)])
        .rev()
        .collect();
    let rank_of = |u: Option<&str>| u.and_then(|u| rank.get(u).copied());
    let mut want: BTreeMap<String, Want> = BTreeMap::new();
    let mut add = |h: &str, by: By, r: usize, u: Option<&str>| {
        let new = (by, r, u.map(str::to_owned));
        let e = want.entry(h.to_owned()).or_insert_with(|| new.clone());
        if (new.0, new.1) < (e.0, e.1) {
            *e = new;
        }
    };
    let my_links = links.iter().filter(|l| l.mpn.eq_ignore_ascii_case(mpn));
    for l in my_links.clone() {
        // a link of a URL mx3 no longer names comes after mx3's own
        let r = rank_of(Some(&l.url)).unwrap_or(urls.len());
        add(&l.hash, By::Link, r, Some(&l.url));
    }
    let names: Vec<(usize, &String, String)> = urls
        .iter()
        .enumerate()
        .filter_map(|(i, u)| name_of(&site::direct(u)).map(|n| (i, u, n)))
        .collect();
    for f in files {
        let by = [f.source.as_deref(), f.referrer.as_deref()]
            .into_iter()
            .filter_map(|u| rank_of(u).map(|r| (r, u)))
            .min_by_key(|(r, _)| *r);
        if let Some((r, u)) = by {
            add(&f.hash, By::Source, r, u);
        } else if f.source.is_none() {
            let name = file_name(&f.at);
            if let Some((r, u, _)) = names.iter().find(|(_, _, n)| *n == name) {
                add(&f.hash, By::Name, *r, Some(u));
            }
        }
    }
    let mut found: Vec<(usize, MpnDoc)> = want
        .into_iter()
        .map(|(h, (by, r, u))| (r, doc_of(h, by, u, files)))
        .collect();
    found.sort_by(|(ra, a), (rb, b)| order(*ra, a).cmp(&order(*rb, b)));
    let why = found.is_empty().then(|| {
        if matches!(part, Part::Unknown) && my_links.clone().next().is_none() {
            "not in mx3".to_owned()
        } else if let Some(u) = urls.first() {
            format!("not in the library: {u} (mx3 lib fetch --mpn {mpn})")
        } else {
            "mx3 has no datasheet URL".to_owned()
        }
    });
    MpnDocs {
        mpn: mpn.to_owned(),
        maker,
        known: matches!(part, Part::Known { .. }),
        urls: urls.to_vec(),
        docs: found.into_iter().map(|(_, d)| d).collect(),
        why,
    }
}

/// The sort key [`order`] gives.
type DocOrder<'a> = (
    bool,
    bool,
    usize,
    Reverse<(Option<&'a str>, Option<&'a str>, ident::RevKey<'a>)>,
    Option<&'a LibPath>,
);

/// The order of an MPN's documents: one some file holds before one none does, a file name match last, then the
/// URL's rank, the newest revision, the path.
fn order(rank: usize, d: &MpnDoc) -> DocOrder<'_> {
    (
        d.paths.is_empty(),
        d.by == By::Name,
        rank,
        Reverse(ident::order_key(&d.ident)),
        d.paths.first(),
    )
}

/// A found content with its paths: the one whose origin names the URL first, then by path.
fn doc_of(hash: String, by: By, url: Option<String>, files: &[CatalogFile]) -> MpnDoc {
    let mut here: Vec<&CatalogFile> = files.iter().filter(|f| f.hash == hash).collect();
    let named = |f: &CatalogFile| {
        url.is_some()
            && (f.source.as_deref() == url.as_deref() || f.referrer.as_deref() == url.as_deref())
    };
    here.sort_by(|a, b| (!named(a), &a.at).cmp(&(!named(b), &b.at)));
    MpnDoc {
        ident: here.first().map(|f| f.ident.clone()).unwrap_or_default(),
        paths: here.iter().map(|f| f.at.clone()).collect(),
        hash,
        url,
        by,
    }
}

/// The longest part number [`sch_mpns`] takes.
const SCH_MPN_BYTES_MAX: usize = 64;

/// The part numbers of one `KiCad` schematic's text: its `Mpn`, `Mpn1`, `Mpn2`... fields
/// (`(property "Mpn1" "PART1" ...`). Not `KiCad`'s `~`, no spaces or control characters.
pub fn sch_mpns(text: &str) -> std::collections::BTreeSet<String> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"\(property\s+"(?i:mpn\d*)"\s+"((?:[^"\\]|\\.)*)""#)
            .expect("static regex")
    });
    RE.captures_iter(text)
        .map(|c| c[1].replace("\\\"", "\"").trim().to_owned())
        .filter(|v| {
            !v.is_empty()
                && v != "~"
                && v.len() <= SCH_MPN_BYTES_MAX
                && v.chars().all(|c| c.is_ascii_graphic())
        })
        .collect()
}

/// One document of a part number as `mx3 lib mpn --json` prints it, or why there is none. `mpn`, `maker`, `path`,
/// `hash`, `doc`, `rev` and `why` are a contract its consumers read: new fields may come, these stay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MpnRow {
    pub mpn: String,
    pub maker: Option<String>,
    /// The file on this computer; none when no file with the content is here
    pub path: Option<PathBuf>,
    /// The catalog's `share/path` of that file, or of the best one elsewhere
    pub file: Option<String>,
    pub hash: Option<String>,
    pub doc: Option<String>,
    pub rev: Option<String>,
    pub date: Option<String>,
    /// The URL that led to it
    pub url: Option<String>,
    pub by: Option<By>,
    /// Why there is no path: no document, or none of its files on this computer
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// The rows of an answer, the documents with a file on this computer before the others, each MPN's rows together
/// in the order asked.
pub fn rows(answer: &[MpnDocs], roots: &Roots) -> Vec<MpnRow> {
    let mut out = vec![];
    for m in answer {
        if m.docs.is_empty() {
            out.push(MpnRow {
                mpn: m.mpn.clone(),
                maker: m.maker.clone(),
                path: None,
                file: None,
                hash: None,
                doc: None,
                rev: None,
                date: None,
                url: None,
                by: None,
                why: Some(m.why.clone().unwrap_or_else(|| "no document".to_owned())),
            });
            continue;
        }
        let mut mine: Vec<MpnRow> = m.docs.iter().map(|d| row(m, d, roots)).collect();
        mine.sort_by_key(|r| r.path.is_none());
        out.extend(mine);
    }
    out
}

fn row(m: &MpnDocs, d: &MpnDoc, roots: &Roots) -> MpnRow {
    let local = d
        .paths
        .iter()
        .find_map(|p| roots.resolve(p).map(|abs| (p, abs)));
    let why = match (&local, d.paths.is_empty()) {
        (Some(_), _) => None,
        (None, true) => Some("the library knows this content, but no file holds it now".to_owned()),
        (None, false) => Some(
            "no file with its content is on this computer (mx3 lib index, or not synced yet)"
                .to_owned(),
        ),
    };
    let file = local.as_ref().map(|(p, _)| *p).or(d.paths.first());
    MpnRow {
        mpn: m.mpn.clone(),
        maker: m.maker.clone(),
        path: local.as_ref().map(|(_, abs)| abs.clone()),
        file: file.map(LibPath::rel),
        hash: Some(d.hash.clone()),
        doc: d.ident.doc.clone(),
        rev: d.ident.rev.clone(),
        date: d.ident.date.clone(),
        url: d.url.clone(),
        by: Some(d.by),
        why,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(share: &str, path: &str) -> LibPath {
        LibPath {
            share: share.into(),
            path: path.into(),
        }
    }

    fn file(path: &str, hash: &str, doc: &str, rev: &str, source: Option<&str>) -> CatalogFile {
        CatalogFile {
            at: at("ds", path),
            hash: hash.into(),
            ident: Ident {
                doc: (!doc.is_empty()).then(|| doc.into()),
                rev: (!rev.is_empty()).then(|| rev.into()),
                ..Default::default()
            },
            source: source.map(String::from),
            referrer: None,
        }
    }

    fn link(m: &str, h: &str, u: &str) -> Link {
        Link {
            mpn: m.into(),
            hash: h.into(),
            url: u.into(),
        }
    }

    fn known(maker: Option<&str>, urls: &[&str]) -> Part {
        Part::Known {
            maker: maker.map(String::from),
            urls: urls.iter().map(|u| (*u).to_owned()).collect(),
        }
    }

    #[test]
    fn by_link_and_by_source_best_first() {
        let part = known(
            Some("Maker"),
            &[
                "https://maker.example/ds.pdf",
                "https://mirror.example/copy.pdf",
            ],
        );
        let files = vec![
            file(
                "copy.pdf",
                "h-mirror",
                "DS1",
                "A",
                Some("https://mirror.example/copy.pdf"),
            ),
            file("ds-a.pdf", "h-maker", "DS1", "B", None),
            file(
                "zz/ds-b.pdf",
                "h-maker",
                "DS1",
                "B",
                Some("https://maker.example/ds.pdf"),
            ),
            file("series.pdf", "h-series", "CAT", "1", None),
        ];
        let links = vec![
            link("p1", "h-series", "https://old.example/series.pdf"),
            link("P1", "h-gone", "https://maker.example/ds.pdf"),
            link("P2", "h-maker", "https://maker.example/ds.pdf"),
        ];
        let got = lookup("P1", &part, &links, &files);
        assert_eq!(got.maker.as_deref(), Some("Maker"));
        assert_eq!(got.why, None);
        let docs: Vec<(&str, By, Option<&str>)> = got
            .docs
            .iter()
            .map(|d| {
                (
                    d.hash.as_str(),
                    d.by,
                    d.paths.first().map(|p| p.path.as_str()),
                )
            })
            .collect();
        // the maker's URL first (its copy that names the URL), then the mirror's, then a link of a URL mx3 no
        // longer names; a linked content no file holds last
        assert_eq!(
            docs,
            [
                ("h-maker", By::Source, Some("zz/ds-b.pdf")),
                ("h-mirror", By::Source, Some("copy.pdf")),
                ("h-series", By::Link, Some("series.pdf")),
                ("h-gone", By::Link, None),
            ]
        );
        assert_eq!(got.docs[0].paths.len(), 2);
        assert_eq!(got.docs[0].ident.rev.as_deref(), Some("B"));
    }

    #[test]
    fn why_there_is_none() {
        let why = |part: &Part| lookup("P", part, &[], &[]).why.unwrap();
        assert_eq!(why(&Part::Unknown), "not in mx3");
        assert_eq!(why(&known(Some("Acme"), &[])), "mx3 has no datasheet URL");
        assert_eq!(
            why(&known(Some("Acme"), &["https://acme.example/p.pdf"])),
            "not in the library: https://acme.example/p.pdf (mx3 lib fetch --mpn P)"
        );
        // a part mx3 does not know, but a link of it
        let l = [link("P", "h", "https://acme.example/p.pdf")];
        let got = lookup("P", &Part::Unknown, &l, &[]);
        assert_eq!(got.docs.len(), 1);
        assert_eq!(got.docs[0].paths, []);
    }

    #[test]
    fn a_redirect_is_matched_as_a_fetch_follows_it() {
        let part = known(
            None,
            &[
                "https://www.example.com/docs/go.tsp?distId=10&gotoUrl=https%3A%2F%2Fwww.example.com%2Flit%2Fgpn%2Fpart1",
            ],
        );
        let files = vec![file(
            "part1.pdf",
            "h",
            "DOC1",
            "C",
            Some("https://www.example.com/lit/gpn/part1"),
        )];
        let got = lookup("PART1", &part, &[], &files);
        assert_eq!(got.docs[0].by, By::Source);
        assert_eq!(
            got.docs[0].url.as_deref(),
            Some("https://www.example.com/lit/gpn/part1")
        );
        assert!(
            origin_urls(match &part {
                Part::Known { urls, .. } => urls,
                Part::Unknown => unreachable!("built known"),
            })
            .contains(&"https://www.example.com/lit/gpn/part1".to_owned())
        );
    }

    /// A file added before origins were kept (no source) is found by its name, after the surer ways; a file with
    /// a source of its own, or a URL with a name every site uses, is not.
    #[test]
    fn by_file_name_last() {
        let urls = [
            "https://maker.example/files/specs/part1-spec.pdf",
            "https://x.example/Datasheet.pdf",
        ];
        let part = known(Some("Maker"), &urls);
        let files = vec![
            file("Maker/part1-spec.pdf", "h-old", "", "", None),
            file(
                "other/part1-spec.pdf",
                "h-src",
                "",
                "",
                Some("https://elsewhere.example/part1-spec.pdf"),
            ),
            file("x/datasheet.pdf", "h-generic", "", "", None),
            file("Maker/part1-new.pdf", "h-link", "", "", None),
        ];
        let links = vec![link("PART1-A", "h-link", "https://maker.example/new.pdf")];
        let got = lookup("PART1-A", &part, &links, &files);
        let docs: Vec<(&str, By)> = got.docs.iter().map(|d| (d.hash.as_str(), d.by)).collect();
        assert_eq!(docs, [("h-link", By::Link), ("h-old", By::Name)]);
        assert_eq!(got.docs[1].url.as_deref(), Some(urls[0]));
        assert_eq!(url_names(&urls.map(String::from)), ["part1-spec.pdf"]);
    }

    #[test]
    fn mpn_fields_of_a_schematic() {
        let t = r#"(symbol (property "Reference" "U1") (property "Mpn1" "PART1-DR" (at 0 0 0))
            (property "Mpn2" "PART1-DR") (property "mpn" "CAP-0402") (property "Mpn3" "~")
            (property "Mpn4" "") (property "Mpn5" "a b") (property "Datasheet" "x") (property "MpnX" "NO")
            (property "Mpn6" "Q\"1"))"#;
        let m: Vec<String> = sch_mpns(t).into_iter().collect();
        assert_eq!(m, ["CAP-0402", "PART1-DR", "Q\"1"]);
    }

    /// The fields a consumer of `mx3 lib mpn --json` reads are there, found or not, with the same names and
    /// types as before the library moved into mx3.
    #[test]
    fn json_keeps_the_contract_fields() {
        let found = MpnRow {
            mpn: "P1".into(),
            maker: Some("M".into()),
            path: Some(PathBuf::from("/l/ds/a.pdf")),
            file: Some("ds/a.pdf".into()),
            hash: Some("h".into()),
            doc: Some("DS1".into()),
            rev: Some("B".into()),
            date: None,
            url: Some("https://m.example/a.pdf".into()),
            by: Some(By::Source),
            why: None,
        };
        let j = serde_json::to_value(&found).unwrap();
        for k in [
            "mpn", "maker", "path", "hash", "doc", "rev", "date", "url", "by",
        ] {
            assert!(j.get(k).is_some(), "{k}");
        }
        assert_eq!(j["path"], "/l/ds/a.pdf");
        assert_eq!(j["by"], "source");
        assert!(j.get("why").is_none());
        let none = MpnRow {
            path: None,
            hash: None,
            why: Some("not in mx3".into()),
            ..found
        };
        let j = serde_json::to_value(&none).unwrap();
        assert!(j["path"].is_null());
        assert_eq!(j["why"], "not in mx3");
        assert_eq!(serde_json::from_value::<MpnRow>(j).unwrap(), none);
    }

    #[test]
    fn rows_put_files_here_first_and_say_why() {
        let dir = std::env::temp_dir().join(format!("mx3-api-rows-{}", std::process::id()));
        let share = dir.join("ds");
        std::fs::create_dir_all(share.join("b")).unwrap();
        std::fs::write(share.join("b/here.pdf"), b"x").unwrap();
        let roots = Roots::new(vec![share.clone()]);
        let doc = |h: &str, paths: &[&str]| MpnDoc {
            hash: h.into(),
            ident: Ident {
                doc: Some("D".into()),
                ..Default::default()
            },
            url: None,
            by: By::Link,
            paths: paths.iter().map(|p| at("ds", p)).collect(),
        };
        let answer = [
            MpnDocs {
                mpn: "P1".into(),
                maker: Some("M".into()),
                known: true,
                urls: vec![],
                docs: vec![
                    doc("h1", &["a/elsewhere.pdf"]),
                    doc("h2", &[]),
                    doc("h3", &["a/x.pdf", "b/here.pdf"]),
                ],
                why: None,
            },
            MpnDocs {
                mpn: "P2".into(),
                maker: None,
                known: false,
                urls: vec![],
                docs: vec![],
                why: Some("not in mx3".into()),
            },
        ];
        let r = rows(&answer, &roots);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].hash.as_deref(), Some("h3"));
        assert_eq!(
            r[0].path.as_deref(),
            Some(share.join("b/here.pdf").as_path())
        );
        assert_eq!(r[0].file.as_deref(), Some("ds/b/here.pdf"));
        assert_eq!(r[0].why, None);
        assert_eq!(r[1].hash.as_deref(), Some("h1"));
        assert_eq!(r[1].file.as_deref(), Some("ds/a/elsewhere.pdf"));
        assert!(r[1].why.as_deref().unwrap().contains("not synced yet"));
        assert!(r[2].why.as_deref().unwrap().contains("no file holds it"));
        assert_eq!(r[3].why.as_deref(), Some("not in mx3"));
        assert_eq!(r[3].hash, None);
    }
}
