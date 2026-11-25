//! Parametric part search at suppliers (SRCH-4): the filters a person narrows by on Digi-Key's site (category,
//! package, stock, lifecycle, parameters), then the cheapest at a quantity. Pure pieces shared by the backend
//! and the CLI; the request and answer types are in `endpoints::supplier_search`.

pub mod filter;
pub mod price;
