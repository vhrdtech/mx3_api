//! PCB assembly planning (PCB-2): for boards being assembled from one BOM, which designators are still open on
//! each line, which boxes hold a fitting part, and the box moves that place parts. Progress comes from the
//! placements recorded by the backend (`SerialPlacement`), so an assembly can continue where it stopped (PCB-6).

use std::collections::HashSet;

use serde::Serialize;

use crate::endpoints::bom::{Bom, BomGroupId, ParameterSource};
use crate::endpoints::part::PartId;
use crate::endpoints::pcb::SerialPlacement;
use crate::endpoints::serial::SerialId;
use crate::endpoints::warehouse::{BoxId, BoxItemMove};

/// One board being assembled
#[derive(Clone, Debug, Serialize)]
pub struct Board {
    pub serial_id: SerialId,
    /// Serial number shown to people
    pub serial: i32,
    pub group: String,
    pub group_id: BomGroupId,
    pub done: HashSet<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinePlan {
    pub item_idx: usize,
    pub description: String,
    /// Designators on all boards of this line
    pub total: u32,
    pub done: u32,
    /// Open designators per board (boards without this line left out)
    pub open: Vec<(SerialId, Vec<String>)>,
    pub sources: Vec<Source>,
}

/// A box holding one of the line's part choices
#[derive(Clone, Debug, Serialize)]
pub struct Source {
    pub part_id: PartId,
    pub part_number: String,
    pub box_id: BoxId,
    pub box_path: String,
    pub in_box: u32,
    /// Other lines that use the same part (take enough for them too)
    pub also_on_lines: Vec<usize>,
}

impl Board {
    /// Marks the board's designators that have a recorded placement
    pub fn apply_placements(&mut self, placements: &[SerialPlacement]) {
        for p in placements {
            if p.serial_id == self.serial_id {
                self.done.insert(p.designator.clone());
            }
        }
    }
}

impl LinePlan {
    pub fn is_done(&self) -> bool {
        self.done >= self.total
    }

    pub fn open_count(&self) -> u32 {
        self.open.iter().map(|(_, d)| d.len() as u32).sum()
    }
}

/// Every line with designators on at least one of the boards, in BOM order
pub fn plan(bom: &Bom, boards: &[Board]) -> Vec<LinePlan> {
    let mut lines = vec![];
    for (item_idx, item) in bom.items.iter().enumerate() {
        let mut total = 0;
        let mut done = 0;
        let mut open = vec![];
        for board in boards {
            if !item.is_in_group(board.group_id, &bom.groups) {
                continue;
            }
            total += item.designators.len() as u32;
            let board_open: Vec<String> = item
                .designators
                .iter()
                .filter(|d| !board.done.contains(*d))
                .cloned()
                .collect();
            done += item.designators.len() as u32 - board_open.len() as u32;
            if !board_open.is_empty() {
                open.push((board.serial_id, board_open));
            }
        }
        if total == 0 {
            continue;
        }
        let mut sources = vec![];
        for part_id in &item.part_choices {
            let Some(part) = bom.referenced_parts.get(part_id) else {
                continue;
            };
            let also_on_lines: Vec<usize> = bom
                .items
                .iter()
                .enumerate()
                .filter(|(l, i)| *l != item_idx && i.part_choices.contains(part_id))
                .map(|(l, _)| l)
                .collect();
            for b in &part.warehouse_stock {
                let in_box = b.part_count(*part_id);
                if in_box == 0 {
                    continue;
                }
                sources.push(Source {
                    part_id: *part_id,
                    part_number: part.part_number.clone(),
                    box_id: b.id,
                    box_path: b.path_serial(),
                    in_box,
                    also_on_lines: also_on_lines.clone(),
                });
            }
        }
        lines.push(LinePlan {
            item_idx,
            description: item
                .find_parameter_many(
                    &["Detailed description", "Description", "Value"],
                    &bom.referenced_parts,
                    ParameterSource::PreferLocal,
                )
                .and_then(|v| v.as_string().ok())
                .unwrap_or_default(),
            total,
            done,
            open,
            sources,
        });
    }
    lines
}

/// Moves placing open designators of a line from one box, board by board, at most `available` parts (one move
/// per designator, as the UI's "Place" does). `only` limits them to these designators when not empty.
pub fn place_moves(
    line: &LinePlan,
    part_id: PartId,
    from_box: BoxId,
    available: u32,
    only: &[String],
) -> Vec<BoxItemMove> {
    let mut moves = vec![];
    let mut remaining = available as usize;
    for (serial_id, open) in &line.open {
        for designator in open {
            if remaining == 0 {
                return moves;
            }
            if !only.is_empty() && !only.iter().any(|d| d.eq_ignore_ascii_case(designator)) {
                continue;
            }
            moves.push(BoxItemMove::ToSerial {
                from_box,
                part_id,
                count: 1,
                to_serial: *serial_id,
                designator: designator.clone(),
            });
            remaining -= 1;
        }
    }
    moves
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(open: Vec<(i32, Vec<&str>)>) -> LinePlan {
        LinePlan {
            item_idx: 0,
            description: "".into(),
            total: 4,
            done: 0,
            open: open
                .into_iter()
                .map(|(s, d)| (SerialId(s), d.into_iter().map(String::from).collect()))
                .collect(),
            sources: vec![],
        }
    }

    #[test]
    fn place_moves_limits_to_stock_and_designators() {
        let l = line(vec![(1, vec!["R1", "R2"]), (2, vec!["R1", "R2"])]);
        let moves = place_moves(&l, PartId(5), BoxId(9), 3, &[]);
        assert_eq!(moves.len(), 3);
        let BoxItemMove::ToSerial {
            to_serial,
            designator,
            ..
        } = &moves[2]
        else {
            panic!()
        };
        assert_eq!((*to_serial, designator.as_str()), (SerialId(2), "R1"));

        let moves = place_moves(&l, PartId(5), BoxId(9), 10, &["r2".to_string()]);
        assert_eq!(moves.len(), 2);
    }
}
