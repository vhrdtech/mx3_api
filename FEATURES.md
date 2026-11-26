# Features

The source of truth for what these crates do, their known bugs and what is planned. IDs are stable per area and never reused: `CORE` for `mx3_api_core`, `LAB` for `mx3_api_lab`. Status: done, in progress, bug, planned.

Code comments also name IDs of the mx3 application's own tracker (`PRM-17`, `BOM-29`, `WH-25`...): those say which application feature a type serves.

## Crates

| Crate | Role | May depend on |
|---|---|---|
| `mx3_api_core` | Wire types every domain shares | serde, serde_json, base64; reqwest (`client`), sqlx (`sqlx`) |
| `mx3_api_lab` | Lab endpoint types, BOM logic, clients | `mx3_api_core`, serde, chrono, rvariant, mpn_schemes; reqwest, tokio (`client`), sqlx (`sqlx`) |

## Core (CORE)

- **CORE-1 shared-wire-types** (done): `ErrorBody` and `error_message`, `Money`, `files::ExportRequest`/`ExportReport`, `base64_bytes`, `API_LEVEL` and its header. `mx3_api_core/src/`.
- **CORE-2 features-off-by-default** (done): `client` adds `client_headers()`, `sqlx` the `Money` and PostgreSQL `money` conversions; with none the crate is serde types only. `mx3_api_core/src/lib.rs`.
- **CORE-3 datasheet-link** (done): the `mx3:` link format of a KiCad `Datasheet` field (`mx3:${Mpn1} ${Mpn2}`): `is_link`, `parse` into part numbers (percent escapes decoded, white space apart, unresolved references and `~` dropped, at most `LINK_BYTES_MAX` bytes and `LINK_MPNS_MAX` part numbers), `resolve` and `parse_resolved` filling `${Field}` references from a symbol's fields. `mx3_api_core/src/datasheet_link.rs`.

## Lab (LAB)

- **LAB-1 lab-endpoint-types** (done): parts, parameters and facts incl. values by MPN, MPN schemes and decoding, BOMs, BOM chains and revisions, PCBs and serials, footprint reviews, companies, supplier search and substitutes, market prices, crawler, warehouse, orders. `mx3_api_lab/src/endpoints/`, `warehouse/`, `orders/`, `supplier_search/`.
- **LAB-2 shared-bom-logic** (done): BOM filters, cost, import and part picks, revision carry-over, spreadsheet export, part suggestions, assembly planning. `mx3_api_lab/src/bom/`.
- **LAB-3 clients** (done): `client` feature: `BlockingClient` with typed calls (`blocking_api.rs` and free functions per area) and the async `MxClient` answering with a polled `Promise`. `mx3_api_lab/src/blocking.rs`, `mx_client.rs`, `promise.rs`.
- **LAB-4 sqlx-types** (done): `sqlx` feature: `sqlx::Type` on the id newtypes and enums. `mx3_api_lab/src/endpoints/`.
- **LAB-5 library-types** (in progress): the document library's wire types: content hashes, uploads of extracted text, identity readings, stored text, catalog sync and its report, search, the docs list with a snapshot position, stats; limits as constants and checks for hashes, shares, paths and an origin's web address (`valid_web_url`, `Origin::web_only`). Application ids LIB-1. `mx3_api_lab/src/endpoints/library.rs`. `mx3_api_lab/src/library/ident.rs`: which document a file is, by vendor rules and the rules every vendor shares (tpm's LIB-4/LIB-9, moved here), and which of two revisions is newer. `mx3_api_lab/src/library/mpn.rs`: the documents of a part number (by link, by origin URL, by file name) and their flat rows; `roots.rs`: catalog paths on this computer; `site.rs`: redirect pages seen through. `mx3_api_lab/src/endpoints/lib_cover.rs`: which parts a document covers (a part, a pattern, a decoded family), covers taken back, derivation; `mx3_api_lab/src/endpoints/library_links.rs`: the links a fetch by part number records. `mpn::lookup` takes the covers as four more ways (`cover`, `family`, `pattern`, `text`).
- **LAB-6 pcb-description** (done): `SetPcbDescription`, `EP_PCB_SET_DESCRIPTION`, `set_pcb_description`. `mx3_api_lab/src/endpoints/pcb.rs`.
- **LAB-7 parts-by-maker** (in progress): one part per maker. `CompaniesMap::find_maker` and `Company::maker_matches` match a maker name exact or loose (case, punctuation, "Inc."/"USA" dropped) through synonyms; `LookupPartResult::NotUnique` names each part's maker (`MpnMaker`); `MpnResult.makers` and `MpnResult.maker` (`MakerMatch`). `mx3_api_lab/src/endpoints/companies.rs`, `part.rs`, `params.rs`.

## Dropped and superseded

None yet.
