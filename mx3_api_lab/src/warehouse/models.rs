use chrono::{DateTime, Utc};

use crate::endpoints::{
    params::Resolved,
    part::PartId,
    pcb::PcbId,
    warehouse::{BoxId, BoxSerial},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct BoxCreate {
    pub serial: String,
    pub parent: Option<BoxParent>,
}

#[derive(Serialize, Deserialize)]
pub enum BoxParent {
    Serial(BoxSerial),
    BoxId(BoxId),
}

#[derive(Serialize, Deserialize)]
pub struct MoveBoxes {
    pub source: Vec<BoxId>,
    pub target: Option<BoxId>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct WarehouseBox {
    pub id: BoxId,
    pub serial: String,
    pub parent_chain: Vec<String>,
    pub is_removed: bool,
    pub last_counted: DateTime<Utc>,
    pub items: BoxContents,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BoxContents {
    pub items: Vec<BoxItem>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BoxItem {
    pub count: i32,
    pub kind: BoxItemKind,
    /// box item parameters, specific to one item only
    pub params: Vec<Resolved>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BoxItemKind {
    Part { part_id: PartId },
    Pcb { pcb_id: PcbId }, // Pcba
}

impl WarehouseBox {
    pub fn part_count(&self, part_id: PartId) -> u32 {
        let mut total = 0;
        for item in &self.items.items {
            if let BoxItemKind::Part { part_id: p } = &item.kind
                && &part_id == p
                && let Ok(count) = u32::try_from(item.count)
            {
                total += count;
            }
        }
        total
    }

    pub fn take_parts(&mut self, part_id: PartId, count: u32) {
        for item in &mut self.items.items {
            if let BoxItemKind::Part { part_id: p } = &item.kind
                && part_id == *p
            {
                item.count -= count as i32;
                return;
            }
        }
    }

    pub fn path_serial(&self) -> String {
        let path: String =
            itertools::intersperse(self.parent_chain.iter().rev().map(|s| s.as_str()), "/")
                .collect();
        if path.is_empty() {
            self.serial.clone()
        } else {
            format!("{}/{}", path, self.serial)
        }
    }
}

impl BoxContents {
    pub fn new(items: Vec<BoxItem>) -> Self {
        Self { items }
    }

    pub fn part_count(&self, part_id: PartId) -> Option<i32> {
        self.items.iter().find_map(|i| {
            if let BoxItemKind::Part { part_id: p } = &i.kind {
                if *p != part_id {
                    return None;
                }
                Some(i.count)
            } else {
                None
            }
        })
    }
}
