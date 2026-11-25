//! Typed blocking calls for the lab endpoints, used by the CLI. They mirror
//! the `Promise` calls of `MxClient` that the UI uses.

use anyhow::Result;

#[cfg(feature = "client")]
use crate::blocking::BlockingClient;
use crate::endpoints::bom::{
    AddBomItemPartChoices, Bom, BomHeader, BomId, BomItemCandidates, BomXlsxFile, BomXlsxRequest,
    CreateBom, EP_BOM_XLSX, EP_CONFIRM_CANDIDATES, EP_DROP_CANDIDATES, EP_MAKE_CANDIDATES,
    EP_REMOVE_CHOICES, EP_SET_MATCH, SetBomItemMatch,
};
use crate::endpoints::bom_chain::{
    DescribeBom, EP_BOM_DESCRIBE, EP_BOM_FREEZE, EP_BOM_IMPORT, EP_BOM_PRUNE, EP_BOM_SPLIT,
    FreezeBom, ImportResult, PruneBom, PruneResult, SplitBom,
};
use crate::endpoints::companies::{Company, CompanyId, CreateCompany};
use crate::endpoints::crawler::CrawlerCommand;
use crate::endpoints::file::{EP_FILE_EXPORT, File, FileExport, FileExportReport, GetFile};
use crate::endpoints::part::{LookupParts, LookupPartsResult, Part, PartId};
use crate::endpoints::pcb::{
    CreatePcb, CreatePcbaSerial, EP_PCB_SET_PRODUCT, Pcb, PcbId, SerialPlacement, SetPcbProduct,
};
use crate::endpoints::search::ParametricSearch;
use crate::endpoints::serial::{Serial, SerialId};
use crate::endpoints::warehouse::{
    BoxFilter, BoxId, BoxItemMove, BoxItemMoveResult, BoxTreeFilter, BoxTreeItem, Boxes,
    RegisterStockReq, RegisterStockResult, WatchParts,
};
use crate::orders::api::{
    EP_ORDERS_SUPPLIER_SYNC, EP_ORDERS_WAREHOUSE_GET, EP_ORDERS_WAREHOUSE_HEADERS,
    EP_ORDERS_WAREHOUSE_IMPORT,
};
use crate::orders::models::{
    Order, OrderHeader, OrderId, OrdersHeadersFilter, OrdersSupplierSyncKind,
    OrdersSupplierSyncResult,
};
use crate::warehouse::api::{
    EP_WAREHOUSE_BOX_CREATE, EP_WAREHOUSE_BOX_GET, EP_WAREHOUSE_BOX_MOVE_SEVERAL,
    EP_WAREHOUSE_BOX_TREE,
};
use crate::warehouse::models::{BoxCreate, MoveBoxes, WarehouseBox};

pub const EP_PCB_SERIAL_PLACEMENTS: &str = "/pcb/serial/placements";

impl BlockingClient {
    /// `GET /version` (PLT-24, no token): what the backend runs.
    pub fn version(&self) -> Result<crate::endpoints::version::BackendVersion> {
        self.get_json(crate::endpoints::version::EP_VERSION)
    }

    // Parts and search
    pub fn get_part(&self, id: PartId) -> Result<Part> {
        self.post_json(&id, "get_part")
    }

    pub fn lookup_parts(&self, q: &LookupParts) -> Result<LookupPartsResult> {
        self.post_json(q, "lookup_parts")
    }

    pub fn parametric_search(&self, q: &ParametricSearch) -> Result<Vec<Part>> {
        self.post_json(&q.clone().lowered(), "parametric_search")
    }

    pub fn watch_parts(&self, q: &WatchParts) -> Result<()> {
        self.post_json(q, "watch_parts")
    }

    pub fn set_part_manufacturer(
        &self,
        q: &crate::endpoints::part::SetPartManufacturer,
    ) -> Result<()> {
        self.post_json(q, "set_part_manufacturer")
    }

