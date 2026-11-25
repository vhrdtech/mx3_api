# mx3_api_core

Wire types every domain of the mx3 API shares: the body of a failed answer (`ErrorBody`), `Money` in fixed point, the file export request and report, and the API level clients send.

Types only: no I/O, threads or async runtime. It depends on serde, serde_json and base64; the `client` feature adds the request headers (reqwest types), `sqlx` the conversions to PostgreSQL `money`.
