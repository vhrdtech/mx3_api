use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::endpoints::{api::ApiKeyId, companies::CompanyId, part::PartId, warehouse::BoxId};
use mx3_api_core::money::Money;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct OrderId(pub i32);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct OrderItemId(pub i32);

#[derive(Default, Serialize, Deserialize)]
pub enum OrdersSupplierSyncKind {
    #[default]
    All,
    DateRange {
        start: NaiveDate,
        end: NaiveDate,
    },
}

#[derive(Serialize, Deserialize)]
pub struct OrdersSupplierSyncResult {
    pub new_orders: Vec<OrderId>,
    pub errors: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub enum OrdersHeadersFilter {
    All,
    Finished,
    NotFinished,
}

#[derive(Serialize, Deserialize)]
pub struct OrderHeader {
    pub id: OrderId,
    pub api_key_id: Option<ApiKeyId>,
    pub imported_into_box_id: Option<BoxId>,

    pub supplier_id: CompanyId,
    pub order_number: Option<String>,
    pub sales_order_number: Option<String>,

    pub order_date: Option<DateTime<Utc>>,
    pub ship_date: Option<NaiveDate>,
    pub delivery_date: Option<NaiveDate>,

    pub status: Option<String>,

    pub ship_method: Option<String>,
    pub tracking_number: Option<String>,
    pub delivery_address: Option<serde_json::Value>,
    pub tracking_url: Option<String>,

    pub currency: Option<String>,
    pub total_price: Option<Money>,

    pub finished: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Order {
    pub header: OrderHeader,
    pub items: Vec<OrderItem>,
}

#[derive(Serialize, Deserialize)]
pub struct OrderItem {
    pub id: OrderItemId,
    pub supplier_pn: Option<String>,
    pub part_id: PartId,
    pub mfg_pn: String,
    pub description: Option<String>,
    pub packaging: Option<String>,
    pub unit_price: Option<Money>,
    pub quantity_shipped: Option<u32>,
    pub quantity_ordered: Option<u32>,
    pub quantity_reserved: Option<u32>,
    pub quantity_back_order: Option<u32>,
    pub quantity_initial: Option<u32>,
    pub country_of_origin: Option<String>,
}
