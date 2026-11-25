# mx3_api

Wire types and clients of the mx3 lab management API, for tools that read or write parts, parameters, BOMs and PCBs.

| Crate | What it holds |
|---|---|
| [`mx3_api_core`](mx3_api_core) | Types every domain shares: the error body, money, the file export report, the API level |
| [`mx3_api_lab`](mx3_api_lab) | The electronics lab: parts, parameters (incl. by MPN), MPN schemes, BOMs, PCBs and serials, footprint reviews, suppliers, warehouse, orders |

## Features

Both crates have the same two features, off by default. With none, a crate is serde types and pure logic only: no
reqwest, tokio or sqlx in its tree.

| Feature | Adds |
|---|---|
| `client` | `BlockingClient` and the async `MxClient` (reqwest with rustls, no OpenSSL); `MxClient` needs a tokio runtime |
| `sqlx` | `sqlx::Type` on id newtypes and enums, `Money` to and from PostgreSQL `money` |

## Use

```toml
[dependencies]
mx3_api_lab = { git = "https://github.com/vhrdtech/mx3_api" }                         # types only
mx3_api_lab = { git = "https://github.com/vhrdtech/mx3_api", features = ["client"] }  # with clients

# rvariant (a dependency) takes si_dynamic 0.3, which is not on crates.io yet:
[patch.crates-io]
si_dynamic = { git = "https://github.com/romixlab/napier", tag = "v0.3.0" }
```

```rust,ignore
use mx3_api_lab::blocking::BlockingClient;
use mx3_api_lab::endpoints::params::{self, ByMpn};

let client = BlockingClient::new("http://localhost:3000".to_owned());
let answer = params::by_mpn(&client, &ByMpn { mpns, keys: vec!["capacitance".to_owned()], ..Default::default() })?;
```

## Licence

MIT, see [LICENSE](LICENSE).