    pub fn crawler_cmd(&self, cmd: &CrawlerCommand) -> Result<()> {
        self.post_json(cmd, "crawler_cmd")
    }

    /// What the crawler does per supplier (INT-12)
    pub fn crawler_status(&self) -> Result<crate::endpoints::crawler::CrawlerStatus> {
        self.get_json("crawler_status")
    }

    /// Queue every failed part again, at one supplier or all (CRAWL-2)
    pub fn crawler_retry_failed(
        &self,
        q: &crate::endpoints::crawler::RetryFailed,
    ) -> Result<crate::endpoints::crawler::RetryFailedResult> {
        self.post_json(q, crate::endpoints::crawler::EP_RETRY_FAILED)
    }

    /// The parts each supplier's crawler takes next, with priority and why (CRAWL-6)
    pub fn crawler_queue(
        &self,
        q: &crate::endpoints::crawler::QueueQuery,
    ) -> Result<Vec<crate::endpoints::crawler::SupplierQueue>> {
        self.post_json(q, crate::endpoints::crawler::EP_QUEUE)
    }

    /// Move a BOM's parts to the front of every crawler's queue (CRAWL-6)
    pub fn crawler_bom_front(
        &self,
        q: &crate::endpoints::crawler::BomFront,
    ) -> Result<crate::endpoints::crawler::BomFrontResult> {
        self.post_json(q, crate::endpoints::crawler::EP_BOM_FRONT)
    }

    /// Every supplier API's quota with what is used of it (CRAWL-7)
    pub fn crawler_quota(&self) -> Result<Vec<crate::endpoints::crawler::SupplierQuota>> {
        self.get_json(crate::endpoints::crawler::EP_QUOTA.trim_start_matches('/'))
    }

    /// Change a supplier API's quota (CRAWL-7)
    pub fn crawler_quota_set(
        &self,
        q: &crate::endpoints::crawler::SetQuota,
    ) -> Result<crate::endpoints::crawler::SetQuotaResult> {
        self.post_json(q, crate::endpoints::crawler::EP_QUOTA_SET)
    }

    /// The parts whose last crawl failed (CRAWL-2)
    pub fn crawler_failed(
        &self,
        q: &crate::endpoints::crawler::ListFailed,
    ) -> Result<Vec<crate::endpoints::crawler::FailedPart>> {
        self.post_json(q, crate::endpoints::crawler::EP_FAILED)
    }

    /// Parametric search at Digi-Key, with picks added to a BOM line (SRCH-4)
    /// Market look-up of parts at Octopart (INT-16): calls of up to 100 parts each, each up to 90 s
    pub fn market(
        &self,
        q: &crate::endpoints::market::MarketRequest,
    ) -> Result<crate::endpoints::market::MarketAnswer> {
        self.post_json_waiting(
            q,
            crate::endpoints::market::EP_MARKET,
            std::time::Duration::from_secs(600),
        )
    }

    pub fn supplier_search(
        &self,
        q: &crate::endpoints::supplier_search::PartSearch,
    ) -> Result<crate::endpoints::supplier_search::PartSearchResult> {
        // several Digi-Key calls, each up to a minute
        self.post_json_waiting(
            q,
            crate::endpoints::supplier_search::EP_SUPPLIER_SEARCH,
            std::time::Duration::from_secs(300),
        )
    }

    /// Digi-Key's substitutes, recommended products and near MPNs for one MPN (INT-18)
    pub fn supplier_substitutes(
        &self,
        q: &crate::endpoints::substitutes::SubstitutesQuery,
    ) -> Result<crate::endpoints::substitutes::SubstitutesResult> {
        self.post_json_waiting(
            q,
            crate::endpoints::substitutes::EP_SUPPLIER_SUBSTITUTES,
            std::time::Duration::from_secs(300),
        )
    }

    /// The kept Digi-Key suggestions of parts (INT-18)
    pub fn dk_suggestions(
        &self,
        q: &crate::endpoints::substitutes::SuggestionsQuery,
    ) -> Result<Vec<crate::endpoints::substitutes::PartSuggestions>> {
        self.post_json(q, crate::endpoints::substitutes::EP_DK_SUGGESTIONS)
    }

