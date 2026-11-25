//! Parameter values as facts with sources and ranks (PRM-10).
//!
//! Parameter values as facts with sources and ranks (PRM-10, tpm P2527 design/parameters.md §3, §5): every value
//! is a statement of one source, the value clients see is the best current fact (rank, then newest), and the
//! vocabulary (definitions, sources, ranks) is data in the database. JSON endpoints under `/params/`.
//!
//! Since the cut-over (PRM-5) they are the only parameter API: `Part.parameters`, `BomItem.parameters`,
//! `Serial.parameters` and `BoxItem.params` carry the owner's resolved values, writes go through `params/set`.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rvariant::{Variant, VariantTy};
use serde::{Deserialize, Serialize};

#[cfg(feature = "client")]
use crate::blocking::BlockingClient;
use crate::endpoints::parameter::{ParameterDefId, ParameterFor};
use crate::endpoints::part::PartId;
#[cfg(feature = "client")]
use crate::request::RequestInitiator;
#[cfg(feature = "client")]
use crate::{MxClient, Promise};

pub const EP_PARAMS_DEFS: &str = "/params/defs";
pub const EP_PARAMS_DEF_CREATE: &str = "/params/def/create";
pub const EP_PARAMS_DEF_EDIT: &str = "/params/def/edit";
pub const EP_PARAMS_DEF_MERGE: &str = "/params/def/merge";
pub const EP_PARAMS_SOURCES: &str = "/params/sources";
pub const EP_PARAMS_RANK_SET: &str = "/params/rank/set";
pub const EP_PARAMS_RANK_CLEAR: &str = "/params/rank/clear";
pub const EP_PARAMS_OF: &str = "/params/of";
pub const EP_PARAMS_HISTORY: &str = "/params/history";
pub const EP_PARAMS_FACTS: &str = "/params/facts";
pub const EP_PARAMS_SET: &str = "/params/set";
pub const EP_PARAMS_RETRACT: &str = "/params/retract";
pub const EP_PARAMS_YIELD: &str = "/params/yield";
pub const EP_PARAMS_REVIEW: &str = "/params/review";
pub const EP_PARAMS_CONFLICTS: &str = "/params/conflicts";
pub const EP_PARAMS_CONFLICT_ACK: &str = "/params/conflict/ack";
pub const EP_PARAMS_SEARCH: &str = "/params/search";

/// What a fact is about: a part, a box item, a serial, a BOM line, a BOM or a PCB.
pub type ParamOwner = ParameterFor;

/// The condition a value holds under (`{"f": 100e3}`); empty = none. Part of a fact's identity.
pub type Cond = BTreeMap<String, serde_json::Value>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct FactId(pub i64);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct SourceId(pub i32);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum DefStatus {
    /// Came from a source as it named it, not curated yet
    Raw,
    /// A curated definition waiting for review
    Proposed,
    /// A stable key clients may rely on
    Curated,
    /// A synonym resolving under `merged_into`
    Merged,
    /// Rejected or no longer used; kept, since facts and history point at it
    Retired,
}

impl DefStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Proposed => "proposed",
            Self::Curated => "curated",
            Self::Merged => "merged",
            Self::Retired => "retired",
        }
    }

    pub const ALL: [Self; 5] = [
        Self::Raw,
        Self::Proposed,
        Self::Curated,
        Self::Merged,
        Self::Retired,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "raw" => Self::Raw,
            "proposed" => Self::Proposed,
            "curated" => Self::Curated,
            "merged" => Self::Merged,
            "retired" => Self::Retired,
            _ => return None,
        })
    }
}

/// A parameter definition with its vocabulary columns.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParamDef {
    pub id: ParameterDefId,
    /// The name it was created under (`parameter_def.r_name`)
    pub name: String,
    /// Stable machine name of a curated definition
    pub key: Option<String>,
    /// Display name: the label, else the name
    pub label: String,
    /// Value type with its unit; the source of truth for both
    pub ty: VariantTy,
    /// `ty` in words for clients that don't read rvariant: its [`ty_kind`] (`si`, `str`, `enum`, ...)
    #[serde(default)]
    pub kind: String,
    /// The unit of an `si` or `si_range` value (`Ω`, `F`, `V`)
    #[serde(default)]
    pub unit: Option<String>,
    /// The choices of an `enum` or `multi_enum` value
    #[serde(default)]
    pub variants: Vec<String>,
    pub doc: Option<String>,
    pub status: DefStatus,
    /// Who accepted or rejected the proposal, and when
    #[serde(default)]
    pub reviewed: Option<Reviewed>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub updated: Option<DateTime<Utc>>,
    /// A synonym merged into this definition: its facts resolve under that one
    pub merged_into: Option<ParameterDefId>,
    /// Relative tolerance for "the same value"
    pub compare: f64,
    pub aliases: Vec<ParamAlias>,
    /// Current facts stated for this definition (not superseded, retractions included), over all owners
    pub facts: i64,
}

/// How a source names a definition.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParamAlias {
    pub id: i32,
    pub def_id: ParameterDefId,
    pub source_id: SourceId,
    /// The source's key (`digikey`, `bom_import`)
    #[serde(default)]
    pub source: String,
    pub ext_name: Option<String>,
    pub ext_id: Option<String>,
    pub ext_scope: String,
    pub parser: Option<String>,
    pub parser_args: Option<serde_json::Value>,
    /// A change of its facts is worth a notice (a Digi-Key flag like NCNR)
    #[serde(default)]
    pub notify: bool,
    /// A second landing: the field still lands where its other aliases say, and also states this definition, read
    /// by this alias's parser; a text that does not parse states nothing here
    #[serde(default)]
    pub also: bool,
    pub note: String,
    #[serde(default)]
    pub created_by: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Person,
    Harvester,
    Import,
    Derivation,
}

impl SourceKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "person" => Self::Person,
            "harvester" => Self::Harvester,
            "import" => Self::Import,
            "derivation" => Self::Derivation,
            _ => return None,
        })
    }
}

