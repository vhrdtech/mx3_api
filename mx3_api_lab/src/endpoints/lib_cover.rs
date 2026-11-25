//! Which parts a document of the library covers (LIB-1 coverage).
//!
//! A document is its content hash. It covers one part ([`CoverSpec::Mpn`]), every part number a glob matches ([`CoverSpec::Pattern`], `STM32G0C1CE*`), or every
//! part whose MPN decode reading is of a scheme with fixed values ([`CoverSpec::Family`], `yageo.rc:case_eia=0402`).
//!
//! A person writes covers ([`CoverAdd`]) and takes them back ([`CoverTakeBack`]); a derivation ([`CoverDerive`])
//! writes the part covers that datasheet URL facts, part number links and the documents' first pages show. A part
//! cover taken back overrules every other way: the document is no longer listed for that part.
//! The lookup by part number (`/lib/by_mpn`) answers the covers with their [`By`](super::library::By).
//!
//! Scopes: `lib:read` for [`CoversQuery`], `lib:write` for the rest.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "client")]
use crate::blocking::BlockingClient;

/// A person's covers of a content (lib:write)
pub const EP_LIB_COVER: &str = "/lib/cover";
/// Take back what a content covers (lib:write)
pub const EP_LIB_UNCOVER: &str = "/lib/uncover";
/// Every cover of a content, live and taken back (lib:read)
pub const EP_LIB_COVERS: &str = "/lib/covers";
/// Write again the derived part covers (lib:write)
pub const EP_LIB_COVER_DERIVE: &str = "/lib/cover/derive";

/// Targets per [`CoverAdd`]
pub const COVER_TARGETS_MAX: usize = 64;
/// Bytes of the reason a cover is taken back
pub const WHY_BYTES_MAX: usize = 4000;

/// What a cover covers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CoverSpec {
    /// One part, by its part number, or by its id when the number is several parts. A number the server does not
    /// know is kept as a pattern that matches only it.
    Mpn {
        mpn: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part_id: Option<i32>,
    },
    /// Every part number the glob matches, letter case ignored: `*` any run, `?` one character.
    Pattern { pattern: String },
    /// Every part whose current MPN decode reading is of `scheme` with these values (`case_eia` = `0402`).
    Family {
        scheme: String,
        segments: BTreeMap<String, String>,
    },
}

impl CoverSpec {
    /// A target from one word: a pattern when it holds `*` or `?`, a family as `scheme:key=value[,key=value]`,
    /// else a part number.
    ///
    /// # Errors
    /// An empty word, or a family whose values are not `key=value`.
    pub fn from_word(word: &str) -> Result<Self, String> {
        let w = word.trim();
        if w.is_empty() {
            return Err("an empty cover target".to_owned());
        }
        if w.contains(['*', '?']) {
            return Ok(Self::Pattern {
                pattern: w.to_owned(),
            });
        }
        if let Some((scheme, rest)) = w.split_once(':')
            && !scheme.is_empty()
            && scheme
                .bytes()
                .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-'))
            && rest.contains('=')
        {
            let mut segments = BTreeMap::new();
            for kv in rest.split(',') {
                let Some((k, v)) = kv.split_once('=') else {
                    return Err(format!("family {scheme}: `{kv}` is not key=value"));
                };
                segments.insert(k.trim().to_owned(), v.trim().to_owned());
            }
            return Ok(Self::Family {
                scheme: scheme.to_owned(),
                segments,
            });
        }
        Ok(Self::Mpn {
            mpn: w.to_owned(),
            part_id: None,
        })
    }

    /// The target as one word, the form [`CoverSpec::from_word`] reads.
    pub fn word(&self) -> String {
        match self {
            Self::Mpn { mpn, .. } => mpn.clone(),
            Self::Pattern { pattern } => pattern.clone(),
            Self::Family { scheme, segments } => {
                let kv: Vec<String> = segments.iter().map(|(k, v)| format!("{k}={v}")).collect();
                format!("{scheme}:{}", kv.join(","))
            }
        }
    }
}

/// Who knows that a document covers a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverSource {
    /// A person said so.
    Person,
    /// One of the part's datasheet URL facts is the origin of a file with this content.
    Fact,
    /// A fetch by part number saved or found this content for the part.
    Link,
    /// The part number is printed on the document's first pages.
    Text,
}

impl CoverSource {
    /// The word the JSON and the listing use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Fact => "fact",
            Self::Link => "link",
            Self::Text => "text",
        }
    }
}

/// A person's covers: content `hash` covers each of `targets`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverAdd {
    pub hash: String,
    pub targets: Vec<CoverSpec>,
}

