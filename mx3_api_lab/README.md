# mx3_api_lab

The electronics lab API of mx3: request and response types for parts, parameters (incl. values by MPN), MPN schemes, BOMs and their revisions, PCBs and serials, footprint reviews, supplier search, warehouse and orders, plus the BOM logic clients share (filters, cost, import, assembly planning).

Depends on `mx3_api_core`, serde, chrono, rvariant (parameter values) and mpn_schemes. With no features it stays free of reqwest, tokio and sqlx:

- `client`: `blocking::BlockingClient`, and `MxClient` whose calls answer with a polled `Promise` (needs a tokio runtime).
- `sqlx`: `sqlx::Type` on the id newtypes and enums, for a backend.
