use super::models::BoxCreate;
use crate::{
    endpoints::warehouse::{BoxId, BoxTreeFilter, BoxTreeItem},
    prelude::*,
    warehouse::models::{MoveBoxes, WarehouseBox},
};

pub const EP_WAREHOUSE_BOX_CREATE: &str = "/warehouse/box/create";
pub const EP_WAREHOUSE_BOX_TREE: &str = "/warehouse/box/tree";
pub const EP_WAREHOUSE_BOX_MOVE_SEVERAL: &str = "/warehouse/box/move_several";
pub const EP_WAREHOUSE_BOX_GET: &str = "/warehouse/box/get";

pub struct MxWarehouseClient<'i> {
    pub(crate) request_tx: &'i UnboundedSender<RequestInitiator>,
}

pub struct MxWarehouseBoxesClient<'i> {
    pub(crate) request_tx: &'i UnboundedSender<RequestInitiator>,
}

impl MxWarehouseClient<'_> {
    pub fn boxes(&self) -> MxWarehouseBoxesClient<'_> {
        MxWarehouseBoxesClient {
            request_tx: self.request_tx,
        }
    }
}

impl MxWarehouseBoxesClient<'_> {
    pub fn create(&self, c: &BoxCreate, marker: &'static str) -> Promise<BoxId> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_WAREHOUSE_BOX_CREATE.into(), &c);
        send_req(self.request_tx, req, rx, marker)
    }

    pub fn tree(&self, filter: &BoxTreeFilter, marker: &'static str) -> Promise<Vec<BoxTreeItem>> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_WAREHOUSE_BOX_TREE.into(), &filter);
        send_req(self.request_tx, req, rx, marker)
    }

    pub fn move_several(&self, q: MoveBoxes, marker: &'static str) -> Promise<()> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_WAREHOUSE_BOX_MOVE_SEVERAL.into(), &q);
        send_req(self.request_tx, req, rx, marker)
    }

    pub fn get(&self, id: &BoxId, marker: &'static str) -> Promise<WarehouseBox> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_WAREHOUSE_BOX_GET.into(), &id);
        send_req(self.request_tx, req, rx, marker)
    }
}