    /// Digi-Key's categories by name (SRCH-4)
    pub fn supplier_categories(
        &self,
        q: &crate::endpoints::supplier_search::CategoryQuery,
    ) -> Result<Vec<crate::endpoints::supplier_search::CategoryHit>> {
        self.post_json(q, crate::endpoints::supplier_search::EP_SUPPLIER_CATEGORIES)
    }

    // Warehouse
    pub fn get_boxes(&self, filter: &BoxFilter) -> Result<Boxes> {
        self.post_json(filter, "get_boxes")
    }

    pub fn box_get(&self, id: BoxId) -> Result<WarehouseBox> {
        self.post_json(&id, EP_WAREHOUSE_BOX_GET)
    }

    pub fn box_tree(&self) -> Result<Vec<BoxTreeItem>> {
        self.post_json(&BoxTreeFilter::All, EP_WAREHOUSE_BOX_TREE)
    }

    pub fn box_create(&self, c: &BoxCreate) -> Result<BoxId> {
        self.post_json(c, EP_WAREHOUSE_BOX_CREATE)
    }

    pub fn box_move_several(&self, q: &MoveBoxes) -> Result<()> {
        self.post_json(q, EP_WAREHOUSE_BOX_MOVE_SEVERAL)
    }

    pub fn box_item_move(&self, m: &BoxItemMove) -> Result<BoxItemMoveResult> {
        self.post_json(m, "box_move")
    }

    pub fn register_stock(&self, q: &RegisterStockReq) -> Result<RegisterStockResult> {
        self.post_json(q, "register_stock")
    }

    // Companies
    pub fn get_companies(&self) -> Result<Vec<Company>> {
        self.get_json("get_companies")
    }

    pub fn create_company(&self, q: &CreateCompany) -> Result<CompanyId> {
        self.post_json(q, "create_company")
    }

    pub fn update_manufacturers(&self) -> Result<()> {
        self.post_json(&(), "update_manufacturers")
    }

    // BOMs
    pub fn get_bom_headers(&self) -> Result<Vec<BomHeader>> {
        self.get_json("get_bom_headers")
    }

    pub fn get_bom(&self, id: BomId) -> Result<Bom> {
        self.post_json(&id, "get_bom")
    }

    pub fn create_bom(&self, q: &CreateBom) -> Result<BomId> {
        self.post_json(q, "create_bom")
    }

    /// BOM-28: import as the next revision of a chain (or a new one), carrying over from the chain's newest
    pub fn import_bom(&self, q: &CreateBom) -> Result<ImportResult> {
        self.post_json(q, EP_BOM_IMPORT)
    }

    /// BOM-28: a split-off chain whose revision 1 copies the given revision
    pub fn split_bom(&self, q: &SplitBom) -> Result<BomHeader> {
        self.post_json(q, EP_BOM_SPLIT)
    }

    /// BOM-28: freeze a revision by hand, or release it
    pub fn freeze_bom(&self, q: &FreezeBom) -> Result<BomHeader> {
        self.post_json(q, EP_BOM_FREEZE)
    }

    /// BOM-28: set a revision's description (frozen ones too)
    pub fn describe_bom(&self, q: &DescribeBom) -> Result<BomHeader> {
        self.post_json(q, EP_BOM_DESCRIBE)
    }

    /// BOM-28: delete working revisions of a chain
    pub fn prune_bom(&self, q: &PruneBom) -> Result<PruneResult> {
        self.post_json(q, EP_BOM_PRUNE)
    }

    pub fn add_bom_item_part_choices(&self, q: &AddBomItemPartChoices) -> Result<()> {
        self.post_json(q, "add_bom_item_part_choices")
    }

    /// BOM-24: the given candidates of a line become real part choices
    pub fn confirm_bom_item_candidates(&self, q: &BomItemCandidates) -> Result<()> {
        self.post_json(q, EP_CONFIRM_CANDIDATES)
    }

