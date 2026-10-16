//! Deterministic complete search over the eight dashboard cells.
use dashboard_config::{Config, Geometry, Placement};

use crate::editor::EditorError;

pub fn repack_changed_views(
    before: &Config,
    after: &mut Config,
) -> Result<usize, EditorError> {
    // Callers admit only clean, uniquely identified configurations.
    let mut moved = 0;
    for view in &after.views {
        let geometry = after
            .view_orientation(view.id)
            .ok_or(EditorError::MissingView(view.id))?
            .geometry();
        if before.view_orientation(view.id).map(|o| o.geometry()) == Some(geometry) {
            continue;
        }
        let indices: Vec<_> = after
            .widgets
            .iter()
            .enumerate()
            .filter(|(_, w)| w.view == view.id)
            .map(|(i, _)| i)
            .collect();
        if indices.len() > 8 {
            return Err(EditorError::LayoutCannotFit(view.id));
        }
        let mut order = indices.clone();
        // Stable ties retain original vector order; placements never reorder widgets.
        order.sort_by_key(|&i| {
            std::cmp::Reverse(after.widgets[i].placement.span.columns() * after.widgets[i].placement.span.rows())
        });
        let mut placements: Vec<_> = order.iter().map(|&i| after.widgets[i].placement).collect();
        if !pack(geometry, &mut placements, 0, 0) {
            return Err(EditorError::LayoutCannotFit(view.id));
        }
        for (index, placement) in order.into_iter().zip(placements) {
            moved += usize::from(after.widgets[index].placement != placement);
            after.widgets[index].placement = placement;
        }
    }
    Ok(moved)
}

fn pack(
    geometry: Geometry,
    placements: &mut [Placement],
    index: usize,
    occupied: u8,
) -> bool {
    if index == placements.len() {
        return true;
    }
    let original = placements[index];
    let mut candidates = Vec::with_capacity(9);
    candidates.push(original);
    for row in 0..geometry.rows() {
        for column in 0..geometry.columns() {
            let placement = Placement {
                row,
                column,
                span: original.span,
            };
            if placement != original {
                candidates.push(placement);
            }
        }
    }
    for placement in candidates {
        if let Some(mask) = placement.occupied_in(geometry).filter(|mask| mask & occupied == 0) {
            placements[index] = placement;
            if pack(geometry, placements, index + 1, occupied | mask) {
                return true;
            }
        }
    }
    placements[index] = original;
    false
}
