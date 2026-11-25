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

## Lab (LAB)

- **LAB-1 lab-endpoint-types** (done): parts, parameters and facts incl. values by MPN, MPN schemes and decoding, BOMs, BOM chains and revisions, PCBs and serials, footprint reviews, companies, supplier search and substitutes, market prices, crawler, warehouse, orders. `mx3_api_lab/src/endpoints/`, `warehouse/`, `orders/`, `supplier_search/`.
- **LAB-2 shared-bom-logic** (done): BOM filters, cost, import and part picks, revision carry-over, spreadsheet export, part suggestions, assembly planning. `mx3_api_lab/src/bom/`.
- **LAB-3 clients** (done): `client` feature: `BlockingClient` with typed calls (`blocking_api.rs` and free functions per area) and the async `MxClient` answering with a polled `Promise`. `mx3_api_lab/src/blocking.rs`, `mx_client.rs`, `promise.rs`.
- **LAB-4 sqlx-types** (done): `sqlx` feature: `sqlx::Type` on the id newtypes and enums. `mx3_api_lab/src/endpoints/`.
- **LAB-5 library-types** (in progress): the document library's wire types: content hashes, uploads of extracted text, identity readings, stored text, catalog sync and its report, search, the docs list with a snapshot position, stats; limits as constants and checks for hashes, shares and paths. Application ids LIB-1. `mx3_api_lab/src/endpoints/library.rs`. `mx3_api_lab/src/library/ident.rs`: which document a file is, by vendor rules and the rules every vendor shares (tpm's LIB-4/LIB-9, moved here), and which of two revisions is newer. `mx3_api_lab/src/library/mpn.rs`: the documents of a part number (by link, by origin URL, by file name) and their flat rows; `roots.rs`: catalog paths on this computer; `site.rs`: redirect pages seen through. `mx3_api_lab/src/endpoints/library_links.rs`: the links a fetch by part number records. Missing: the coverage types.

## Dropped and superseded

None yet.
