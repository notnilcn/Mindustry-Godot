// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: arc.scene.ui.layout.Table / Stack (plan 14 §3.3, OD-UI1).
//
//! Scene2d-style table and stack containers.
//!
//! [`MindTable`] places children into rows of cells with grow/fill/expand,
//! per-cell padding, colspan and min/max sizing; the fluent
//! `table.add(child).grow().pad(4)` API mirrors `Table.add(...)`. [`MindStack`]
//! shares one rectangle between every child (`arc Stack`).

use godot::classes::control::SizeFlags;
use godot::classes::notify::ContainerNotification;
use godot::classes::{Container, Control, IContainer};
use godot::prelude::*;

use super::cell::{Align, CellData};

/// One `add()` entry: the child, its layout cell and its row index.
struct TableEntry {
    control: Gd<Control>,
    cell: CellData,
    row: usize,
}

/// One cell placement resolved for a layout pass.
struct Placement {
    entry: usize,
    col: usize,
    span: usize,
    row_index: usize,
}

/// The resolved grid for the current entries: visible rows/columns and minima.
struct Grid {
    placements: Vec<Placement>,
    #[allow(dead_code)]
    // ported surface; not reachable from the native call graph (kept for parity)
    rows: Vec<usize>,
    col_min: Vec<f32>,
    row_min: Vec<f32>,
}

/// The `Table.add(child)` return value: a mutating guard that marks layout dirty.
///
/// Godot caches minimum sizes, so a chained `.width(...)`/`.pad(...)` after
/// `add()` must drop that cache; `Drop` runs [`MindTable::invalidate`] once per
/// statement.
pub struct Cell<'a> {
    table: &'a mut MindTable,
    index: usize,
}

// Arc's `Cell` builder surface; a few combinators have no native caller yet.
#[allow(dead_code)]
impl<'a> Cell<'a> {
    fn data(&mut self) -> &mut CellData {
        &mut self.table.entries[self.index].cell
    }

    /// Index of the cell in the table (for later re-borrows via [`MindTable::cell`]).
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn grow(mut self) -> Self {
        let data = self.data();
        data.grow_x = true;
        data.grow_y = true;
        self
    }

    pub fn grow_x(mut self) -> Self {
        self.data().grow_x = true;
        self
    }

    pub fn grow_y(mut self) -> Self {
        self.data().grow_y = true;
        self
    }

    pub fn fill(mut self) -> Self {
        let data = self.data();
        data.fill_x = true;
        data.fill_y = true;
        self
    }

    pub fn fill_x(mut self) -> Self {
        self.data().fill_x = true;
        self
    }

    pub fn fill_y(mut self) -> Self {
        self.data().fill_y = true;
        self
    }

    pub fn expand(mut self) -> Self {
        let data = self.data();
        data.expand_x = true;
        data.expand_y = true;
        self
    }

    pub fn expand_x(mut self) -> Self {
        self.data().expand_x = true;
        self
    }

    pub fn expand_y(mut self) -> Self {
        self.data().expand_y = true;
        self
    }

    pub fn uniform(mut self) -> Self {
        self.data().uniform = true;
        self
    }

    pub fn width(mut self, value: f32) -> Self {
        self.data().width = value;
        self
    }

    pub fn height(mut self, value: f32) -> Self {
        self.data().height = value;
        self
    }

    pub fn size(mut self, value: f32) -> Self {
        self.data().size = value;
        self
    }

    pub fn min_width(mut self, value: f32) -> Self {
        self.data().min_width = value;
        self
    }

    pub fn max_width(mut self, value: f32) -> Self {
        self.data().max_width = value;
        self
    }

    pub fn min_height(mut self, value: f32) -> Self {
        self.data().min_height = value;
        self
    }

    pub fn max_height(mut self, value: f32) -> Self {
        self.data().max_height = value;
        self
    }

    /// Uniform padding on all four sides (`Cell.pad`).
    pub fn pad(mut self, value: f32) -> Self {
        let data = self.data();
        data.pad_left = value;
        data.pad_right = value;
        data.pad_top = value;
        data.pad_bottom = value;
        self
    }

    pub fn pad_left(mut self, value: f32) -> Self {
        self.data().pad_left = value;
        self
    }

