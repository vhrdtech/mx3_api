# Changelog

All notable changes to this project are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `Part.merged_into` and `PartMerged.frozen_lines`: a merged stub is retired, not deleted. Wire change (LAB-11).
- `Watched`, `Unwatch` and `EP_PART_WATCHED`, `EP_PART_UNWATCH`: list and stop crawler watches (LAB-8).
- `Stubs`, `PartStubs` and `EP_PART_STUBS`: maker-less parts beside a part with a maker (LAB-9).
- `MergePart`, `PartMerged` and `EP_PART_MERGE`: merge a maker-less stub into the part with a maker (LAB-10).
- `CompaniesMap::find_maker`, `Company::maker_matches`: a maker name matched exact or loose through synonyms (LAB-7).
- `MpnResult.makers` (every part of the MPN with its maker) and `MpnResult.maker` (`MakerMatch`). Wire change (LAB-7).
- `mx3_api_core::datasheet_link`: reads `mx3:` links of KiCad Datasheet fields into part numbers, filling `${Field}` references (CORE-3).
- `valid_web_url` and `Origin::web_only`: the one rule for an origin's web address; a `file://` URL is none (LAB-5).
- `SetPcbDescription`, `EP_PCB_SET_DESCRIPTION` and `set_pcb_description` to change a PCB's description (LAB-6).
- `mx3_api_core`: error body, money, file export report and API level, split out of mx3 (CORE-1).
- `mx3_api_lab`: lab endpoint types, BOM logic and both clients, split out of mx3 (LAB-1).
- `client` and `sqlx` features, off by default: with none, serde types only (CORE-2).
- `also` on `ParamAlias`, `AddAlias`, `EditAlias`; `also` aliases in `AliasResolved`; `added` and `retracted` renormalize counts. Wire change (LAB-1).
- `mx3_api_lab::endpoints::library`: the document library's types and client calls: upload, have, text, catalog sync, search, docs, stats (LAB-5).
- `mx3_api_lab::library::ident`: a document's number, revision and date from its text, and revision order; `Ident` and `Src` serialize (LAB-5).
- Library lookup by part number: `/lib/by_mpn` types and client call, ranking, contract rows, local roots. Wire change (LAB-5).
- `MpnDocs.known` and `MpnDocs.urls`: whether mx3 holds the part and its datasheet URLs. Wire change (LAB-5).
- `/lib/links` types and client call: record which content a part number led to. Wire change (LAB-5).
- Library coverage types and calls; `By` gains `cover`, `family`, `pattern`, `text`. Wire change (LAB-5).

### Changed

- `LookupPartResult::NotUnique` carries `MpnMaker` (part and maker name) instead of bare part ids. Wire change (LAB-7).
- Clients send `User-Agent: mx3_api/<version>`, the library's version, not the application's (CORE-2).