/// Who states values, with its rank and overrides.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParamSource {
    pub id: SourceId,
    pub key: String,
    pub label: String,
    pub default_rank: i32,
    pub kind: SourceKind,
    /// API scope a caller needs to write facts of this source; empty = only the backend writes it
    pub writable_by: String,
    pub note: String,
    pub ranks: Vec<RankOverride>,
}

/// A source's rank for one definition, one part, or both (the most specific applies).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RankOverride {
    pub id: i32,
    pub source_id: SourceId,
    pub def_id: Option<ParameterDefId>,
    pub part_id: Option<PartId>,
    pub rank: i32,
    pub note: String,
    pub by: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Reviewed {
    pub by: String,
    pub at: DateTime<Utc>,
}

/// One statement of one source.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Fact {
    pub id: FactId,
    /// The definition it was stated for
    pub def_id: ParameterDefId,
    /// The definition it resolves under: the merge target of `def_id`, else `def_id`
    pub res_def_id: ParameterDefId,
    pub owner: ParamOwner,
    pub source_id: SourceId,
    /// The source's key (`manual`, `digikey`)
    pub source: String,
    pub value: Variant,
    /// The source's own text
    pub raw: Option<String>,
    /// Could not be parsed into the definition's type: `value` is the text
    pub unparsed: bool,
    /// What normalising the source's text found odd (`mojibake`, `trailing:Max`), `unparsed:<reason>` for text
    /// kept as text (PRM-12)
    #[serde(default)]
    pub warnings: Vec<String>,
    /// How the source named it, for re-deriving it later
    #[serde(default)]
    pub ext: Option<FactExt>,
    pub cond: Cond,
    /// When the source stated it
    pub at: DateTime<Utc>,
    /// When the source last said the same again
    pub seen_at: DateTime<Utc>,
    pub recorded: DateTime<Utc>,
    /// Set once the same source stated something newer; None = current
    pub superseded_at: Option<DateTime<Utc>>,
    /// "No value any more"; `value` is the value it retracts
    pub retracted: bool,
    pub rank_override: Option<i32>,
    /// The rank it resolves with: the override, else the most specific rank of its source, else the default
    pub eff_rank: i32,
    pub note: String,
    /// The API client (or the backend itself) that wrote it
    pub by: String,
    pub reviewed: Option<Reviewed>,
    pub derived_from: Option<FactId>,
}

/// How a source named a fact's parameter: its name, its id (a Digi-Key parametric id, `Product.Ncnr` for a fixed
/// field) and where it met it (a category path), as an alias matches them.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct FactExt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// For a value decoded from an MPN (MPN-1): the scheme's id and version and the segment it was read from (none
    /// for a fact the scheme implies)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment: Option<String>,
}

/// The value of one definition for one owner (and condition): the best current fact.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Resolved {
    /// The definition it resolves under
    pub def_id: ParameterDefId,
    pub key: Option<String>,
    /// That definition's name (`parameter_def.r_name`), what name lookups match
    pub name: String,
    pub label: String,
    pub value: Variant,
    pub raw: Option<String>,
    pub unparsed: bool,
    /// The winning fact's warnings (see [`Fact::warnings`])
    #[serde(default)]
    pub warnings: Vec<String>,
    pub cond: Cond,
    pub fact_id: FactId,
    /// The definition the winning fact was stated for (differs from `def_id` for a merged synonym)
    pub fact_def_id: ParameterDefId,
    pub source_id: SourceId,
    pub source: String,
    pub eff_rank: i32,
    pub at: DateTime<Utc>,
    /// No source has a value any more: this is the last one, retracted
    pub retracted: bool,
    pub reviewed: Option<Reviewed>,
    pub note: String,
    /// Other current facts at or above the conflict rank that disagree with this value (design §3.4), not yet
    /// acked; empty = no conflict
    #[serde(default)]
    pub conflict: Vec<FactId>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResolvedSet {
    pub owner: ParamOwner,
    pub values: Vec<Resolved>,
    /// Every current fact of the owner, when asked for with `with_facts`
    pub facts: Option<Vec<Fact>>,
}

impl Resolved {
    /// A value of `name` as if stated by `manual` now, not stored: for tests and placeholders.
    pub fn new(def_id: ParameterDefId, name: &str, value: Variant) -> Self {
        Self {
            def_id,
            key: None,
            name: name.into(),
            label: name.into(),
            value,
            raw: None,
            unparsed: false,
            warnings: vec![],
            cond: Cond::new(),
            fact_id: FactId(0),
            fact_def_id: def_id,
            source_id: SourceId(0),
            source: "manual".into(),
            eff_rank: 0,
            at: Utc::now(),
            retracted: false,
            reviewed: None,
            note: String::new(),
            conflict: Vec::new(),
        }
    }
}

/// The resolved value named `name` (ignoring case), the first by label order; a retracted one (no source has a
/// value any more) is no value.
pub fn find<'a>(values: &'a [Resolved], name: &str) -> Option<&'a Resolved> {
    values
        .iter()
        .find(|r| !r.retracted && r.name.eq_ignore_ascii_case(name))
}

/// A definition by id, by curated key or by name.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum DefRef {
    Id(ParameterDefId),
    Key(String),
    Name(String),
}

impl From<ParameterDefId> for DefRef {
    fn from(id: ParameterDefId) -> Self {
        Self::Id(id)
    }
}

