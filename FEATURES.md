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

## Dropped and superseded

None yet.