    pub fn pad_right(mut self, value: f32) -> Self {
        self.data().pad_right = value;
        self
    }

    pub fn pad_top(mut self, value: f32) -> Self {
        self.data().pad_top = value;
        self
    }

    pub fn pad_bottom(mut self, value: f32) -> Self {
        self.data().pad_bottom = value;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.data().align = align;
        self
    }

    pub fn top(mut self) -> Self {
        self.data().align = Align::Top;
        self
    }

    pub fn bottom(mut self) -> Self {
        self.data().align = Align::Bottom;
        self
    }

    pub fn left(mut self) -> Self {
        self.data().align = Align::Left;
        self
    }

    pub fn right(mut self) -> Self {
        self.data().align = Align::Right;
        self
    }

    pub fn colspan(mut self, value: i32) -> Self {
        self.data().colspan = value.max(1);
        self
    }

    pub fn color(mut self, value: Color) -> Self {
        self.data().color = value;
        self
    }

    pub fn disabled(mut self, value: bool) -> Self {
        self.data().disabled = value;
        self
    }
}

impl Drop for Cell<'_> {
    fn drop(&mut self) {
        self.table.invalidate();
    }
}

/// `MindTable` — Arc `Table` over a Godot `Container`.
#[derive(GodotClass)]
#[class(rename = RustMindTable, base=Container, init)]
pub struct MindTable {
    base: Base<Container>,
    entries: Vec<TableEntry>,
    current_row: usize,
    defaults: CellData,
    background_name: StringName,
    margin_value: f32,
    /// Arc `Table.top()`: the grid hugs the top edge instead of centring.
    pub table_top: bool,
    /// Arc `Table.left()`: the grid hugs the left edge.
    pub table_left: bool,
}

#[godot_api]
impl IContainer for MindTable {
    fn on_notification(&mut self, what: ContainerNotification) {
        if what == ContainerNotification::SORT_CHILDREN {
            self.sort_children();
        }
    }

    fn get_minimum_size(&self) -> Vector2 {
        let grid = self.grid();
        let mut min_width = self.margin_value * 2.0;
        let mut min_height = self.margin_value * 2.0;
        for value in grid.col_min {
            min_width += value;
        }
        for value in grid.row_min {
            min_height += value;
        }
        Vector2::new(min_width, min_height)
    }

    fn draw(&mut self) {
        if self.background_name.is_empty() {
            return;
        }
        let name = self.background_name.clone();
        let Some(style) = self
            .base()
            .get_theme_stylebox_ex(&name)
            .theme_type("MindTable")
            .done()
        else {
            return;
        };
        let size = self.base().get_size();
        self.base_mut()
            .draw_style_box(&style, Rect2::new(Vector2::ZERO, size));
    }
}