// ---------------------------------------------------------------- requests

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ListDefs {
    /// In the name, key or label, ignoring case
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub status: Option<DefStatus>,
    /// Only definitions with an alias of this source (key)
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateDef {
    /// Unique, ignoring case
    pub name: String,
    pub ty: VariantTy,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub doc: Option<String>,
    /// None = `raw`; `proposed` for a curated definition waiting for review
    #[serde(default)]
    pub status: Option<DefStatus>,
    /// Why (the vocabulary log)
    #[serde(default)]
    pub note: String,
}

/// Change a definition.
///
/// Change a definition. A new `ty` re-derives every current fact resolving under it (merged synonyms included)
/// into the new type, as a renormalize does (its source's alias and parser, from its raw text, else its value), in
/// one transaction; a fact that does not fit is kept as text flagged unparsed and counted. The old facts stay in
/// the history.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EditDef {
    pub def: ParameterDefId,
    #[serde(default)]
    pub ty: Option<VariantTy>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub doc: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub status: Option<DefStatus>,
    #[serde(default)]
    pub compare: Option<f64>,
    /// Count what a new type would do, change nothing
    #[serde(default)]
    pub dry_run: bool,
    /// Why (the vocabulary log)
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DefEdited {
    /// The definition after the edit (before it, on a dry run)
    pub def: ParamDef,
    /// Facts re-parsed into the new type
    pub converted: u32,
    /// Facts already of the new type's value (unchanged)
    pub unchanged: u32,
    /// Facts that did not fit, kept as text flagged unparsed: the fact and why
    pub failed: Vec<(FactId, String)>,
    /// The type change's renormalize in full (PRM-12): per definition, warnings, examples
    #[serde(default)]
    pub renormalized: Option<Renormalized>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MergeDef {
    pub from: ParameterDefId,
    pub into: ParameterDefId,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SetRank {
    pub source: String,
    #[serde(default)]
    pub def: Option<ParameterDefId>,
    #[serde(default)]
    pub part: Option<PartId>,
    pub rank: i32,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClearRank {
    pub source: String,
    #[serde(default)]
    pub def: Option<ParameterDefId>,
    #[serde(default)]
    pub part: Option<PartId>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ParamsOf {
    pub owners: Vec<ParamOwner>,
    /// Only these definitions (by the definition they resolve under); empty with `keys` empty = all
    #[serde(default)]
    pub defs: Vec<ParameterDefId>,
    /// Only these curated keys
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub with_facts: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParamsHistory {
    pub owner: ParamOwner,
    /// Every fact resolving under this definition, synonyms merged into it included
    pub def: DefRef,
}

/// Every fact stated for a definition, over all owners (the value browser).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParamFacts {
    pub def: DefRef,
    /// Superseded facts too (the whole history), not only the current ones
    #[serde(default)]
    pub history: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SetParam {
    pub owner: ParamOwner,
    pub def: DefRef,
    /// The typed value; converted to the definition's type
    #[serde(default)]
    pub value: Option<Variant>,
    /// The source's text: parsed into the definition's type when no value is given, kept either way
    #[serde(default)]
    pub raw: Option<String>,
    /// The source's key; None = `manual`
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub cond: Cond,
    #[serde(default)]
    pub note: String,
    /// When the text does not parse, keep it as text (flagged unparsed) instead of failing
    #[serde(default)]
    pub store_unparsed: bool,
    /// `def` is a name no definition has: create it, typed like the value (text for a raw text)
    #[serde(default)]
    pub create_def: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SetOutcome {
    /// The source's current fact after the call
    pub fact: Fact,
    /// A new fact was written (false: the source said the same before, only `seen_at` moved)
    pub changed: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RetractParam {
    pub owner: ParamOwner,
    pub def: DefRef,
    /// None = `manual`
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub cond: Cond,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct YieldFact {
    pub fact: FactId,
    /// true: the fact gives way (rank `rank`, default 0); false: back to its source's rank
    pub yielded: bool,
    #[serde(default)]
    pub rank: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReviewFact {
    pub fact: FactId,
    /// false clears the mark
    pub reviewed: bool,
}

/// Two current facts of one owner, definition and condition that disagree (design §3.4), not yet acked.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParamConflict {
    pub owner: ParamOwner,
    /// The definition they resolve under
    pub def_id: ParameterDefId,
    pub key: Option<String>,
    pub name: String,
    pub label: String,
    pub cond: Cond,
    /// The lower-id fact of the pair
    pub fact_a: Fact,
    /// The higher-id fact of the pair
    pub fact_b: Fact,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ListConflicts {
    /// Empty = every owner
    #[serde(default)]
    pub owners: Vec<ParamOwner>,
    /// Facts at or above this rank only; None = the configured default (everything above `llm`)
    #[serde(default)]
    pub min_rank: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AckConflict {
    pub fact_a: FactId,
    pub fact_b: FactId,
    #[serde(default)]
    pub note: String,
}

/// A silenced conflict; a new fact for either side makes a new pair, which is not silenced.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConflictAck {
    pub id: i32,
    pub fact_a: FactId,
    pub fact_b: FactId,
    pub by: String,
    pub note: String,
    pub at: DateTime<Utc>,
}

// ---------------------------------------------------------------- calls

#[cfg(feature = "client")]
pub fn defs(c: &BlockingClient, q: &ListDefs) -> anyhow::Result<Vec<ParamDef>> {
    c.post_json(q, EP_PARAMS_DEFS)
}

#[cfg(feature = "client")]
pub fn def_create(c: &BlockingClient, q: &CreateDef) -> anyhow::Result<ParamDef> {
    c.post_json(q, EP_PARAMS_DEF_CREATE)
}

#[cfg(feature = "client")]
pub fn def_edit(c: &BlockingClient, q: &EditDef) -> anyhow::Result<DefEdited> {
    c.post_json(q, EP_PARAMS_DEF_EDIT)
}

#[cfg(feature = "client")]
pub fn facts(c: &BlockingClient, q: &ParamFacts) -> anyhow::Result<Vec<Fact>> {
    c.post_json(q, EP_PARAMS_FACTS)
}

#[cfg(feature = "client")]
pub fn def_merge(c: &BlockingClient, q: &MergeDef) -> anyhow::Result<ParamDef> {
    c.post_json(q, EP_PARAMS_DEF_MERGE)
}

#[cfg(feature = "client")]
pub fn sources(c: &BlockingClient) -> anyhow::Result<Vec<ParamSource>> {
    c.post_json(&(), EP_PARAMS_SOURCES)
}

#[cfg(feature = "client")]
pub fn rank_set(c: &BlockingClient, q: &SetRank) -> anyhow::Result<RankOverride> {
    c.post_json(q, EP_PARAMS_RANK_SET)
}

/// Whether there was an override to clear.
#[cfg(feature = "client")]
pub fn rank_clear(c: &BlockingClient, q: &ClearRank) -> anyhow::Result<bool> {
    c.post_json(q, EP_PARAMS_RANK_CLEAR)
}

#[cfg(feature = "client")]
pub fn of(c: &BlockingClient, q: &ParamsOf) -> anyhow::Result<Vec<ResolvedSet>> {
    c.post_json(q, EP_PARAMS_OF)
}

#[cfg(feature = "client")]
pub fn history(c: &BlockingClient, q: &ParamsHistory) -> anyhow::Result<Vec<Fact>> {
    c.post_json(q, EP_PARAMS_HISTORY)
}

#[cfg(feature = "client")]
pub fn set(c: &BlockingClient, q: &SetParam) -> anyhow::Result<SetOutcome> {
    c.post_json(q, EP_PARAMS_SET)
}

/// The retraction, None when the source had no value to retract.
#[cfg(feature = "client")]
pub fn retract(c: &BlockingClient, q: &RetractParam) -> anyhow::Result<Option<Fact>> {
    c.post_json(q, EP_PARAMS_RETRACT)
}

#[cfg(feature = "client")]
pub fn yield_fact(c: &BlockingClient, q: &YieldFact) -> anyhow::Result<Fact> {
    c.post_json(q, EP_PARAMS_YIELD)
}

#[cfg(feature = "client")]
pub fn review(c: &BlockingClient, q: &ReviewFact) -> anyhow::Result<Fact> {
    c.post_json(q, EP_PARAMS_REVIEW)
}

#[cfg(feature = "client")]
pub fn conflicts(c: &BlockingClient, q: &ListConflicts) -> anyhow::Result<Vec<ParamConflict>> {
    c.post_json(q, EP_PARAMS_CONFLICTS)
}

#[cfg(feature = "client")]
pub fn conflict_ack(c: &BlockingClient, q: &AckConflict) -> anyhow::Result<ConflictAck> {
    c.post_json(q, EP_PARAMS_CONFLICT_ACK)
}

// ---------------------------------------------------------------- by MPN (PRM-17)

pub const EP_PARAMS_BY_MPN: &str = "/params/by_mpn";

/// A part by manufacturer part number, the manufacturer narrowing it when given, as `lookup_parts` matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpnKey {
    #[serde(default)]
    pub manufacturer: Option<String>,
    pub mpn: String,
}

/// The most MPNs one [`ByMpn`] takes; each is looked up and decoded on its own, so a longer list goes in parts.
pub const BY_MPN_MPNS_MAX: usize = 1000;
/// The most keys one [`ByMpn`] takes.
pub const BY_MPN_KEYS_MAX: usize = 200;

/// Asks for the resolved values of parts by MPN (PRM-17), by definition key, so a client never matches raw names.
///
/// Empty `keys` = every definition, like [`ParamsOf`]. More than [`BY_MPN_MPNS_MAX`] MPNs or [`BY_MPN_KEYS_MAX`]
/// keys is refused.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ByMpn {
    pub mpns: Vec<MpnKey>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub with_facts: bool,
}

/// One requested MPN's answer: its part (if any), the values resolved for it, and its current `mpn_decode`
/// reading, which exists even for an MPN mx3 holds no part for (footprint-review families key on it).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MpnResult {
    pub mpn: MpnKey,
    /// `NotFound` and `NotUnique` both mean no value could be resolved; a client that only wants the part reuses
    /// the same match as `lookup_parts`
    pub part: crate::endpoints::part::LookupPartResult,
    pub values: Vec<Resolved>,
    /// Requested keys (the ones a definition exists for) with no value for this part; a retracted entry in
    /// `values` is no value
    pub missing: Vec<String>,
    pub facts: Option<Vec<Fact>>,
    pub mpn_decode: crate::endpoints::mpn::MpnReading,
}

/// The answer to [`ByMpn`].
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ByMpnResult {
    /// Requested keys no definition has at all (same for every MPN); every other key was looked up
    pub unknown_keys: Vec<String>,
    /// In the order asked
    pub results: Vec<MpnResult>,
}

#[cfg(feature = "client")]
pub fn by_mpn(c: &BlockingClient, q: &ByMpn) -> anyhow::Result<ByMpnResult> {
    c.post_json(q, EP_PARAMS_BY_MPN)
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn params_by_mpn(&self, q: &ByMpn, marker: &'static str) -> Promise<ByMpnResult> {
        self.params_call(EP_PARAMS_BY_MPN, q, marker)
    }
}

// The parametric search over facts: SRCH-7.

/// Which kind of owner a search looks for.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum SearchOwner {
    #[default]
    Part,
    BoxItem,
    Serial,
    BomItem,
    Bom,
    Pcb,
}

/// What a term asks of the resolved value of its definition. Numbers are in the SI base unit without prefix
/// (`49900` for 49.9 kΩ); a fact that is a range or a tolerance is compared by its bounds.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum SearchOp {
    /// Equal within the term's relative tolerance (a range or tolerance value matches when it covers the number)
    Eq(f64),
    /// Between the bounds, both included (a range value matches when it overlaps)
    Range { lo: f64, hi: f64 },
    /// At or above (within the tolerance)
    Ge(f64),
    /// At or below (within the tolerance)
    Le(f64),
    /// The text or enum value is one of these, case insensitive
    EnumIn(Vec<String>),
    /// The text contains this, case insensitive
    Text(String),
}

/// One condition on one definition; the caller names the definition, the search names none.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchTerm {
    pub def: DefRef,
    pub op: SearchOp,
    /// Relative tolerance of `Eq`, `Ge` and `Le` (0.001 = 0.1 %); none = the definition's `compare`
    #[serde(default)]
    pub tolerance: Option<f64>,
}

/// Owners whose resolved values satisfy every term.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ParamSearch {
    pub terms: Vec<SearchTerm>,
    #[serde(default)]
    pub owner: SearchOwner,
    /// Only parts with stock in a box (owner `part`) or box items with a count (owner `box_item`)
    #[serde(default)]
    pub in_warehouse: bool,
}

#[cfg(feature = "client")]
pub fn search(c: &BlockingClient, q: &ParamSearch) -> anyhow::Result<Vec<ParamOwner>> {
    c.post_json(q, EP_PARAMS_SEARCH)
}

// ---------------------------------------------------------------- calls from the UI

#[cfg(feature = "client")]
impl MxClient {
    fn params_call<Q: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        ep: &str,
        q: &Q,
        marker: &'static str,
    ) -> Promise<T> {
        let (req, rx) = RequestInitiator::post_json_arg(ep.into(), q);
        self.send_req(req, rx, marker)
    }

    pub fn params_defs(&self, q: &ListDefs, marker: &'static str) -> Promise<Vec<ParamDef>> {
        self.params_call(EP_PARAMS_DEFS, q, marker)
    }

    pub fn params_def_create(&self, q: &CreateDef, marker: &'static str) -> Promise<ParamDef> {
        self.params_call(EP_PARAMS_DEF_CREATE, q, marker)
    }

    pub fn params_def_edit(&self, q: &EditDef, marker: &'static str) -> Promise<DefEdited> {
        self.params_call(EP_PARAMS_DEF_EDIT, q, marker)
    }

    pub fn params_def_merge(&self, q: &MergeDef, marker: &'static str) -> Promise<ParamDef> {
        self.params_call(EP_PARAMS_DEF_MERGE, q, marker)
    }

    pub fn params_sources(&self, marker: &'static str) -> Promise<Vec<ParamSource>> {
        self.params_call(EP_PARAMS_SOURCES, &(), marker)
    }

    pub fn params_of(&self, q: &ParamsOf, marker: &'static str) -> Promise<Vec<ResolvedSet>> {
        self.params_call(EP_PARAMS_OF, q, marker)
    }

    pub fn params_history(&self, q: &ParamsHistory, marker: &'static str) -> Promise<Vec<Fact>> {
        self.params_call(EP_PARAMS_HISTORY, q, marker)
    }

    pub fn params_facts(&self, q: &ParamFacts, marker: &'static str) -> Promise<Vec<Fact>> {
        self.params_call(EP_PARAMS_FACTS, q, marker)
    }

    pub fn params_set(&self, q: &SetParam, marker: &'static str) -> Promise<SetOutcome> {
        self.params_call(EP_PARAMS_SET, q, marker)
    }

    pub fn params_retract(&self, q: &RetractParam, marker: &'static str) -> Promise<Option<Fact>> {
        self.params_call(EP_PARAMS_RETRACT, q, marker)
    }

    pub fn params_yield(&self, q: &YieldFact, marker: &'static str) -> Promise<Fact> {
        self.params_call(EP_PARAMS_YIELD, q, marker)
    }

    pub fn params_review(&self, q: &ReviewFact, marker: &'static str) -> Promise<Fact> {
        self.params_call(EP_PARAMS_REVIEW, q, marker)
    }

    pub fn params_rank_set(&self, q: &SetRank, marker: &'static str) -> Promise<RankOverride> {
        self.params_call(EP_PARAMS_RANK_SET, q, marker)
    }

    pub fn params_rank_clear(&self, q: &ClearRank, marker: &'static str) -> Promise<bool> {
        self.params_call(EP_PARAMS_RANK_CLEAR, q, marker)
    }

    pub fn params_search(&self, q: &ParamSearch, marker: &'static str) -> Promise<Vec<ParamOwner>> {
        self.params_call(EP_PARAMS_SEARCH, q, marker)
    }

    pub fn params_conflicts(
        &self,
        q: &ListConflicts,
        marker: &'static str,
    ) -> Promise<Vec<ParamConflict>> {
        self.params_call(EP_PARAMS_CONFLICTS, q, marker)
    }

    pub fn params_conflict_ack(
        &self,
        q: &AckConflict,
        marker: &'static str,
    ) -> Promise<ConflictAck> {
        self.params_call(EP_PARAMS_CONFLICT_ACK, q, marker)
    }
}

// The parameter vocabulary: PRM-11.

pub const EP_PARAMS_DEF_REVIEW: &str = "/params/def/review";
pub const EP_PARAMS_DEF_UNMERGE: &str = "/params/def/unmerge";
pub const EP_PARAMS_ALIAS_ADD: &str = "/params/alias/add";
pub const EP_PARAMS_ALIAS_EDIT: &str = "/params/alias/edit";
pub const EP_PARAMS_ALIAS_REMOVE: &str = "/params/alias/remove";
pub const EP_PARAMS_ALIAS_RESOLVE: &str = "/params/alias/resolve";
pub const EP_PARAMS_SOURCE_ADD: &str = "/params/source/add";
pub const EP_PARAMS_SOURCE_EDIT: &str = "/params/source/edit";
pub const EP_PARAMS_VOCAB_LOG: &str = "/params/vocab/log";

// Renormalizing stored values: PRM-12.

pub const EP_PARAMS_RENORMALIZE: &str = "/params/renormalize";

/// Re-derive current facts from what their source said.
///
/// Re-derive current facts from what their source said (`raw`, else the value) and how it named it (`ext`), through
/// today's aliases and parsers, as a crawl would store them now. The filters narrow which facts (AND-ed, none =
/// every current fact). Unchanged facts stay; a new value, warnings or condition supersedes the fact with one
/// `derived_from` it, at the same `at`; an alias pointing elsewhere moves it there. Facts without `ext` use the
/// source's alias of their definition. A dry run (the default) counts it all and changes nothing.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Renormalize {
    /// Facts of this source (key)
    #[serde(default)]
    pub source: Option<String>,
    /// Facts resolving under this definition (merged synonyms included)
    #[serde(default)]
    pub def: Option<DefRef>,
    /// Facts of this alias's source that it leads to, or that its source named by its id or name
    #[serde(default)]
    pub alias: Option<i32>,
    #[serde(default = "yes")]
    pub dry_run: bool,
    /// How many example changes per definition the answer carries
    #[serde(default = "five")]
    pub examples: u32,
}

fn yes() -> bool {
    true
}

fn five() -> u32 {
    5
}

/// What a renormalize did (or, on a dry run, would do).
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct Renormalized {
    pub dry_run: bool,
    /// Current facts looked at
    pub facts: u32,
    pub unchanged: u32,
    /// Superseded by a fact re-derived in place
    pub changed: u32,
    /// Moved to another definition or condition
    pub moved: u32,
    /// Written anew on a definition an `also` alias feeds
    #[serde(default)]
    pub added: u32,
    /// Retracted: an `also` alias no longer feeds the definition, or its parser no longer reads the text
    #[serde(default)]
    pub retracted: u32,
    /// Re-derived facts kept as text, unparsed (changed or not)
    pub unparsed: u32,
    /// Facts left alone because they could not be re-derived at all (empty text): the fact and why
    pub skipped: Vec<(FactId, String)>,
    /// Per definition the re-derived facts resolve under, by name
    pub defs: Vec<RenormDef>,
    /// Warning codes (the part before `:`) on the re-derived facts, how many carry each
    pub warnings: BTreeMap<String, u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RenormDef {
    pub def_id: ParameterDefId,
    pub name: String,
    pub key: Option<String>,
    pub facts: u32,
    pub unchanged: u32,
    pub changed: u32,
    /// Moved here from another definition or condition
    pub moved_in: u32,
    /// Written anew here through an `also` alias
    #[serde(default)]
    pub added: u32,
    /// Retracted here (see [`Renormalized::retracted`])
    #[serde(default)]
    pub retracted: u32,
    pub unparsed: u32,
    pub warnings: BTreeMap<String, u32>,
    /// A few of the changes, to see what a parser does
    pub examples: Vec<RenormExample>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RenormExample {
    pub fact: FactId,
    /// The definition it was stated under before, when it moved
    pub from_def: Option<ParameterDefId>,
    pub raw: Option<String>,
    pub before: Variant,
    pub after: Variant,
    pub cond: Cond,
    pub warnings: Vec<String>,
}

#[cfg(feature = "client")]
pub fn renormalize(c: &BlockingClient, q: &Renormalize) -> anyhow::Result<Renormalized> {
    c.post_json(q, EP_PARAMS_RENORMALIZE)
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn params_renormalize(
        &self,
        q: &Renormalize,
        marker: &'static str,
    ) -> Promise<Renormalized> {
        self.params_call(EP_PARAMS_RENORMALIZE, q, marker)
    }
}

/// A value type in words.
///
/// A value type in words, for clients that don't read rvariant: `(kind, unit, enum variants)`. Kinds: `empty`,
/// `bool`, `str`, `str_list`, `number`, `si`, `si_range`, `enum`, `multi_enum`, `binary`, `list`, `map`, `date`,
/// `time`, `datetime`, `instant`, `money`, `tolerance`.
pub fn ty_kind(ty: &VariantTy) -> (&'static str, Option<String>, Vec<String>) {
    match ty {
        VariantTy::Empty => ("empty", None, vec![]),
        VariantTy::Bool => ("bool", None, vec![]),
        VariantTy::Str => ("str", None, vec![]),
        VariantTy::StrList => ("str_list", None, vec![]),
        VariantTy::Number(_) => ("number", None, vec![]),
        VariantTy::SI { unit, .. } => ("si", Some(unit.to_string()), vec![]),
        VariantTy::SIRange { unit, .. } => ("si_range", Some(unit.to_string()), vec![]),
        VariantTy::Enum { variants, .. } => ("enum", None, variants.to_vec()),
        VariantTy::MultiSelectionEnum { variants, .. } => ("multi_enum", None, variants.to_vec()),
        VariantTy::Binary => ("binary", None, vec![]),
        VariantTy::List => ("list", None, vec![]),
        VariantTy::Map => ("map", None, vec![]),
        VariantTy::Date => ("date", None, vec![]),
        VariantTy::Time => ("time", None, vec![]),
        VariantTy::DateTime => ("datetime", None, vec![]),
        VariantTy::Instant => ("instant", None, vec![]),
        VariantTy::Money { currency, .. } => ("money", Some(currency.to_string()), vec![]),
        VariantTy::Tolerance(_) => ("tolerance", None, vec![]),
    }
}

/// A value type from the words of [`ty_kind`].
///
/// A value type from the words of [`ty_kind`], for a person or a script: `str`, `bool`, `date`, ..., `number`
/// (`number:u32`, default f64), `si:Ω` (`si:f64:kΩ`, default f32), `si_range:V`, `money:EUR`, `tolerance`,
/// `enum:Name:a|b|c`, `multi_enum:Name:a|b`; or rvariant's JSON (`{"Number":"U32"}`).
pub fn ty_parse(s: &str) -> Result<VariantTy, String> {
    use rvariant::NumberTy;
    use rvariant::si_dynamic::Registry;
    let s = s.trim();
    if s.starts_with('{') || s.starts_with('"') {
        return serde_json::from_str(s).map_err(|e| format!("type JSON: {e}"));
    }
    let (kind, rest) = s.split_once(':').unwrap_or((s, ""));
    let number_ty = |n: &str| -> Result<NumberTy, String> {
        n.to_ascii_uppercase()
            .parse()
            .map_err(|e| format!("number type `{n}` ({e}): u8..u64, i8..i64, f32, f64"))
    };
    // `si:Ω` or `si:f64:Ω`
    let si = |rest: &str| -> Result<(NumberTy, rvariant::si_dynamic::UnitExpr), String> {
        let (n, unit) = match rest.split_once(':') {
            Some((n, unit)) => (number_ty(n)?, unit),
            None => (NumberTy::F32, rest),
        };
        let unit = Registry::new()
            .parse_unit(unit)
            .map_err(|e| format!("unit `{unit}`: {e}"))?;
        if unit.dim().is_none() {
            return Err(format!("unit `{unit}`: not a unit si_dynamic knows"));
        }
        Ok((n, unit))
    };
    // `Name:a|b|c`
    let variants = |rest: &str| -> Result<(std::sync::Arc<str>, std::sync::Arc<[String]>), String> {
        let (name, list) = rest
            .split_once(':')
            .ok_or_else(|| format!("`{kind}:Name:a|b|c`"))?;
        let list: Vec<String> = list.split('|').map(|v| v.trim().to_owned()).collect();
        Ok((name.into(), list.into()))
    };
    let no_rest = |ty: VariantTy| {
        if rest.is_empty() {
            Ok(ty)
        } else {
            Err(format!("type `{kind}` takes no `:{rest}`"))
        }
    };
    match kind {
        "empty" => no_rest(VariantTy::Empty),
        "bool" => no_rest(VariantTy::Bool),
        "str" => no_rest(VariantTy::Str),
        "str_list" => no_rest(VariantTy::StrList),
        "binary" => no_rest(VariantTy::Binary),
        "list" => no_rest(VariantTy::List),
        "map" => no_rest(VariantTy::Map),
        "date" => no_rest(VariantTy::Date),
        "time" => no_rest(VariantTy::Time),
        "datetime" => no_rest(VariantTy::DateTime),
        "instant" => no_rest(VariantTy::Instant),
        "number" if rest.is_empty() => Ok(VariantTy::Number(NumberTy::F64)),
        "number" => Ok(VariantTy::Number(number_ty(rest)?)),
        "tolerance" if rest.is_empty() => Ok(VariantTy::Tolerance(NumberTy::F32)),
        "tolerance" => Ok(VariantTy::Tolerance(number_ty(rest)?)),
        "si" => {
            let (number_ty, unit) = si(rest)?;
            Ok(VariantTy::SI { number_ty, unit })
        }
        "si_range" => {
            let (number_ty, unit) = si(rest)?;
            Ok(VariantTy::SIRange {
                number_ty,
                start_inclusive: true,
                end_inclusive: true,
                unit,
            })
        }
        "money" if !rest.is_empty() => Ok(VariantTy::Money {
            currency: rest.into(),
            precision: 2,
        }),
        "enum" => {
            let (name, variants) = variants(rest)?;
            Ok(VariantTy::Enum { name, variants })
        }
        "multi_enum" => {
            let (name, variants) = variants(rest)?;
            Ok(VariantTy::MultiSelectionEnum { name, variants })
        }
        _ => Err(format!(
            "type `{s}`: str, bool, number[:u32], si:UNIT, si_range:UNIT, money:CUR, tolerance, \
             enum:Name:a|b, multi_enum:Name:a|b, str_list, date, time, datetime, or rvariant JSON"
        )),
    }
}

/// What a person decides about a definition.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum DefVerdict {
    /// A proposed definition becomes curated: its key is one clients rely on
    Accept,
    /// A proposed definition is retired; kept with its history
    Reject,
    /// Back to proposed (a curated or retired definition reopened, a raw one put up for review)
    Propose,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReviewDef {
    pub def: ParameterDefId,
    pub verdict: DefVerdict,
    /// Why
    #[serde(default)]
    pub note: String,
}

/// Take a merged synonym out of its merge: its facts resolve under itself again.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnmergeDef {
    pub def: ParameterDefId,
    /// None = `raw`
    #[serde(default)]
    pub status: Option<DefStatus>,
    #[serde(default)]
    pub note: String,
}

/// How a source names a definition: by its name, by its id, or both; `ext_scope` is what the name or id is unique
/// within (a Digi-Key category, a Mouser product type), "" = everywhere.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddAlias {
    pub def: DefRef,
    /// The source's key
    pub source: String,
    #[serde(default)]
    pub ext_name: Option<String>,
    #[serde(default)]
    pub ext_id: Option<String>,
    #[serde(default)]
    pub ext_scope: String,
    /// How the source's text becomes a value (PRM-12); None = the definition's type
    #[serde(default)]
    pub parser: Option<String>,
    #[serde(default)]
    pub parser_args: Option<serde_json::Value>,
    /// A change of its facts is worth a notice
    #[serde(default)]
    pub notify: bool,
    /// A second landing of the field (see [`ParamAlias::also`])
    #[serde(default)]
    pub also: bool,
    /// Why
    #[serde(default)]
    pub note: String,
}

/// Change an alias; a field left None stays. `def` moves the alias to another definition.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct EditAlias {
    pub alias: i32,
    #[serde(default)]
    pub def: Option<DefRef>,
    #[serde(default)]
    pub ext_name: Option<String>,
    #[serde(default)]
    pub ext_id: Option<String>,
    #[serde(default)]
    pub ext_scope: Option<String>,
    #[serde(default)]
    pub parser: Option<String>,
    #[serde(default)]
    pub parser_args: Option<serde_json::Value>,
    #[serde(default)]
    pub notify: Option<bool>,
    /// Make it a second landing of the field, or a plain alias again (see [`ParamAlias::also`])
    #[serde(default)]
    pub also: Option<bool>,
    /// Why: kept as the alias's note and in the log
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoveAlias {
    pub alias: i32,
    #[serde(default)]
    pub note: String,
}

/// Which definition a source means by a name or an id.
///
/// Which definition a source means by a name or an id, within scopes. An alias of a more specific scope wins (the
/// first of `ext_scopes`, then the next, then one valid everywhere), then one matching both id and name over one
/// matching either, an id over a name; without an alias a definition of that name.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ResolveAlias {
    pub source: String,
    #[serde(default)]
    pub ext_name: Option<String>,
    #[serde(default)]
    pub ext_id: Option<String>,
    /// Where the name or id was met, most specific first (a Digi-Key leaf category, then its parents)
    #[serde(default)]
    pub ext_scopes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AliasResolved {
    /// The definition the source's name resolves under (the merge target of a synonym)
    pub def: ParamDef,
    /// The alias that matched; None = a definition of that name
    pub alias: Option<ParamAlias>,
    /// Other aliases of the source that match as well (PRM-8 collisions), for a person to resolve
    pub ambiguous: Vec<ParamAlias>,
    /// The `also` aliases that match: the definitions the field states as well
    #[serde(default)]
    pub also: Vec<ParamAlias>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddSource {
    /// `snake_case`, unique
    pub key: String,
    pub label: String,
    pub default_rank: i32,
    pub kind: SourceKind,
    /// API scope a caller needs to write its facts; "" = the backend only
    #[serde(default)]
    pub writable_by: String,
    #[serde(default)]
    pub note: String,
}

/// Change a source; a field left None stays.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct EditSource {
    pub source: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub default_rank: Option<i32>,
    #[serde(default)]
    pub kind: Option<SourceKind>,
    #[serde(default)]
    pub writable_by: Option<String>,
    /// Why: kept as the source's note and in the log
    #[serde(default)]
    pub note: Option<String>,
}

/// The vocabulary log, newest first.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct VocabLogQuery {
    #[serde(default)]
    pub def: Option<ParameterDefId>,
    /// A source's key
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
    /// None = 200
    #[serde(default)]
    pub limit: Option<i64>,
}

