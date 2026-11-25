use crate::endpoints::bom::BomId;
use crate::endpoints::companies::CompanyId;
use crate::endpoints::part::PartId;
use crate::endpoints::pcb::PcbId;
use crate::endpoints::user::UserId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
pub struct FolderId(i32);

impl FolderId {
    pub fn root() -> FolderId {
        FolderId(1)
    }
}

#[derive(Serialize, Deserialize)]
pub struct FolderItem {
    kind: FolderItemKind,
    name: String,
    description: String,
}

#[derive(Serialize, Deserialize)]
pub enum FolderItemKind {
    Bom(BomId),
    Part(PartId),
    // Box
    // Cart
    Company(CompanyId),
    // File
    Folder(FolderId),
    // Order
    // ParameterDef,
    Pcb(PcbId),
    // SerialSeries,
    User(UserId),
}

#[derive(Serialize, Deserialize)]
pub struct FolderContents {
    pub items: Vec<FolderItem>,
    /// System folders are autopopulated
    pub is_system_folder: bool,
}
