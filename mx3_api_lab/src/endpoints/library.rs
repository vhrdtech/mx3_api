//! The document library (LIB-1): the full text and catalog of a collection of documents (datasheets, books, errata)
//! that each computer keeps in the same folders, served from one database.
//!
//! A file is known by its share (the collection's top folder, `datasheets`) and its path below it, the same on every
//! computer; its text by the blake3 of its bytes (`hash`, 64 lowercase hex digits), so a file at two paths is
//! extracted once. Computers extract text and upload it ([`Upload`]); the server keeps the catalog of which content
//! sits at which path ([`CatalogSync`]) and answers searches ([`SearchQuery`]) and the list of documents
//! ([`DocsQuery`]) that open and completion read offline, and which documents a part number has ([`ByMpnQuery`]).
//!
//! Scopes: `lib:read` for reading, `lib:write` for uploads and the catalog.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "client")]
use crate::blocking::BlockingClient;

/// Which contents the server lacks or holds a worse extraction of (lib:write)
pub const EP_LIB_HAVE: &str = "/lib/have";
/// Upload extracted text (lib:write)
pub const EP_LIB_UPLOAD: &str = "/lib/upload";
/// A new reading of the document identity of stored contents (lib:write)
pub const EP_LIB_IDENT: &str = "/lib/ident";
/// The stored text of some contents, for a new identity pass (lib:read)
pub const EP_LIB_TEXT: &str = "/lib/text";
/// What a computer sees of some shares; the server updates the catalog and says what changed (lib:write)
pub const EP_LIB_CATALOG_SYNC: &str = "/lib/catalog/sync";
/// Full-text search (lib:read)
pub const EP_LIB_SEARCH: &str = "/lib/search";
/// Every file of the catalog, or nothing when unchanged since a position (lib:read)
pub const EP_LIB_DOCS: &str = "/lib/docs";
/// What the library holds (lib:read)
pub const EP_LIB_STATS: &str = "/lib/stats";
/// The documents of part numbers (lib:read)
pub const EP_LIB_BY_MPN: &str = "/lib/by_mpn";

/// Contents per [`HaveQuery`], [`TextQuery`] or [`IdentUpdate`]
pub const HASHES_MAX: usize = 5000;
/// Contents per [`Upload`]
pub const UPLOAD_CONTENTS_MAX: usize = 64;
/// Pages of one content
pub const PAGES_MAX: usize = 20_000;
/// Bytes of one page's text; the server stores a longer page in parts of at most [`PART_BYTES`]
pub const PAGE_BYTES_MAX: usize = 8 * 1024 * 1024;
/// Bytes of one stored part of a page (a full-text vector holds at most 1 MB)
pub const PART_BYTES: usize = 100 * 1024;
/// Bytes of all page text in one [`Upload`]
pub const UPLOAD_BYTES_MAX: usize = 12 * 1024 * 1024;
/// Files per [`CatalogSync`]
pub const SEEN_MAX: usize = 100_000;
/// Bytes of a path below its share
pub const PATH_MAX: usize = 4000;
/// Words of a search
pub const WORDS_MAX: usize = 32;
/// Hits a search may ask for
pub const HITS_MAX: usize = 500;
/// Part numbers per [`ByMpnQuery`]
pub const BY_MPN_MAX: usize = 2000;
/// Bytes of one part number
pub const MPN_BYTES_MAX: usize = 200;

/// A content hash: blake3 of the file's bytes, 64 lowercase hex digits.
pub fn valid_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A share: the collection's top folder name, letters, digits, `_`, `.`, `-`.
pub fn valid_share(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

/// A path below its share: relative, `/`-separated, no empty, `.` or `..` segment, no control character.
pub fn valid_path(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= PATH_MAX
        && !s.chars().any(char::is_control)
        && s.split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

pub use crate::library::ident::{Ident, Src as IdentSrc};

/// How the day a file entered the library is known: added by hand, fetched, or only its modification time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AddedHow {
    Add,
    Fetch,
    Mtime,
}

/// When a file entered the library and from where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub added: NaiveDate,
    pub how: AddedHow,
    /// The URL it was downloaded from
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The page that linked it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referrer: Option<String>,
}

/// An extraction's quality as a computer would produce it: its version and the OCR languages it would use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub hash: String,
    pub extract_v: i32,
    /// `eng+rus`; empty without OCR
    #[serde(default)]
    pub ocr: String,
}

/// Which of these contents the server lacks, holds a worse extraction of, or holds with an older identity reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HaveQuery {
    pub offers: Vec<Offer>,
    /// The identity rules' version the computer runs
    pub idv: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Have {
    /// Not stored at all
    pub missing: Vec<String>,
    /// Stored, but the offer is better (newer version, more OCR languages, or the stored one failed)
    pub worse: Vec<String>,
    /// Stored with an identity read by older rules than `idv`
    pub ident_stale: Vec<String>,
}

