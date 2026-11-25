use crate::endpoints::companies::CompaniesMap;
use crate::endpoints::part::{Part, PartId};
use crate::endpoints::serial::SerialId;
#[cfg(feature = "client")]
use crate::request::RequestInitiator;
use crate::warehouse::models::WarehouseBox;
#[cfg(feature = "client")]
use crate::{MxClient, Promise};
use chrono::Duration;
use rvariant::Variant;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct BoxId(pub i32);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct BoxItemId(pub i32);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxSerial(pub String);

#[derive(Serialize, Deserialize)]
pub struct BoxFilter {
    pub contains_part: Option<PartId>,
    pub description_contains: Option<String>,
    pub limit: u32,
}

#[derive(Serialize, Deserialize)]
pub struct Boxes {
    pub boxes: Vec<WarehouseBox>,
    pub referenced_parts: HashMap<PartId, Part>,
    pub referenced_companies: CompaniesMap,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BoxItemMove {
    ToSerial {
        from_box: BoxId,
        part_id: PartId,
        count: u32,
        to_serial: SerialId,
        designator: String,
    },
    // ToBox {
    //     from_box: BoxId,
    //     part_id: PartId,
    //     count: u32,
    //     to_box: BoxId,
    // },
    ToNewBox {
        from_box: BoxId,
        part_id: PartId,
        count: u32,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BoxItemMoveResult {
    Moved,
    BoxCreated {
        box_id: BoxId,
        box_serial: BoxSerial,
    },
}

#[derive(Serialize, Deserialize)]
pub struct WatchParts {
    pub part_ids: Vec<PartId>,
    pub interval: Duration,
}

#[derive(Serialize, Deserialize)]
pub struct RegisterStockReq {
    pub existing_box_serial: String,
    pub manufacturer: String,
    pub mpn: String,
    /// Only set if a part is being created
    pub is_electronic: bool,
    /// Return an error if part with this name already exists
    pub require_new_part: bool,
    pub quantity: u32,
    /// set parent_box_id to id of this box serial for the newly created or existing box
    pub move_box_into_box: Option<String>,
    pub set_part_params: Vec<(String, Variant)>,
    pub set_box_item_params: Vec<(String, Variant)>,
}

#[derive(Serialize, Deserialize)]
pub struct RegisterStockResult {
    pub part_id: PartId,
    pub serial: BoxSerial,
}

#[derive(Serialize, Deserialize)]
pub enum BoxTreeFilter {
    All,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BoxTreeItem {
    pub id: BoxId,
    pub serial: String,
    pub children: Vec<BoxTreeItem>,
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn get_boxes(&self, filter: &BoxFilter, marker: &'static str) -> Promise<Boxes> {
        let (req, rx) = RequestInitiator::post_json_arg("get_boxes".into(), filter);
        self.send_req(req, rx, marker)
    }

    pub fn register_stock(
        &self,
        q: &RegisterStockReq,
        marker: &'static str,
    ) -> Promise<RegisterStockResult> {
        let (req, rx) = RequestInitiator::post_json_arg("register_stock".into(), q);
        self.send_req(req, rx, marker)
    }

    pub fn box_move(&self, m: &BoxItemMove, marker: &'static str) -> Promise<BoxItemMoveResult> {
        let (req, rx) = RequestInitiator::post_json_arg("box_move".into(), &m);
        self.send_req(req, rx, marker)
    }

    pub fn watch_parts(&self, q: &WatchParts, marker: &'static str) -> Promise<()> {
        let (req, rx) = RequestInitiator::post_json_arg("watch_parts".into(), &q);
        self.send_req(req, rx, marker)
    }

    pub fn box_tree(
        &self,
        filter: BoxTreeFilter,
        marker: &'static str,
    ) -> Promise<Vec<BoxTreeItem>> {
        let (req, rx) = RequestInitiator::post_json_arg("box_tree".into(), &filter);
        self.send_req(req, rx, marker)
    }
}

impl BoxId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }

    pub fn value(self) -> i32 {
        self.0
    }
}

impl BoxItemId {
    pub fn value(self) -> i32 {
        self.0
    }
}

impl BoxItemMove {
    pub fn from_box(&self) -> BoxId {
        match self {
            BoxItemMove::ToSerial { from_box, .. } => *from_box,
            BoxItemMove::ToNewBox { from_box, .. } => *from_box,
        }
    }
}