/// One change to the vocabulary: what, the row before and after, who and why.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VocabChange {
    pub id: i64,
    pub at: DateTime<Utc>,
    /// `def_create`, `def_edit`, `def_accept`, `def_reject`, `def_propose`, `def_merge`, `def_unmerge`, `alias_add`,
    /// `alias_edit`, `alias_remove`, `source_add`, `source_edit`, `rank_set`, `rank_clear`
    pub action: String,
    pub def_id: Option<ParameterDefId>,
    pub alias_id: Option<i32>,
    pub source_id: Option<SourceId>,
    pub rank_id: Option<i32>,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    pub note: String,
    pub by: String,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Harvester => "harvester",
            Self::Import => "import",
            Self::Derivation => "derivation",
        }
    }
}

#[cfg(feature = "client")]
pub fn def_review(c: &BlockingClient, q: &ReviewDef) -> anyhow::Result<ParamDef> {
    c.post_json(q, EP_PARAMS_DEF_REVIEW)
}

#[cfg(feature = "client")]
pub fn def_unmerge(c: &BlockingClient, q: &UnmergeDef) -> anyhow::Result<ParamDef> {
    c.post_json(q, EP_PARAMS_DEF_UNMERGE)
}

#[cfg(feature = "client")]
pub fn alias_add(c: &BlockingClient, q: &AddAlias) -> anyhow::Result<ParamAlias> {
    c.post_json(q, EP_PARAMS_ALIAS_ADD)
}