/// One page's text, 1-based.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageText {
    pub page: i32,
    pub body: String,
}

/// One content's extraction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentUpload {
    pub hash: String,
    pub extract_v: i32,
    #[serde(default)]
    pub ocr: String,
    /// Why extraction failed; then `pages` is empty
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub ident: Ident,
    pub idv: i32,
    /// The PDF's own dates, ISO days
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf_created: Option<String>,
    pub pages: Vec<PageText>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Upload {
    pub contents: Vec<ContentUpload>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Uploaded {
    /// Stored (new, or better than what was there)
    pub stored: Vec<String>,
    /// Not stored: the server holds an extraction at least as good
    pub kept: Vec<String>,
}

/// A new identity reading of stored contents, by newer rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdentUpdate {
    pub idv: i32,
    pub items: Vec<IdentItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdentItem {
    pub hash: String,
    pub ident: Ident,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextQuery {
    pub hashes: Vec<String>,
}

/// The stored text of one content, pages in order, a page's parts joined.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentText {
    pub hash: String,
    pub pages: Vec<PageText>,
}

/// Where a file lives, as the indexing computer reads it from its path and text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The version of the rules that told the kind
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kindv: Option<i32>,
}

/// A file as one computer sees it now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    pub share: String,
    pub path: String,
    pub hash: String,
    pub size: i64,
    /// The file's modification day: `since` of what a first sync enters
    pub day: NaiveDate,
    #[serde(default)]
    pub ident: Ident,
    #[serde(default)]
    pub place: Place,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
}

/// What a computer sees of the shares it walked. Paths of other shares are left alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogSync {
    pub shares: Vec<String>,
    pub seen: Vec<Seen>,
    /// Compute and report, write nothing
    #[serde(default)]
    pub dry_run: bool,
}

/// A document whose revision changed at a path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub share: String,
    pub path: String,
    pub old: Ident,
    pub new: Ident,
    /// `newer`, `older`, or none when there is no telling
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
}

/// What a catalog sync changed or noticed. Paths are `share/path`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncReport {
    /// The catalog held nothing of the walked shares: every file entered, none told one by one
    pub first: usize,
    /// Path, its content before and now
    pub changed: Vec<(String, String, String)>,
    /// From, to
    pub moved: Vec<(String, String)>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub revisions: Vec<Revision>,
    /// Paths whose content here is one the catalog says the path held before: not synced here yet
    pub stale: Vec<String>,
    /// Paths missing here that another computer wrote less than a day ago
    pub waiting: Vec<String>,
    /// Snapshot position after the sync
    pub seq: i64,
}

/// A search: every word must be in the file (its name or any page).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub words: Vec<String>,
    /// The words are one web-search query (`"phrase"`, `or`, `-word`) instead
    #[serde(default)]
    pub raw: bool,
    /// Files to answer, at most [`HITS_MAX`]
    pub n: usize,
    /// SQL `LIKE` patterns on the place, case ignored
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
}

/// One file a search found, best first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub share: String,
    pub path: String,
    pub hash: String,
    pub score: f64,
    /// The best page; 0 when only the name matched
    pub page: i32,
    /// Pages that match
    pub hits: i64,
    pub pages: i32,
    /// Words around the match on the best page, matched words in `[` `]`
    pub snippet: String,
    #[serde(default)]
    pub place: Place,
    #[serde(default)]
    pub ident: Ident,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsQuery {
    /// Answer `unchanged` when the catalog is still at this position
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_seq: Option<i64>,
    /// Also paths that went away
    #[serde(default)]
    pub gone: bool,
}

/// One file of the catalog, without text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocFile {
    pub share: String,
    pub path: String,
    pub hash: String,
    pub size: i64,
    pub since: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gone: Option<NaiveDate>,
    #[serde(default)]
    pub ident: Ident,
    #[serde(default)]
    pub place: Place,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    pub seq: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Docs {
    pub seq: i64,
    #[serde(default)]
    pub unchanged: bool,
    #[serde(default)]
    pub files: Vec<DocFile>,
}

/// A count by one value of a column (`datasheet` 208).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Count {
    pub key: String,
    pub n: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    /// Paths in the catalog, not gone
    pub files: i64,
    pub contents: i64,
    pub size: i64,
    pub chars: i64,
    pub pages: i64,
    /// Contents with OCR'd text
    pub ocr: i64,
    pub by_share: Vec<Count>,
    pub by_kind: Vec<Count>,
    /// Paths whose content has no text stored yet
    pub no_text: Vec<String>,
    /// Paths whose extraction failed, with why
    pub errors: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_extracted: Option<DateTime<Utc>>,
}

