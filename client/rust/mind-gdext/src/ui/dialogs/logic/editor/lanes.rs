// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `lanes` support code.
//! Upstream: logic/LCanvas.java

use super::*;

// ---------------------------------------------------------------------------
// jump-curve lane prediction (`DragLayout.setJumpHeights`)
// ---------------------------------------------------------------------------

/// `DragLayout.setJumpHeights` (`LCanvas.java:274-321`): representative curves
/// (widest forward jump per end, widest backward jump per begin) get nested lane
/// heights; every curve then takes its representative's height.
pub(super) fn compute_jump_heights(state: &Rc<RefCell<LogicState>>) {
    let statements = state.borrow().statements.clone();
    let selecting = state.borrow().jump_selecting;
    state.borrow_mut().pred_heights.clear();
    let mut curves: Vec<CurveData> = Vec::new();
    for (index, row) in statements.iter().enumerate() {
        if index as i64 == selecting {
            continue;
        }
        let dest = row.jump_dest(statements.len());
        if dest < 0 {
            continue;
        }
        curves.push(CurveData {
            jump: index as i64,
            begin: (index as i64).min(dest),
            end: (index as i64).max(dest),
            flipped: index as i64 >= dest,
            pred: 0,
            done: false,
        });
    }
    let mut repr_before: Vec<(i64, usize)> = Vec::new();
    let mut repr_after: Vec<(i64, usize)> = Vec::new();
    for (curve_index, curve) in curves.iter().enumerate() {
        if curve.flipped {
            if let Some((_, existing)) = repr_after.iter().find(|(begin, _)| *begin == curve.begin)
                && curves[*existing].end >= curve.end
            {
                continue;
            }
            if let Some(existing) = repr_after
                .iter_mut()
                .find(|(begin, _)| *begin == curve.begin)
            {
                existing.1 = curve_index;
            } else {
                repr_after.push((curve.begin, curve_index));
            }
        } else {
            if let Some((_, existing)) = repr_before.iter().find(|(end, _)| *end == curve.end)
                && curves[*existing].begin <= curve.begin
            {
                continue;
            }
            if let Some(existing) = repr_before.iter_mut().find(|(end, _)| *end == curve.end) {
                existing.1 = curve_index;
            } else {
                repr_before.push((curve.end, curve_index));
            }
        }
    }
    let mut processed: Vec<usize> = repr_before.iter().map(|(_, index)| *index).collect();
    processed.extend(repr_after.iter().map(|(_, index)| *index));
    let mut order: Vec<usize> = (0..processed.len()).collect();
    order.sort_by_key(|slot| {
        let curve = &curves[processed[*slot]];
        (curve.begin, *slot)
    });
    let processed: Vec<usize> = order.into_iter().map(|slot| processed[slot]).collect();
    let mut occupiers: Vec<usize> = Vec::new();
    let mut occupied: BTreeSet<i64> = BTreeSet::new();
    let mut scratch = HeightScratch::default();
    for index in 0..processed.len() {
        let cur = processed[index];
        let mut kept: Vec<usize> = Vec::new();
        let mut dropped: Vec<i64> = Vec::new();
        for occupier in occupiers.drain(..) {
            if curves[occupier].end > curves[cur].begin {
                kept.push(occupier);
            } else {
                dropped.push(curves[occupier].pred);
            }
        }
        for pred in dropped {
            occupied.remove(&pred);
        }
        occupiers = kept;
        let height = jump_height(
            &mut curves,
            &processed,
            index,
            Some((&occupiers, &occupied)),
            &mut scratch,
        );
        occupiers.push(cur);
        occupied.insert(height);
    }
    let mut pred_heights: BTreeMap<i64, i64> = BTreeMap::new();
    for curve in &curves {
        let representative = if curve.flipped {
            repr_after
                .iter()
                .find(|(begin, _)| *begin == curve.begin)
                .map(|(_, index)| *index)
        } else {
            repr_before
                .iter()
                .find(|(end, _)| *end == curve.end)
                .map(|(_, index)| *index)
        };
        if let Some(representative) = representative {
            pred_heights.insert(curve.jump, curves[representative].pred);
        }
    }
    state.borrow_mut().pred_heights = pred_heights;
}

/// `DragLayout.getJumpHeight` (`LCanvas.java:323-353`): the lowest free lane
/// above every jump nested inside this one. The scratch buffers are shared
/// across recursion levels exactly like Java's static
/// `tmpOccupiers2`/`tmpBits2`; `seed` is `Some` only for the top-level call
/// (whose `occupied` set is a frozen copy, not the scratch bits).
pub(super) fn jump_height(
    curves: &mut [CurveData],
    processed: &[usize],
    index: usize,
    seed: Option<(&[usize], &BTreeSet<i64>)>,
    scratch: &mut HeightScratch,
) -> i64 {
    let jmp = processed[index];
    if curves[jmp].done {
        return curves[jmp].pred;
    }
    let frozen: Option<BTreeSet<i64>> = match seed {
        Some((occupiers, occupied)) => {
            scratch.occupiers = occupiers.to_vec();
            scratch.bits = occupied.clone();
            Some(occupied.clone())
        }
        None => None,
    };
    let mut max_height: i64 = -1;
    let end_limit = curves[jmp].end;
    let mut cursor = index + 1;
    while cursor < processed.len() {
        let cur = processed[cursor];
        if curves[cur].end <= end_limit {
            let mut kept: Vec<usize> = Vec::new();
            let mut dropped: Vec<i64> = Vec::new();
            for occupier in scratch.occupiers.drain(..) {
                if curves[occupier].end > curves[cur].begin {
                    kept.push(occupier);
                } else {
                    dropped.push(curves[occupier].pred);
                }
            }
            for pred in dropped {
                scratch.bits.remove(&pred);
            }
            scratch.occupiers = kept;
            let height = jump_height(curves, processed, cursor, None, scratch);
            scratch.occupiers.push(cur);
            scratch.bits.insert(height);
            max_height = max_height.max(height);
        }
        cursor += 1;
    }
    let mut lane = max_height + 1;
    loop {
        let used = match &frozen {
            Some(set) => set.contains(&lane),
            None => scratch.bits.contains(&lane),
        };
        if !used {
            break;
        }
        lane += 1;
    }
    curves[jmp].pred = lane;
    curves[jmp].done = true;
    scratch.occupiers.clear();
    lane
}
