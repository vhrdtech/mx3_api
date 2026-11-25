use super::models;
use crate::{
    endpoints::warehouse::BoxId,
    orders::models::{
        Order, OrderId, OrdersHeadersFilter, OrdersSupplierSyncKind, OrdersSupplierSyncResult,
    },
    prelude::*,
};

pub const EP_ORDERS_SUPPLIER_SYNC: &str = "/orders/supplier/sync";

pub const EP_ORDERS_WAREHOUSE_HEADERS: &str = "/orders/warehouse/headers";
pub const EP_ORDERS_WAREHOUSE_IMPORT: &str = "/orders/warehouse/import";
pub const EP_ORDERS_WAREHOUSE_GET: &str = "/orders/warehouse/get";

pub struct MxOrdersClient<'i> {
    pub(crate) request_tx: &'i UnboundedSender<RequestInitiator>,
}

pub struct MxSupplierOrdersClient<'i> {
    pub(crate) request_tx: &'i UnboundedSender<RequestInitiator>,
}

pub struct MxWarehouseOrdersClient<'i> {
    pub(crate) request_tx: &'i UnboundedSender<RequestInitiator>,
}

impl MxOrdersClient<'_> {
    pub fn supplier(&self) -> MxSupplierOrdersClient<'_> {
        MxSupplierOrdersClient {
            request_tx: self.request_tx,
        }
    }

    pub fn warehouse(&self) -> MxWarehouseOrdersClient<'_> {
        MxWarehouseOrdersClient {
            request_tx: self.request_tx,
        }
    }
}

impl MxSupplierOrdersClient<'_> {
    /// Retrieve new orders and sync existing status from supplier's API's into mx3
    pub fn sync(
        &self,
        kind: &OrdersSupplierSyncKind,
        marker: &'static str,
    ) -> Promise<OrdersSupplierSyncResult> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_ORDERS_SUPPLIER_SYNC.into(), kind);
        send_req(self.request_tx, req, rx, marker)
    }
}

impl MxWarehouseOrdersClient<'_> {
    /// Get orders headers from mx3 database
    #[expect(
        clippy::needless_pass_by_value,
        reason = "public signature mx3's callers use; changing it is its own API change"
    )]
    pub fn headers(
        &self,
        filter: OrdersHeadersFilter,
        marker: &'static str,
    ) -> Promise<Vec<models::OrderHeader>> {
        let (req, rx) =
            RequestInitiator::post_json_arg(EP_ORDERS_WAREHOUSE_HEADERS.into(), &filter);
        send_req(self.request_tx, req, rx, marker)
    }

    /// Get order items for an order
    pub fn get(&self, id: OrderId, marker: &'static str) -> Promise<Order> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_ORDERS_WAREHOUSE_GET.into(), &id);
        send_req(self.request_tx, req, rx, marker)
    }

    /// Create a box with parts from order
    pub fn import(&self, id: OrderId, marker: &'static str) -> Promise<BoxId> {
        let (req, rx) = RequestInitiator::post_json_arg(EP_ORDERS_WAREHOUSE_IMPORT.into(), &id);
        send_req(self.request_tx, req, rx, marker)
    }
}