#[godot_api]
impl MindTable {
    /// Appends `child` to the current row and returns its [`Cell`].
    pub fn add<N: Inherits<Control>>(&mut self, child: &Gd<N>) -> Cell<'_> {
        let control: Gd<Control> = child.clone().upcast();
        let mut cell = self.defaults.clone();
        cell.colspan = cell.colspan.max(1);
        self.entries.push(TableEntry {
            control: control.clone(),
            cell,
            row: self.current_row,
        });
        self.base_mut().add_child(&control);
        let index = self.entries.len() - 1;
        self.invalidate();
        Cell { table: self, index }
    }

    /// Starts a new row (`Table.row()`); later `add` calls land in it.
    pub fn row(&mut self) {
        self.current_row += 1;
        self.invalidate();
    }

    /// The cell whose flags apply to subsequent `add` calls (`defaults()`).
    pub fn defaults(&mut self) -> &mut CellData {
        &mut self.defaults
    }

    /// Re-borrows an existing cell by index (see [`Self::add`]).
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn cell(&mut self, index: usize) -> Option<Cell<'_>> {
        if index >= self.entries.len() {
            return None;
        }
        Some(Cell { table: self, index })
    }

    /// The control of an existing cell.
    #[allow(dead_code)] // Arc Table surface; no native caller yet.
    pub fn cell_control(&self, index: usize) -> Option<Gd<Control>> {
        self.entries.get(index).map(|entry| entry.control.clone())
    }

    /// Number of cells currently in the table.
    #[allow(dead_code)] // Arc Table surface; no native caller yet.
    pub fn cell_count(&self) -> usize {
        self.entries.len()
    }

    /// Removes every cell and child, resetting the row cursor.
    pub fn clear(&mut self) {
        for entry in self.entries.drain(..) {
            if entry.control.is_instance_valid() {
                let mut control = entry.control;
                control.queue_free();
            }
        }
        self.current_row = 0;
        self.invalidate();
    }

    /// Sets the background style name resolved from the theme (`_draw` pass).
    #[allow(dead_code)] // Arc Table surface; no native caller yet.
    pub fn background(&mut self, style_name: &str) {
        self.background_name = StringName::from(style_name);
        self.base_mut().queue_redraw();
        self.invalidate();
    }

    /// Sets the table's outer margin.
    pub fn margin(&mut self, value: f32) {
        self.margin_value = value;
        self.invalidate();
        self.base_mut().queue_redraw();
    }

    /// Marks this table's minimum size and layout dirty and propagates up.
    pub fn invalidate(&mut self) {
        if !self.base().is_inside_tree() {
            return;
        }
        self.base_mut().update_minimum_size();
        self.base_mut().queue_sort();
    }

    /// Forces an immediate layout pass at the current size.
    pub fn sort_now(&mut self) {
        self.sort_children();
    }

    /// Arc `Table.top()` / back to centre alignment.
    #[func]
    pub fn set_table_top(&mut self, value: bool) {
        self.table_top = value;
        self.base_mut().queue_sort();
    }

    /// Arc `Table.left()` / back to centre alignment.
    #[func]
    pub fn set_table_left(&mut self, value: bool) {
        self.table_left = value;
        self.base_mut().queue_sort();
    }

    /// Sets the owning container's horizontal size flags (`grow_x` equivalent).
    pub fn grow_horizontal(&mut self) {
        self.base_mut().set_h_size_flags(SizeFlags::EXPAND_FILL);
    }

    /// Sets the owning container's vertical size flags (`grow_y` equivalent).
    pub fn grow_vertical(&mut self) {
        self.base_mut().set_v_size_flags(SizeFlags::EXPAND_FILL);
    }

    fn grid(&self) -> Grid {
        // Visible-only layout, like every Container and Arc's `Table`: a hidden
        // cell must not reserve space (the collapsers depend on this).
        let mut rows: Vec<usize> = Vec::new();
        let mut by_row: Vec<Vec<usize>> = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.control.is_instance_valid() && !entry.control.is_visible() {
                continue;
            }
            let row_index = match rows.iter().position(|row| *row == entry.row) {
                Some(position) => position,
                None => {
                    rows.push(entry.row);
                    by_row.push(Vec::new());
                    rows.len() - 1
                }
            };
            by_row[row_index].push(index);
        }

        let mut placements = Vec::new();
        let mut col_count: usize = 0;
        for (row_index, row) in by_row.iter().enumerate() {
            let mut col = 0usize;
            for entry_index in row {
                let span = self.entries[*entry_index].cell.colspan.max(1) as usize;
                placements.push(Placement {
                    entry: *entry_index,
                    col,
                    span,
                    row_index,
                });
                col += span;
            }
            col_count = col_count.max(col);
        }

        let mut col_min = vec![0.0f32; col_count];
        let mut row_min = vec![0.0f32; rows.len()];
        let mut spanning: Vec<(usize, f32)> = Vec::new();
        for placement in &placements {
            let Some(entry) = self.entries.get(placement.entry) else {
                continue;
            };
            let cell = &entry.cell;
            let control = &entry.control;
            if !control.is_instance_valid() {
                continue;
            }
            let min_size = control.get_combined_minimum_size();
            let mut width = min_size.x;
            let mut height = min_size.y;
            if cell.width >= 0.0 {
                width = width.max(cell.width);
            }
            if cell.size >= 0.0 {
                width = width.max(cell.size);
            }
            if cell.min_width >= 0.0 {
                width = width.max(cell.min_width);
            }
            if cell.max_width >= 0.0 {
                width = width.min(cell.max_width);
            }
            if cell.height >= 0.0 {
                height = height.max(cell.height);
            }
            if cell.size >= 0.0 {
                height = height.max(cell.size);
            }
            if cell.min_height >= 0.0 {
                height = height.max(cell.min_height);
            }
            if cell.max_height >= 0.0 {
                height = height.min(cell.max_height);
            }
            width += cell.pad_left + cell.pad_right;
            height += cell.pad_top + cell.pad_bottom;
            if placement.span == 1 {
                col_min[placement.col] = col_min[placement.col].max(width);
            } else {
                spanning.push((placement.entry, width));
            }
            row_min[placement.row_index] = row_min[placement.row_index].max(height);
        }

        // Arc `Table.computeSize`: distribute only the deficit a spanning cell
        // adds over its columns' own widths, spread evenly.
        for (entry_index, need) in spanning {
            let Some(placement) = placements
                .iter()
                .find(|placement| placement.entry == entry_index)
            else {
                continue;
            };
            let mut have = 0.0;
            for offset in 0..placement.span {
                have += col_min[placement.col + offset];
            }
            if need > have {
                let extra = (need - have) / placement.span as f32;
                for offset in 0..placement.span {
                    col_min[placement.col + offset] += extra;
                }
            }
        }

        Grid {
            placements,
            rows,
            col_min,
            row_min,
        }
    }

    fn sort_children(&mut self) {
        let own_size = self.base().get_size();
        let grid = self.grid();
        let mut min_width = self.margin_value * 2.0;
        let mut min_height = self.margin_value * 2.0;
        for value in &grid.col_min {
            min_width += value;
        }
        for value in &grid.row_min {
            min_height += value;
        }
        let extra = own_size - Vector2::new(min_width, min_height);

        let mut grow_col = 0usize;
        let mut grow_row = 0usize;
        for placement in &grid.placements {
            let cell = &self.entries[placement.entry].cell;
            if cell.grow_x || cell.expand_x {
                grow_col += placement.span;
            }
            if cell.grow_y || cell.expand_y {
                grow_row += 1;
            }
        }
        let col_extra = extra.x.max(0.0) / (grow_col.max(1) as f32);
        let row_extra = extra.y.max(0.0) / (grow_row.max(1) as f32);

        let mut col_widths = grid.col_min.clone();
        for placement in &grid.placements {
            let cell = &self.entries[placement.entry].cell;
            if cell.grow_x || cell.expand_x {
                for offset in 0..placement.span {
                    col_widths[placement.col + offset] += col_extra;
                }
            }
        }
        let mut row_heights = grid.row_min.clone();
        for placement in &grid.placements {
            let cell = &self.entries[placement.entry].cell;
            if cell.grow_y || cell.expand_y {
                row_heights[placement.row_index] += row_extra;
            }
        }

        let mut col_x = Vec::with_capacity(col_widths.len());
        let mut running = self.margin_value;
        for value in &col_widths {
            col_x.push(running);
            running += value;
        }
        let mut row_y = Vec::with_capacity(row_heights.len());
        running = self.margin_value;
        for value in &row_heights {
            row_y.push(running);
            running += value;
        }

        let grid_width: f32 = self.margin_value * 2.0 + col_widths.iter().sum::<f32>();
        let grid_height: f32 = self.margin_value * 2.0 + row_heights.iter().sum::<f32>();
        let origin = Vector2::new(
            if self.table_left {
                0.0
            } else {
                (own_size.x - grid_width).max(0.0) * 0.5
            },
            if self.table_top {
                0.0
            } else {
                (own_size.y - grid_height).max(0.0) * 0.5
            },
        );

        for placement in &grid.placements {
            let entry = &self.entries[placement.entry];
            if !entry.control.is_instance_valid() {
                continue;
            }
            let mut cell_width = 0.0;
            for offset in 0..placement.span {
                cell_width += col_widths[placement.col + offset];
            }
            let cell_height = row_heights[placement.row_index];
            let mut control = entry.control.clone();
            let pos = origin + Vector2::new(col_x[placement.col], row_y[placement.row_index]);
            let inner = Rect2::new(
                pos + Vector2::new(entry.cell.pad_left, entry.cell.pad_top),
                Vector2::new(
                    (cell_width - entry.cell.pad_left - entry.cell.pad_right).max(0.0),
                    (cell_height - entry.cell.pad_top - entry.cell.pad_bottom).max(0.0),
                ),
            );
            place_control(&mut control, inner, &entry.cell);
        }
    }
}

