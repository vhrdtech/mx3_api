use crate::endpoints::part::PartId;
use crate::endpoints::warehouse::BoxId;
#[cfg(feature = "client")]
use crate::request::RequestInitiator;
#[cfg(feature = "client")]
use crate::{MxClient, Promise};
use mx3_api_core::files::{ExportReport, ExportRequest};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct FileId(pub i32);

#[derive(Serialize, Deserialize)]
pub struct GetFile {
    pub id: FileId,
}

#[derive(Serialize, Deserialize)]
pub struct File {
    #[serde(with = "mx3_api_core::base64_bytes")]
    pub bytes: Vec<u8>,
    pub part_id: Option<PartId>,
    pub box_id: Option<BoxId>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FileHeader {
    pub id: FileId,
    pub part_id: Option<PartId>,
    pub box_id: Option<BoxId>,
    pub filename: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size: u32,
}

/// `POST /file/export`: photos still only in the DB go to files in the mx3-lab share (WH-25).
pub const EP_FILE_EXPORT: &str = "/file/export";
pub type FileExport = ExportRequest;
pub type FileExportReport = ExportReport;

#[cfg(feature = "client")]
impl MxClient {
    #[expect(
        clippy::needless_pass_by_value,
        reason = "public signature mx3's callers use; changing it is its own API change"
    )]
    pub fn get_file(&self, q: GetFile, marker: &'static str) -> Promise<File> {
        let (req, rx) = RequestInitiator::post_json_arg("get_file".into(), &q);
        self.send_req(req, rx, marker)
    }
}
