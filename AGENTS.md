# Working on mx3_api

Guidance for AI agents and contributors. Read this before changing code.

mx3_api holds the public wire types and clients of the mx3 lab management API. The mx3 backend, desktop app and CLI build on it, and so do outside tools such as better_erc. A change to a type here is a change to the wire: the backend and every client must agree on it.

## FEATURES.md is the source of truth

[FEATURES.md](FEATURES.md) lists every feature with its status, every known bug and what is planned, with stable IDs per area (`CORE-1`, `LAB-3`).

- Read the relevant area before starting.
- Name IDs with a short slug when talking to people (`LAB-3 clients`); commit messages and CHANGELOG keep the bare ID.
- Update it in the same commit as the code. Never renumber or reuse IDs.
- Planned work in code is `TODO(ID)` with an ID; no other TODO comments.

## CHANGELOG.md records every change

- Every change a user of the crates would notice gets one line under `## [Unreleased]`: `Added`, `Changed`, `Fixed`, `Removed`, with the ID in parentheses, 20 words at most.
- A wire change (field added, renamed, removed) says so: the mx3 backend and clients update together.
- Pure refactors and typo fixes need no entry.

## Layout

Cargo workspace, edition 2024, one shared version.

- `mx3_api_core/` — types every domain shares. Types only: no I/O, threads or async runtime.
- `mx3_api_lab/src/endpoints/` — request and response types per endpoint area; the client calls for an area sit next to its types behind `#[cfg(feature = "client")]`.
- `mx3_api_lab/src/bom/` — BOM logic clients share (filters, cost, import, revisions, export, assembly).
- `mx3_api_lab/src/{warehouse,orders,supplier_search}/` — those areas' models and pure logic.
- `mx3_api_lab/src/{blocking,blocking_api,mx_client,promise,request}.rs` — the clients (`client` feature only).

## Features

Both crates: `client` and `sqlx`, off by default. With no features a crate must stay serde types and pure logic: no reqwest, tokio or sqlx in `cargo tree -p mx3_api_lab -e normal,build`. Downstream tools take the crate with no features and turn on `client` behind their own feature.

- New client code: `#[cfg(feature = "client")]` on the impl block, function or import.
- New id newtype or enum stored in PostgreSQL: `#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(...))]`.
- Check all three builds: `just check-features`.

## Commands

```sh
just                  # list recipes
just test             # every test, with and without features
just lint             # fmt, clippy (no warnings) in each feature set, spl check
just jobs=4 test      # cap build and test parallelism
just check-features   # build with no features, each feature, all features
```

Before declaring a change done: build, clippy without warnings, fmt, tests, `spl check`.

## Dependencies

- Every dependency comes from `[workspace.dependencies]`, with `default-features = false` where the crate has defaults worth dropping.
- TLS is rustls only: reqwest with `rustls-tls`, never `default-tls` or OpenSSL.
- rvariant and mpn_schemes come by git at the tags mx3 uses, so mx3 resolves one copy of each; move them together with mx3.
- `si_dynamic` 0.3 (under rvariant) is not on crates.io: the root `[patch.crates-io]` takes it by git, and consumers patch it the same way.
- C build steps in the graph, allowed in `spirit_level.toml`: `cc` (psm under pest, through si_dynamic's parser) and `ring` (rustls, `client` only); `wasm-bindgen-shared` sits in the lock for chrono's wasm support and is not built on other targets.

## Code conventions

- Match the surrounding style. Public items are documented; the first doc paragraph is one short sentence.
- No `unwrap` on data from the network or the user; `expect("why it cannot fail")` only where it cannot.
- Prices use `Money`, parameter values `rvariant::VariantTy` with SI units; no numbers as formatted strings.
- A lint exception is `#[expect(lint, reason = "...")]`, never `#[allow]`.
- Public text is neutral: no host names, people, customers or internal board names in code, tests or docs. Test fixtures use placeholders (`B100`, `Widget`) and public part numbers.

## Tests

- Pure logic (BOM cost, picks, revisions, parsing, price breaks): plain unit tests next to the code.
- A bug fix starts with a failing test when the bug can be reproduced in one.
- The mx3 repository runs the backend and client tests against a database; this repo has no database tests.

## Commits

Conventional Commits with a scope: `feat(lab): ...`, `fix(core): ...`, `docs: ...`, `build: ...`. Short imperative subject, blank line, body with what and why, and a `Tested:` line.

## Versions

One version for the workspace in `[workspace.package]`. A release moves the `[Unreleased]` entries under `## [x.y.z] - YYYY-MM-DD` and bumps the version in one commit: minor for added or changed types, patch for fixes. Until a first release, consumers take the default branch by git and Cargo.lock pins the commit.