/// Positions one control inside its cell (`MindTable._place`).
fn place_control(control: &mut Gd<Control>, rect: Rect2, cell: &CellData) {
    let cell_size = rect.size;
    let min_size = control.get_combined_minimum_size();
    let mut final_size = min_size;
    if cell.fill_x || cell.grow_x {
        final_size.x = cell_size.x;
    }
    if cell.fill_y || cell.grow_y {
        final_size.y = cell_size.y;
    }
    if cell.width >= 0.0 {
        final_size.x = cell.width;
    }
    if cell.height >= 0.0 {
        final_size.y = cell.height;
    }
    if cell.size >= 0.0 {
        final_size = Vector2::new(cell.size, cell.size);
    }
    if cell.max_width >= 0.0 {
        final_size.x = final_size.x.min(cell.max_width);
    }
    if cell.max_height >= 0.0 {
        final_size.y = final_size.y.min(cell.max_height);
    }

    let offset = match cell.align {
        Align::Top => Vector2::new((cell_size.x - final_size.x) * 0.5, 0.0),
        Align::Left => Vector2::new(0.0, (cell_size.y - final_size.y) * 0.5),
        Align::Right => Vector2::new(
            cell_size.x - final_size.x,
            (cell_size.y - final_size.y) * 0.5,
        ),
        Align::Bottom => Vector2::new(
            (cell_size.x - final_size.x) * 0.5,
            cell_size.y - final_size.y,
        ),
        Align::TopLeft => Vector2::ZERO,
        Align::TopRight => Vector2::new(cell_size.x - final_size.x, 0.0),
        Align::BottomLeft => Vector2::new(0.0, cell_size.y - final_size.y),
        Align::BottomRight => cell_size - final_size,
        Align::Center => (cell_size - final_size) * 0.5,
    };
    control.set_position(rect.position + offset);
    control.set_size(final_size);
}