    /// BOM-24: remove the given candidates from a line; real choices are left alone
    pub fn drop_bom_item_candidates(&self, q: &BomItemCandidates) -> Result<()> {
        self.post_json(q, EP_DROP_CANDIDATES)
    }

    /// BOM-25: the given real choices of a line become candidates; at least one real choice stays
    pub fn make_bom_item_candidates(&self, q: &BomItemCandidates) -> Result<()> {
        self.post_json(q, EP_MAKE_CANDIDATES)
    }

    /// BOM-29: remove the given real choices of a line entirely; refused on a frozen revision
    pub fn remove_bom_item_part_choices(&self, q: &BomItemCandidates) -> Result<()> {
        self.post_json(q, EP_REMOVE_CHOICES)
    }

    /// BOM-26: set or clear a line's match-group text
    pub fn set_bom_item_match(&self, q: &SetBomItemMatch) -> Result<()> {
        self.post_json(q, EP_SET_MATCH)
    }

    // PCBs and serials
    /// The BOM spreadsheet (BOM-27); the backend reads every part of the BOM, so it may take a while
    pub fn bom_xlsx(&self, q: &BomXlsxRequest) -> Result<BomXlsxFile> {
        self.post_json_waiting(q, EP_BOM_XLSX, std::time::Duration::from_secs(120))
    }

    pub fn set_pcb_product(&self, q: &SetPcbProduct) -> Result<()> {
        self.post_json(q, EP_PCB_SET_PRODUCT)
    }

    pub fn get_pcbs(&self) -> Result<Vec<Pcb>> {
        self.get_json("get_pcbs")
    }

    pub fn create_pcb(&self, q: &CreatePcb) -> Result<PcbId> {
        self.post_json(q, "create_pcb")
    }

    pub fn get_pcb_serials(&self, id: PcbId) -> Result<Vec<Serial>> {
        self.post_json(&id, "get_pcb_serials")
    }

    pub fn get_pcb_groups(&self, id: PcbId) -> Result<Vec<String>> {
        self.post_json(&id, "get_pcb_groups")
    }

    pub fn get_pcb_bom(&self, id: PcbId) -> Result<Option<Bom>> {
        self.post_json(&id, "get_pcb_bom")
    }

    pub fn create_pcba_serial(&self, q: &CreatePcbaSerial) -> Result<Serial> {
        self.post_json(q, "create_pcba_serial")
    }

    /// Parts already placed on these serials (box moves to a serial), oldest first
    pub fn serial_placements(&self, serials: &[SerialId]) -> Result<Vec<SerialPlacement>> {
        self.post_json(&serials, EP_PCB_SERIAL_PLACEMENTS)
    }

    // Orders
    pub fn orders_sync(&self, kind: &OrdersSupplierSyncKind) -> Result<OrdersSupplierSyncResult> {
        self.post_json(kind, EP_ORDERS_SUPPLIER_SYNC)
    }

    pub fn order_headers(&self, filter: &OrdersHeadersFilter) -> Result<Vec<OrderHeader>> {
        self.post_json(filter, EP_ORDERS_WAREHOUSE_HEADERS)
    }

    pub fn order_get(&self, id: OrderId) -> Result<Order> {
        self.post_json(&id, EP_ORDERS_WAREHOUSE_GET)
    }

    pub fn order_import(&self, id: OrderId) -> Result<BoxId> {
        self.post_json(&id, EP_ORDERS_WAREHOUSE_IMPORT)
    }

    // Files
    pub fn get_file(&self, q: &GetFile) -> Result<File> {
        self.post_json(q, "get_file")
    }

    /// One pass of the photo export (WH-25); repeat with `after_id = last_id` while `left > 0`.
    pub fn file_export(&self, q: &FileExport) -> Result<FileExportReport> {
        self.post_json(q, EP_FILE_EXPORT)
    }
}
