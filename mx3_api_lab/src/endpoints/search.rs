#[cfg(feature = "client")]
use crate::endpoints::api::{SearchDigiKeyProductDetails, SearchDigiKeyProductDetailsResult};
use crate::endpoints::params::{DefRef, ParamSearch, SearchOp, SearchOwner, SearchTerm};
#[cfg(feature = "client")]
use crate::endpoints::part::Part;
#[cfg(feature = "client")]
use crate::request::RequestInitiator;
#[cfg(feature = "client")]
use crate::{MxClient, Promise};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParametricSearch {
    pub kind: ParametricSearchKind,
    pub warehouse: bool,
    // previously_used: bool,
    pub suppliers: bool,
    pub mpn_contains: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeywordSearch {
    pub keyword: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParametricSearchKind {
    Any,
    /// A search on facts (SRCH-7), definitions chosen by the caller: the only kind the backend runs besides `Any`
    Facts(ParamSearch),
    Capacitor {
        capacitance_farads: f32,
        voltage_volts: f32,
        is_voltage_eq: bool,
    },
    Resistor {
        resistance_ohms: f32,
    },
}

impl ParametricSearchKind {
    /// The search on facts a resistor or capacitor search stands for, with the definitions named here by curated key, on the
    /// caller's side: the backend names no parameter. Other kinds are returned as they are.
    pub fn lower(self) -> ParametricSearchKind {
        let term = |key: &str, op, tolerance| SearchTerm {
            def: DefRef::Key(key.into()),
            op,
            tolerance,
        };
        let terms = match self {
            ParametricSearchKind::Resistor { resistance_ohms } => {
                // the definition's own tolerance, in f64
                vec![term(
                    "resistance",
                    SearchOp::Eq(f64::from(resistance_ohms)),
                    None,
                )]
            }
            ParametricSearchKind::Capacitor {
                capacitance_farads,
                voltage_volts,
                is_voltage_eq,
            } => {
                let voltage = f64::from(voltage_volts);
                vec![
                    term(
                        "capacitance",
                        SearchOp::Eq(f64::from(capacitance_farads)),
                        Some(0.001),
                    ),
                    term(
                        "rated_voltage",
                        if is_voltage_eq {
                            SearchOp::Eq(voltage)
                        } else {
                            SearchOp::Ge(voltage)
                        },
                        Some(0.01),
                    ),
                ]
            }
            other => return other,
        };
        ParametricSearchKind::Facts(ParamSearch {
            terms,
            owner: SearchOwner::Part,
            in_warehouse: false,
        })
    }
}

impl ParametricSearch {
    /// This search with its kind lowered to facts ([`ParametricSearchKind::lower`]).
    pub fn lowered(self) -> ParametricSearch {
        ParametricSearch {
            kind: self.kind.lower(),
            ..self
        }
    }
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn search_dk_product_details(
        &self,
        query: SearchDigiKeyProductDetails,
        marker: &'static str,
    ) -> Promise<SearchDigiKeyProductDetailsResult> {
        let (req, rx) = RequestInitiator::post_json_arg("search_dk_product_details".into(), &query);
        self.send_req(req, rx, marker)
    }

    pub fn parametric_search(
        &self,
        query: &ParametricSearch,
        marker: &'static str,
    ) -> Promise<Vec<Part>> {
        let query = query.clone().lowered();
        let (req, rx) = RequestInitiator::post_json_arg("parametric_search".into(), &query);
        self.send_req(req, rx, marker)
    }

    pub fn keyword_search(&self, query: &KeywordSearch, marker: &'static str) -> Promise<()> {
        let (req, rx) = RequestInitiator::post_json_arg("keyword_search".into(), query);
        self.send_req(req, rx, marker)
    }
}