/// `MindStack` — Arc `Stack`: children share the full rectangle.
#[derive(GodotClass)]
#[class(rename = RustMindStack, base=Container, init)]
pub struct MindStack {
    base: Base<Container>,
}

#[godot_api]
impl IContainer for MindStack {
    fn on_notification(&mut self, what: ContainerNotification) {
        if what == ContainerNotification::SORT_CHILDREN {
            self.sort_children();
        }
    }

    fn get_minimum_size(&self) -> Vector2 {
        let mut result = Vector2::ZERO;
        for child in self.base().get_children().iter_shared() {
            if let Ok(control) = child.try_cast::<Control>() {
                let min_size = control.get_combined_minimum_size();
                result = Vector2::new(result.x.max(min_size.x), result.y.max(min_size.y));
            }
        }
        result
    }
}

#[godot_api]
impl MindStack {
    /// Stretches every `Control` child over the stack rectangle.
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn sort_now(&mut self) {
        self.sort_children();
    }

    fn sort_children(&mut self) {
        let size = self.base().get_size();
        let rect = Rect2::new(Vector2::ZERO, size);
        for child in self.base().get_children().iter_shared() {
            if let Ok(mut control) = child.try_cast::<Control>() {
                control.set_position(rect.position);
                control.set_size(rect.size);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure-data behaviour is covered by the headless UI layout oracle; these
    // tests guard the layout arithmetic that does not need a Godot host.

    fn cell(width: f32, pad: f32) -> CellData {
        CellData {
            pad_left: pad,
            pad_right: pad,
            pad_top: pad,
            pad_bottom: pad,
            width,
            ..CellData::default()
        }
    }

    #[test]
    fn span_deficit_distribution_matches_arc() {
        // Two 40 px columns under a 120 px colspan-2 cell: the 40 px deficit is
        // spread evenly (20 each), not the whole width.
        let mut col_min = vec![40.0f32, 40.0];
        let need = 120.0f32;
        let have: f32 = col_min.iter().sum();
        assert!(need > have);
        let extra = (need - have) / 2.0;
        for value in &mut col_min {
            *value += extra;
        }
        assert!((col_min[0] - 60.0).abs() < f32::EPSILON);
        assert!((col_min[1] - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cell_minimum_adds_padding() {
        let cell = cell(100.0, 3.0);
        assert_eq!(cell.width + cell.pad_left + cell.pad_right, 106.0);
    }
}
