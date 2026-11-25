#[cfg(feature = "client")]
use crate::endpoints::bom::Bom;
use crate::endpoints::part::PartId;
#[cfg(feature = "client")]
use crate::endpoints::serial::Serial;
use crate::endpoints::serial::SerialId;
use crate::endpoints::warehouse::BoxId;
#[cfg(feature = "client")]
use crate::request::RequestInitiator;
#[cfg(feature = "client")]
use crate::{MxClient, Promise};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct PcbId(i32);

impl PcbId {
    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn hack(id: i32) -> PcbId {
        PcbId(id)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Pcb {
    pub id: PcbId,
    pub name: String,
    pub description: String,
    pub git_link: String,
    // picture
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub revision: String,
    pub parent_id: Option<PcbId>,
    /// The product the board is part of ("Portwright"); empty: the parent PCB's (BOM-27)
    #[serde(default)]
    pub product: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CreatePcb {
    pub name: String,
    pub description: String,
    pub git_link: String,
    // picture
    pub revision: String,
    pub parent_id: Option<PcbId>,
    /// The product the board is part of (BOM-27)
    #[serde(default)]
    pub product: String,
}

impl Default for CreatePcb {
    fn default() -> Self {
        CreatePcb {
            name: "".to_string(),
            description: "".to_string(),
            git_link: "".to_string(),
            revision: "A".to_string(),
            parent_id: None,
            product: String::new(),
        }
    }
}

/// BOM-27: set the product a PCB is part of
pub const EP_PCB_SET_PRODUCT: &str = "/pcb/set_product";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetPcbProduct {
    pub pcb_id: PcbId,
    /// Empty clears it (the parent PCB's product applies again)
    pub product: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CreatePcbaSerial {
    pub pcb_id: PcbId,
    pub variant: String,
}

/// One part placed on a serial during assembly (a box move to the serial, PCB-2)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerialPlacement {
    pub serial_id: SerialId,
    pub designator: String,
    pub part_id: PartId,
    pub from_box: BoxId,
    pub count: i32,
    pub moved_at: DateTime<Utc>,
}

#[cfg(feature = "client")]
impl MxClient {
    pub fn get_pcbs(&self, marker: &'static str) -> Promise<Vec<Pcb>> {
        let (req, rx) = RequestInitiator::get_no_args("get_pcbs".into());
        self.send_req(req, rx, marker)
    }

    pub fn get_pcb_serials(&self, id: PcbId, marker: &'static str) -> Promise<Vec<Serial>> {
        let (req, rx) = RequestInitiator::post_json_arg("get_pcb_serials".into(), &id);
        self.send_req(req, rx, marker)
    }

    pub fn get_pcb_groups(&self, id: PcbId, marker: &'static str) -> Promise<Vec<String>> {
        let (req, rx) = RequestInitiator::post_json_arg("get_pcb_groups".into(), &id);
        self.send_req(req, rx, marker)
    }

    pub fn get_pcb_bom(&self, id: PcbId, marker: &'static str) -> Promise<Option<Bom>> {
        let (req, rx) = RequestInitiator::post_json_arg("get_pcb_bom".into(), &id);
        self.send_req(req, rx, marker)
    }

    pub fn create_pcb(&self, create: &CreatePcb, marker: &'static str) -> Promise<PcbId> {
        let (req, rx) = RequestInitiator::post_json_arg("create_pcb".into(), &create);
        self.send_req(req, rx, marker)
    }

    pub fn create_pcba_serial(
        &self,
        create: &CreatePcbaSerial,
        marker: &'static str,
    ) -> Promise<Serial> {
        let (req, rx) = RequestInitiator::post_json_arg("create_pcba_serial".into(), &create);
        self.send_req(req, rx, marker)
    }
}