/// The documents of these part numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByMpnQuery {
    pub mpns: Vec<String>,
}

/// How a document was found for a part number, surest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum By {
    /// A fetch by part number saved or found this content for it.
    Link,
    /// A file's origin (its source or referrer) is one of the part's datasheet URLs.
    Source,
    /// A file with no origin URL whose name is the name of one of the part's datasheet URLs.
    Name,
}

impl By {
    /// The word the JSON and the listing use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Source => "source",
            Self::Name => "name",
        }
    }
}

/// A file of the catalog: its share and its path below it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LibPath {
    pub share: String,
    pub path: String,
}

impl LibPath {
    /// `share/path`, as the library lists it.
    pub fn rel(&self) -> String {
        format!("{}/{}", self.share, self.path)
    }
}

/// One document of a part number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MpnDoc {
    pub hash: String,
    /// The document the best path's file is
    #[serde(default)]
    pub ident: Ident,
    /// The URL that led to it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub by: By,
    /// The catalog's paths holding this content now, best first: one whose origin names `url`, then by path.
    /// Empty: no path holds it any more.
    #[serde(default)]
    pub paths: Vec<LibPath>,
}

/// The documents of one part number, best first; none with why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MpnDocs {
    pub mpn: String,
    /// The maker mx3 names for the part
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maker: Option<String>,
    #[serde(default)]
    pub docs: Vec<MpnDoc>,
    /// Why there is no document (`docs` is empty)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

#[cfg(feature = "client")]
pub fn have(c: &BlockingClient, q: &HaveQuery) -> anyhow::Result<Have> {
    c.post_json(q, EP_LIB_HAVE)
}

#[cfg(feature = "client")]
pub fn upload(c: &BlockingClient, q: &Upload) -> anyhow::Result<Uploaded> {
    c.post_json(q, EP_LIB_UPLOAD)
}

#[cfg(feature = "client")]
pub fn ident(c: &BlockingClient, q: &IdentUpdate) -> anyhow::Result<usize> {
    c.post_json(q, EP_LIB_IDENT)
}

#[cfg(feature = "client")]
pub fn text(c: &BlockingClient, q: &TextQuery) -> anyhow::Result<Vec<ContentText>> {
    c.post_json(q, EP_LIB_TEXT)
}

#[cfg(feature = "client")]
pub fn catalog_sync(c: &BlockingClient, q: &CatalogSync) -> anyhow::Result<SyncReport> {
    c.post_json(q, EP_LIB_CATALOG_SYNC)
}

#[cfg(feature = "client")]
pub fn search(c: &BlockingClient, q: &SearchQuery) -> anyhow::Result<Vec<SearchHit>> {
    c.post_json(q, EP_LIB_SEARCH)
}

#[cfg(feature = "client")]
pub fn docs(c: &BlockingClient, q: &DocsQuery) -> anyhow::Result<Docs> {
    c.post_json(q, EP_LIB_DOCS)
}

#[cfg(feature = "client")]
pub fn stats(c: &BlockingClient) -> anyhow::Result<Stats> {
    c.post_json(&(), EP_LIB_STATS)
}

/// The documents of each part number, in the order asked, once each.
#[cfg(feature = "client")]
pub fn by_mpn(c: &BlockingClient, q: &ByMpnQuery) -> anyhow::Result<Vec<MpnDocs>> {
    c.post_json(q, EP_LIB_BY_MPN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_shares_and_paths() {
        assert!(valid_hash(&"a1".repeat(32)));
        assert!(!valid_hash(&"A1".repeat(32)));
        assert!(!valid_hash("abc"));
        assert!(valid_share("datasheets"));
        assert!(!valid_share(".."));
        assert!(!valid_share("data sheets"));
        assert!(valid_path("ST/G0/es0548.pdf"));
        assert!(valid_path("a..b/.hidden/v1.2..pdf"));
        for p in ["", "/abs", "a//b", "a/../b", "./a", "a/.", "tab\there"] {
            assert!(!valid_path(p), "{p:?}");
        }
        assert!(!valid_path(&"x".repeat(PATH_MAX + 1)));
    }

    #[test]
    fn ident_sources_round_trip() {
        for s in [
            IdentSrc::Footer,
            IdentSrc::History,
            IdentSrc::Cover,
            IdentSrc::PdfMeta,
        ] {
            assert_eq!(IdentSrc::parse(s.as_str()), Some(s));
            let j = serde_json::to_string(&s).unwrap();
            assert_eq!(j, format!("\"{}\"", s.as_str()));
        }
        let i: Ident = serde_json::from_str("{}").unwrap();
        assert_eq!(i, Ident::default());
    }
}