#[cfg(feature = "client")]
pub fn alias_edit(c: &BlockingClient, q: &EditAlias) -> anyhow::Result<ParamAlias> {
    c.post_json(q, EP_PARAMS_ALIAS_EDIT)
}

#[cfg(feature = "client")]
pub fn alias_remove(c: &BlockingClient, q: &RemoveAlias) -> anyhow::Result<ParamAlias> {
    c.post_json(q, EP_PARAMS_ALIAS_REMOVE)
}

/// None when nothing of that name or id is known to the source.
#[cfg(feature = "client")]
pub fn alias_resolve(
    c: &BlockingClient,
    q: &ResolveAlias,
) -> anyhow::Result<Option<AliasResolved>> {
    c.post_json(q, EP_PARAMS_ALIAS_RESOLVE)
}

#[cfg(feature = "client")]
pub fn source_add(c: &BlockingClient, q: &AddSource) -> anyhow::Result<ParamSource> {
    c.post_json(q, EP_PARAMS_SOURCE_ADD)
}

#[cfg(feature = "client")]
pub fn source_edit(c: &BlockingClient, q: &EditSource) -> anyhow::Result<ParamSource> {
    c.post_json(q, EP_PARAMS_SOURCE_EDIT)
}

#[cfg(feature = "client")]
pub fn vocab_log(c: &BlockingClient, q: &VocabLogQuery) -> anyhow::Result<Vec<VocabChange>> {
    c.post_json(q, EP_PARAMS_VOCAB_LOG)
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn params_def_review(&self, q: &ReviewDef, marker: &'static str) -> Promise<ParamDef> {
        self.params_call(EP_PARAMS_DEF_REVIEW, q, marker)
    }

    pub fn params_def_unmerge(&self, q: &UnmergeDef, marker: &'static str) -> Promise<ParamDef> {
        self.params_call(EP_PARAMS_DEF_UNMERGE, q, marker)
    }

    pub fn params_alias_add(&self, q: &AddAlias, marker: &'static str) -> Promise<ParamAlias> {
        self.params_call(EP_PARAMS_ALIAS_ADD, q, marker)
    }

    pub fn params_alias_edit(&self, q: &EditAlias, marker: &'static str) -> Promise<ParamAlias> {
        self.params_call(EP_PARAMS_ALIAS_EDIT, q, marker)
    }

    pub fn params_alias_remove(
        &self,
        q: &RemoveAlias,
        marker: &'static str,
    ) -> Promise<ParamAlias> {
        self.params_call(EP_PARAMS_ALIAS_REMOVE, q, marker)
    }

    pub fn params_source_add(&self, q: &AddSource, marker: &'static str) -> Promise<ParamSource> {
        self.params_call(EP_PARAMS_SOURCE_ADD, q, marker)
    }

    pub fn params_source_edit(&self, q: &EditSource, marker: &'static str) -> Promise<ParamSource> {
        self.params_call(EP_PARAMS_SOURCE_EDIT, q, marker)
    }

    pub fn params_vocab_log(
        &self,
        q: &VocabLogQuery,
        marker: &'static str,
    ) -> Promise<Vec<VocabChange>> {
        self.params_call(EP_PARAMS_VOCAB_LOG, q, marker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ty_words_round_trip() {
        for s in [
            "str",
            "bool",
            "si:Ω",
            "si:V",
            "si_range:A",
            "enum:Lifecycle:Active|Obsolete",
            "money:EUR",
            "tolerance",
            "number",
        ] {
            let ty = ty_parse(s).unwrap_or_else(|e| panic!("{s}: {e}"));
            let (kind, unit, variants) = ty_kind(&ty);
            assert_eq!(kind, s.split(':').next().unwrap(), "{s}");
            if kind.starts_with("si") {
                assert_eq!(unit.as_deref(), s.split(':').nth(1), "{s}");
            }
            if kind == "enum" {
                assert_eq!(variants, ["Active", "Obsolete"]);
            }
        }
        assert_eq!(
            ty_parse("si:f64:kΩ").unwrap(),
            serde_json::from_str::<VariantTy>(
                r#"{"SI":{"number_ty":"F64","unit":{"Unit":{"prefix":"Kilo","base":"Ohm","exp":1}}}}"#
            )
            .unwrap()
        );
        assert_eq!(
            ty_parse(r#"{"Number":"U32"}"#).unwrap(),
            ty_parse("number:u32").unwrap()
        );
        assert!(ty_parse("si:furlong").is_err());
        assert!(ty_parse("bool:x").is_err());
        assert!(ty_parse("enum:NoVariants").is_err());
    }
}