/// One cover written, or the live one the person had already written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverAdded {
    pub id: i64,
    pub new: bool,
    /// The target as stored: a part with its id, or a part number nobody knows as a pattern
    pub target: CoverSpec,
}

/// Take back what content `hash` covers of `target`, saying why. A part no cover names yet is taken back too, so
/// no way lists the document for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverTakeBack {
    pub hash: String,
    pub target: CoverSpec,
    pub why: String,
}

/// Every cover of a content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoversQuery {
    pub hash: String,
}

/// Who took a cover back, when and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverRetracted {
    pub at: DateTime<Utc>,
    pub by: String,
    pub why: String,
}

/// A stored cover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverRow {
    pub id: i64,
    pub hash: String,
    pub target: CoverSpec,
    pub source: CoverSource,
    /// The URL a fact or link cover came from
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// A person's name, or the computer that ran the derivation
    pub by: String,
    pub at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retracted: Option<CoverRetracted>,
}

/// Write again the derived part covers, of every content or only of `hashes` (at most
/// [`HASHES_MAX`](super::library::HASHES_MAX)). Unset values take the server's defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverDerive {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hashes: Option<Vec<String>>,
    /// Pages from the first one read for part numbers
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_pages: Option<u32>,
    /// The shortest part number looked for in the text
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mpn_chars_min: Option<u32>,
}

/// What a derivation changed for one source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverChanged {
    pub added: u64,
    pub removed: u64,
    pub kept: u64,
}

/// What a derivation changed, per source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverDerived {
    pub fact: CoverChanged,
    pub link: CoverChanged,
    pub text: CoverChanged,
}

/// Write a person's covers.
#[cfg(feature = "client")]
pub fn cover(c: &BlockingClient, q: &CoverAdd) -> anyhow::Result<Vec<CoverAdded>> {
    c.post_json(q, EP_LIB_COVER)
}

/// Take a cover back; answers the ids of the covers taken back.
#[cfg(feature = "client")]
pub fn uncover(c: &BlockingClient, q: &CoverTakeBack) -> anyhow::Result<Vec<i64>> {
    c.post_json(q, EP_LIB_UNCOVER)
}

/// Every cover of a content, oldest first.
#[cfg(feature = "client")]
pub fn covers(c: &BlockingClient, q: &CoversQuery) -> anyhow::Result<Vec<CoverRow>> {
    c.post_json(q, EP_LIB_COVERS)
}

/// Write again the derived part covers.
#[cfg(feature = "client")]
pub fn derive(c: &BlockingClient, q: &CoverDerive) -> anyhow::Result<CoverDerived> {
    c.post_json(q, EP_LIB_COVER_DERIVE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_from_words() {
        assert_eq!(
            CoverSpec::from_word(" STM32G0C1CE* ").unwrap(),
            CoverSpec::Pattern {
                pattern: "STM32G0C1CE*".into()
            }
        );
        let fam = CoverSpec::from_word("yageo.rc:case_eia=0402,tol=F").unwrap();
        assert_eq!(fam.word(), "yageo.rc:case_eia=0402,tol=F");
        assert!(matches!(fam, CoverSpec::Family { ref segments, .. } if segments.len() == 2));
        assert_eq!(
            CoverSpec::from_word("SGM8181-1A2XN6G/TR").unwrap(),
            CoverSpec::Mpn {
                mpn: "SGM8181-1A2XN6G/TR".into(),
                part_id: None
            }
        );
        // a colon without key=value, or with capitals before it, is part of a part number
        assert!(matches!(
            CoverSpec::from_word("AB:CD").unwrap(),
            CoverSpec::Mpn { .. }
        ));
        assert!(matches!(
            CoverSpec::from_word("AB:C=D").unwrap(),
            CoverSpec::Mpn { .. }
        ));
        assert!(CoverSpec::from_word("x.y:a=1,b").is_err());
        assert!(CoverSpec::from_word("  ").is_err());
    }

    #[test]
    fn json_shapes() {
        let j = serde_json::to_value(CoverSpec::Pattern {
            pattern: "P*".into(),
        })
        .unwrap();
        assert_eq!(j, serde_json::json!({"kind": "pattern", "pattern": "P*"}));
        let j = serde_json::to_value(CoverSpec::Mpn {
            mpn: "P1".into(),
            part_id: None,
        })
        .unwrap();
        assert_eq!(j, serde_json::json!({"kind": "mpn", "mpn": "P1"}));
        let d: CoverDerive = serde_json::from_str("{}").unwrap();
        assert_eq!(d, CoverDerive::default());
        assert_eq!(
            serde_json::to_value(CoverSource::Text).unwrap(),
            CoverSource::Text.as_str()
        );
    }
}
