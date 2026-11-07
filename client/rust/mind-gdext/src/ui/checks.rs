// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: arc/scene/ui/layout/Table.java (`computeSize`, colspan deficit),
//         arc/scene/ui/layout/WidgetGroup.java:98-106
//         (`invalidateHierarchy`), plus the project-local
//         `client/tools/{shaders_check,ui_layout_check}.gd` oracles replaced here.

//! Native headless UI oracles.
//!
//! The `client/tools/*.gd` `SceneTree` scripts are replaced by native scene
//! roots that drive the same assertions and exit with the same code. `ci.sh`
//! runs the scenes:
//!
//! ```text
//! godot4 --headless --path client res://scenes/tools/shaders_check.tscn
//! godot4 --headless --path client res://scenes/tools/ui_layout_check.tscn
//! godot4 --headless --path client res://scenes/tools/ui_widgets_check.tscn
//! ```
//!
//! - [`MindShadersCheck`] — every `res://shaders/*.gdshader` loads and reports
//!   its declared uniforms (a shader that fails to compile reports zero).
//! - [`MindUiLayoutCheck`] — `MindTable` minimum-size propagation through
//!   `CenterContainer`/`MindScroll`, the PAR-0410 nesting, hidden cells,
//!   `margin()`, `Table.top()/left()` and colspan deficit distribution.
//! - [`MindDialogSweepCheck`] — every manifest dialog scene instantiates through
//!   its native controller and shows/hides/re-shows (the `DialogController`
//!   build seam the editor import cannot exercise).
//!
//! Layout assertions and the next mutation run one step per frame: a
//! container's placement lands in the next layout pass (the GDScript
//! `await process_frame`).

use godot::classes::control::SizeFlags;
use godot::classes::scroll_container::ScrollMode;
use godot::classes::{
    Button, CenterContainer, ColorRect, Control, DirAccess, FileAccess, HBoxContainer, HSlider,
    INode, Label, Node, PackedScene, PanelContainer, ResourceLoader, ScrollContainer, Shader,
    ShaderMaterial, TextureRect, VBoxContainer, Window,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use super::dialog::MindDialog;
use super::kit::widgets::check::MindCheck;
use super::kit::{self, MindScroll, MindTable};

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// Half-pixel tolerance for layout comparisons (`_near` in the oracle).
const EPSILON: f32 = 0.5;

/// A fixed-minimum leaf cell (`_leaf`).
fn leaf(width: f32, height: f32) -> Gd<Control> {
    // code-instantiated: the oracle builds its measured leaves at runtime.
    let mut control = Control::new_alloc();
    control.set_custom_minimum_size(Vector2::new(width, height));
    control
}

/// Whether two vectors agree within [`EPSILON`].
fn near(a: Vector2, b: Vector2) -> bool {
    (a.x - b.x).abs() <= EPSILON && (a.y - b.y).abs() <= EPSILON
}

/// Upcasts a user class to its engine `Control` handle.
fn as_control<T: Inherits<Control>>(node: &Gd<T>) -> Gd<Control> {
    node.clone().upcast()
}

/// The scene-tree root, when the node is inside a tree.
fn scene_root(node: &Node) -> Option<Gd<Window>> {
    Some(node.get_tree().get_root())
}

/// `get_tree().quit(code)` — gdext generates the zero-argument overload, so the
/// exit code goes through a dynamic call (the oracle's pass/fail signal for CI).
fn quit_with_code(node: &Node, code: i32) {
    let mut tree = node.get_tree();
    tree.call("quit", &[code.to_variant()]);
}

// ---------------------------------------------------------------------------
// MindShadersCheck
// ---------------------------------------------------------------------------

/// Directory holding the ported `.gdshader` files.
const SHADER_DIR: &str = "res://shaders";

/// `shaders_check.gd`: every shader must compile under the dummy driver.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindShadersCheck {
    base: Base<Node>,
}

#[godot_api]
impl INode for MindShadersCheck {
    fn init(base: Base<Node>) -> Self {
        Self { base }
    }

    fn ready(&mut self) {
        let (total, failed) = run_shader_checks();
        godot_print!("SHADERCHECK: total={total} failed={failed}");
        let code = if failed > 0 { 1 } else { 0 };
        quit_with_code(&self.base(), code);
    }
}

/// Counts `uniform ` declaration lines in a shader source.
fn declared_uniform_count(source: &str) -> usize {
    source
        .lines()
        .filter(|line| line.trim_start().starts_with("uniform "))
        .count()
}

/// Loads and probes every `.gdshader`; returns `(total, failed)`.
fn run_shader_checks() -> (i32, i32) {
    let Some(dir) = DirAccess::open(SHADER_DIR) else {
        godot_error!("shaders_check: cannot open {SHADER_DIR}");
        return (0, 1);
    };
    let mut files: Vec<String> = dir
        .get_files()
        .as_slice()
        .iter()
        .map(|name| name.to_string())
        .filter(|name| name.ends_with(".gdshader"))
        .collect();
    files.sort();
    let mut loader = ResourceLoader::singleton();
    let mut total = 0;
    let mut failed = 0;
    for file in files {
        total += 1;
        let path = format!("{SHADER_DIR}/{file}");
        let source = FileAccess::get_file_as_string(&GString::from(path.as_str())).to_string();
        let Some(resource) = loader.load(&GString::from(path.as_str())) else {
            godot_error!("SHADERCHECK: LOAD FAILED {path}");
            failed += 1;
            continue;
        };
        let Ok(shader) = resource.try_cast::<Shader>() else {
            godot_error!("SHADERCHECK: NOT A SHADER {path}");
            failed += 1;
            continue;
        };
        // Assigning the shader forces the compiler front-end:
        // `get_shader_uniform_list()` compiles, and a shader that fails reports
        // zero uniforms even when the source declares some.
        let mut material = ShaderMaterial::new_gd();
        material.set_shader(&shader);
        let compiled = shader.get_shader_uniform_list().len() as i32;
        let declared = declared_uniform_count(&source);
        if declared > 0 && compiled == 0 {
            godot_error!("SHADERCHECK: COMPILE FAILED {file} (declared={declared} compiled=0)");
            failed += 1;
            continue;
        }
        godot_print!("SHADERCHECK: {file} uniforms={compiled}");
    }
    (total, failed)
}

// ---------------------------------------------------------------------------
// MindUiLayoutCheck
// ---------------------------------------------------------------------------

/// PAR-0410 nesting case: `ScrollContainer > VBoxContainer > MindTable > MindTable`.
const NESTED_AREA: Vector2 = Vector2::new(1136.0, 493.0);
/// `MapListDialog`-shaped tile size.
const NESTED_TILE: Vector2 = Vector2::new(200.0, 252.0);
/// Row pitch after `pad(8)`: 252 + 16.
const NESTED_ROW_HEIGHT: f32 = 268.0;

/// `ui_layout_check.gd`: `MindTable` minimum-size propagation and layout.
///
/// Each step asserts the previous mutation and performs the next one, so every
/// frame boundary is exactly one layout pass (`await process_frame`).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindUiLayoutCheck {
    base: Base<Node>,
    step: usize,
    failures: i32,

    center: Option<Gd<CenterContainer>>,
    holder: Option<Gd<Control>>,
    scroll: Option<Gd<MindScroll>>,
    body: Option<Gd<ScrollContainer>>,
    outer: Option<Gd<MindTable>>,
    table: Option<Gd<MindTable>>,
    cell: Option<Gd<Control>>,
    sibling: Option<Gd<Control>>,
    span: Option<Gd<Control>>,
}

#[godot_api]
impl INode for MindUiLayoutCheck {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            step: 0,
            failures: 0,
            center: None,
            holder: None,
            scroll: None,
            body: None,
            outer: None,
            table: None,
            cell: None,
            sibling: None,
            span: None,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        self.run_step();
    }
}

impl MindUiLayoutCheck {
    /// One assertion; prints `ok`/`FAIL` like the GDScript oracle.
    fn check(&mut self, condition: bool, message: &str) {
        if condition {
            godot_print!("  ok   {message}");
        } else {
            self.failures += 1;
            godot_error!("  FAIL {message}");
        }
    }

    /// Adds a node under the scene root.
    fn attach<N: Inherits<Node>>(&mut self, node: &Gd<N>) {
        let Some(mut root) = scene_root(&self.base()) else {
            return;
        };
        root.add_child(node);
    }

    /// Runs the next step; quits after the last one.
    fn run_step(&mut self) {
        match self.step {
            0 => self.case1_build(),
            1 => self.case1_assert_and_refill(),
            2 => self.case1_refill_assert_and_case2(),
            3 => self.case2_assert_and_refill(),
            4 => self.case2_refill_assert_and_case3(),
            5 => self.case3_assert_and_filter(),
            6 => self.case3_filtered_and_case4(),
            7 => self.case4_assert_and_hide(),
            8 => self.case4_hidden_and_margin(),
            9 => self.case4_margin_and_case5(),
            10 => self.case5_assert_and_top(),
            11 => self.case5_top_and_left(),
            12 => self.case5_left_and_case6(),
            13 => self.case6_assert_and_widen(),
            14 => self.case6_spread_and_finish(),
            _ => self.finish(),
        }
        self.step += 1;
    }

    /// Prints the summary and quits with the failure code.
    fn finish(&mut self) {
        if let Some(node) = self.center.take() {
            node.free();
        }
        if let Some(node) = self.holder.take() {
            node.free();
        }
        if let Some(node) = self.scroll.take() {
            node.free();
        }
        if let Some(node) = self.body.take() {
            node.free();
        }
        godot_print!("LAYOUTCHECK: failed={}", self.failures);
        let code = if self.failures > 0 { 1 } else { 0 };
        quit_with_code(&self.base(), code);
    }

    // -- case 1: CenterContainer ----------------------------------------------

    fn case1_build(&mut self) {
        godot_print!("case 1: MindTable in a CenterContainer");
        // code-instantiated: the oracle assembles its measured tree at runtime.
        let mut center = CenterContainer::new_alloc();
        center.set_size(Vector2::new(800.0, 600.0));
        self.attach(&center);
        let mut table = MindTable::new_alloc();
        center.add_child(&table);
        for index in 0..6 {
            if index > 0 && index % 3 == 0 {
                table.bind_mut().row();
            }
            let cell = leaf(50.0, 40.0);
            table.bind_mut().add(&cell).pad(4.0);
        }
        self.center = Some(center);
        self.table = Some(table);
    }

    fn case1_assert_and_refill(&mut self) {
        let (Some(mut table), Some(center)) = (self.table.clone(), self.center.clone()) else {
            return;
        };
        let table_control = as_control(&table);
        let first_min = table_control.get_combined_minimum_size();
        self.check(
            near(first_min, Vector2::new(174.0, 96.0)),
            &format!("6 cells (3x2, pad 4) measure 174x96 (got {first_min})"),
        );
        self.check(
            near(table_control.get_size(), first_min),
            &format!(
                "parent sized the table to its minimum (got {})",
                table_control.get_size()
            ),
        );
        self.check(
            near(
                table_control.get_position(),
                (center.get_size() - first_min) * 0.5,
            ),
            &format!(
                "CenterContainer centred the 6-cell table (got {})",
                table_control.get_position()
            ),
        );

        table.bind_mut().clear();
        let wide = leaf(10.0, 30.0);
        // Width set after `add()`: the chained cell setter must invalidate the
        // table (`Table.invalidate`), not just the cell.
        table.bind_mut().add(&wide).width(300.0).pad(2.0);
        self.cell = Some(wide);
    }

    fn case1_refill_assert_and_case2(&mut self) {
        let (Some(table), Some(center), Some(wide)) =
            (self.table.clone(), self.center.clone(), self.cell.clone())
        else {
            return;
        };
        let table_control = as_control(&table);
        let wide_control = as_control(&wide);
        let new_min = table_control.get_combined_minimum_size();
        self.check(
            near(new_min, Vector2::new(304.0, 34.0)),
            &format!("refilled table measures 304x34 (got {new_min})"),
        );
        self.check(
            near(table_control.get_size(), new_min),
            &format!(
                "parent rect equals the new minimum after one frame (got {})",
                table_control.get_size()
            ),
        );
        self.check(
            near(
                table_control.get_position(),
                (center.get_size() - new_min) * 0.5,
            ),
            &format!(
                "CenterContainer re-centred the refilled table (got {})",
                table_control.get_position()
            ),
        );
        self.check(
            near(wide_control.get_position(), Vector2::new(2.0, 2.0))
                && near(wide_control.get_size(), Vector2::new(300.0, 30.0)),
            &format!(
                "the wide cell is laid out without sort_now() (got {} {})",
                wide_control.get_position(),
                wide_control.get_size()
            ),
        );
        if let Some(center) = self.center.take() {
            center.free();
        }

        godot_print!("case 2: MindTable inside MindScroll");
        // code-instantiated: the scroll pane under test has no scene instance.
        let mut scroll = MindScroll::new_alloc();
        as_control(&scroll).set_size(Vector2::new(400.0, 300.0));
        self.attach(&scroll);
        let mut content = scroll.bind_mut().content();
        for _ in 0..6 {
            let cell = leaf(100.0, 80.0);
            content.bind_mut().add(&cell);
            content.bind_mut().row();
        }
        self.scroll = Some(scroll);
        self.table = Some(content);
    }

    // -- case 2: MindScroll ---------------------------------------------------

    fn case2_assert_and_refill(&mut self) {
        let (Some(scroll), Some(mut table)) = (self.scroll.clone(), self.table.clone()) else {
            return;
        };
        let table_control = as_control(&table);
        let min = table_control.get_combined_minimum_size();
        self.check(
            near(min, Vector2::new(100.0, 480.0)),
            &format!("6 stacked cells measure 100x480 (got {min})"),
        );
        self.check(
            near(
                Vector2::new(table_control.get_size().y, 480.0),
                Vector2::splat(480.0),
            ),
            &format!(
                "tall content table is 480 high (got {})",
                table_control.get_size().y
            ),
        );
        self.check(
            scroll.bind().has_scroll(),
            "the scroll pane scrolls the tall content",
        );

        table.bind_mut().clear();
        let wide = leaf(10.0, 40.0);
        table.bind_mut().add(&wide).width(360.0);
        self.cell = Some(wide);
    }

    fn case2_refill_assert_and_case3(&mut self) {
        let (Some(scroll), Some(table), Some(wide)) =
            (self.scroll.clone(), self.table.clone(), self.cell.clone())
        else {
            return;
        };
        let table_control = as_control(&table);
        let wide_control = as_control(&wide);
        let area = as_control(&scroll).get_size();
        self.check(
            near(
                table_control.get_combined_minimum_size(),
                Vector2::new(360.0, 40.0),
            ),
            &format!(
                "refilled content measures 360x40 (got {})",
                table_control.get_combined_minimum_size()
            ),
        );
        self.check(
            near(table_control.get_size(), area),
            &format!(
                "content table fills the scroll area again after one frame (got {}, area {area})",
                table_control.get_size()
            ),
        );
        self.check(
            !scroll.bind().has_scroll(),
            "no stale scroll range after the refill",
        );
        self.check(
            near(
                wide_control.get_position(),
                Vector2::new((area.x - 360.0) * 0.5, (area.y - 40.0) * 0.5),
            ),
            &format!(
                "the wide cell is centred in the content table (Table align center; got {})",
                wide_control.get_position()
            ),
        );
        if let Some(scroll) = self.scroll.take() {
            scroll.free();
        }

        godot_print!("case 3: PAR-0410 nesting (scroll > vbox > table > grid), filter rebuild");
        // code-instantiated: reproduces the map-list dialog hierarchy in code.
        let mut body = ScrollContainer::new_alloc();
        body.set_size(NESTED_AREA);
        body.set_horizontal_scroll_mode(ScrollMode::DISABLED);
        self.attach(&body);
        let mut content = VBoxContainer::new_alloc();
        content.set_h_size_flags(SizeFlags::EXPAND_FILL);
        body.add_child(&content);
        let mut outer = MindTable::new_alloc();
        as_control(&outer).set_h_size_flags(SizeFlags::EXPAND_FILL);
        content.add_child(&outer);
        let mut grid = MindTable::new_alloc();
        outer
            .bind_mut()
            .add(&grid)
            .expand_x()
            .pad_left(28.0)
            .pad_right(12.0);
        let mut last: Option<Gd<Control>> = None;
        for index in 0..30 {
            if index > 0 && index % 5 == 0 {
                grid.bind_mut().row();
            }
            let cell = leaf(NESTED_TILE.x, NESTED_TILE.y);
            grid.bind_mut()
                .add(&cell)
                .width(NESTED_TILE.x)
                .bottom()
                .pad(8.0);
            last = Some(cell);
        }
        self.body = Some(body);
        self.outer = Some(outer);
        self.table = Some(grid);
        self.sibling = last;
    }

    // -- case 3: PAR-0410 nesting ---------------------------------------------

    fn case3_assert_and_filter(&mut self) {
        let Some(mut grid) = self.table.clone() else {
            return;
        };
        let Some(last) = self.sibling.clone() else {
            return;
        };
        let mut grid_control = as_control(&grid);
        let last_control = as_control(&last);
        let full_height = grid_control.get_size().y;
        self.check(
            (full_height - 6.0 * NESTED_ROW_HEIGHT).abs() < f32::EPSILON,
            &format!("unfiltered grid is 6 rows high (got {full_height})"),
        );
        self.check(
            near(
                last_control.get_position(),
                Vector2::new(4.0 * 216.0 + 8.0, 5.0 * NESTED_ROW_HEIGHT + 8.0),
            ),
            &format!(
                "the 30th tile sits in row 6, column 5 (got {})",
                last_control.get_position()
            ),
        );
        // The dialog used to force the stale rect itself (`sort_now` at the old
        // size); reproduce that so the refill must shrink an explicitly sized
        // grid.
        grid_control.set_size(Vector2::new(1080.0, full_height));

        grid.bind_mut().clear();
        let tile = leaf(NESTED_TILE.x, NESTED_TILE.y);
        grid.bind_mut()
            .add(&tile)
            .width(NESTED_TILE.x)
            .bottom()
            .pad(8.0);
        self.cell = Some(tile);
    }

    fn case3_filtered_and_case4(&mut self) {
        let (Some(body), Some(outer), Some(grid), Some(tile)) = (
            self.body.clone(),
            self.outer.clone(),
            self.table.clone(),
            self.cell.clone(),
        ) else {
            return;
        };
        let grid_control = as_control(&grid);
        let outer_control = as_control(&outer);
        let body_control = as_control(&body);
        let tile_control = as_control(&tile);
        let grid_min = grid_control.get_combined_minimum_size();
        self.check(
            near(grid_control.get_size(), grid_min),
            &format!(
                "filtered grid shrinks to its new minimum (got {}, min {grid_min})",
                grid_control.get_size()
            ),
        );
        self.check(
            (outer_control.get_size().y - NESTED_ROW_HEIGHT).abs() < f32::EPSILON,
            &format!(
                "outer table follows (268 high, got {})",
                outer_control.get_size().y
            ),
        );
        // Pane 1136 wide; pads 28/12 leave 1096; the 216 px grid centres in it.
        self.check(
            near(
                grid_control.get_position(),
                Vector2::new(28.0 + (1096.0 - 216.0) * 0.5, 0.0),
            ),
            &format!(
                "outer table centres the filtered grid in its expanded cell (got {})",
                grid_control.get_position()
            ),
        );
        self.check(
            near(tile_control.get_position(), Vector2::new(8.0, 8.0)),
            &format!(
                "the filtered tile is placed at its pad (got {})",
                tile_control.get_position()
            ),
        );
        let tile_y =
            tile_control.get_global_rect().position.y - body_control.get_global_rect().position.y;
        self.check(
            tile_y >= 0.0 && tile_y + NESTED_TILE.y <= body_control.get_size().y,
            &format!("the filtered tile is inside the visible scroll area (y {tile_y})"),
        );
        if let Some(body) = self.body.take() {
            body.free();
        }

        godot_print!("case 4: visibility and margin re-measure");
        // code-instantiated: measured tree built by the oracle.
        let mut center = CenterContainer::new_alloc();
        center.set_size(Vector2::new(600.0, 400.0));
        self.attach(&center);
        let mut table = MindTable::new_alloc();
        center.add_child(&table);
        let a = leaf(100.0, 50.0);
        let b = leaf(100.0, 50.0);
        table.bind_mut().add(&a);
        table.bind_mut().row();
        table.bind_mut().add(&b);
        self.center = Some(center);
        self.table = Some(table);
        self.cell = Some(a);
        self.sibling = Some(b);
    }

    // -- case 4: hidden cells + margin ----------------------------------------

    fn case4_assert_and_hide(&mut self) {
        let Some(table) = self.table.clone() else {
            return;
        };
        self.check(
            near(as_control(&table).get_size(), Vector2::new(100.0, 100.0)),
            &format!(
                "two rows measure 100x100 (got {})",
                as_control(&table).get_size()
            ),
        );
        if let Some(sibling) = self.sibling.clone() {
            let mut control = as_control(&sibling);
            control.set_visible(false);
        }
    }

    fn case4_hidden_and_margin(&mut self) {
        let Some(mut table) = self.table.clone() else {
            return;
        };
        self.check(
            near(as_control(&table).get_size(), Vector2::new(100.0, 50.0)),
            &format!(
                "hiding a cell shrinks the table (got {})",
                as_control(&table).get_size()
            ),
        );
        table.bind_mut().margin(10.0);
    }

    fn case4_margin_and_case5(&mut self) {
        let (Some(table), Some(cell)) = (self.table.clone(), self.cell.clone()) else {
            return;
        };
        self.check(
            near(as_control(&table).get_size(), Vector2::new(120.0, 70.0)),
            &format!(
                "margin(10) grows the table by 20x20 (got {})",
                as_control(&table).get_size()
            ),
        );
        self.check(
            near(as_control(&cell).get_position(), Vector2::new(10.0, 10.0)),
            &format!(
                "the cell sits inside the margin (got {})",
                as_control(&cell).get_position()
            ),
        );
        if let Some(center) = self.center.take() {
            center.free();
        }

        godot_print!("case 5: Table.top()/left() (cont.top(), rule rows) and the centred default");
        // code-instantiated: measured tree built by the oracle.
        let mut holder = Control::new_alloc();
        holder.set_size(Vector2::new(400.0, 300.0));
        self.attach(&holder);
        let mut table = MindTable::new_alloc();
        as_control(&table).set_size(Vector2::new(400.0, 300.0));
        holder.add_child(&table);
        let cell = leaf(100.0, 50.0);
        table.bind_mut().add(&cell);
        self.holder = Some(holder);
        self.table = Some(table);
        self.cell = Some(cell);
    }

    // -- case 5: Table.top()/left() -------------------------------------------

    fn case5_assert_and_top(&mut self) {
        let (Some(mut table), Some(cell)) = (self.table.clone(), self.cell.clone()) else {
            return;
        };
        self.check(
            near(as_control(&cell).get_position(), Vector2::new(150.0, 125.0)),
            &format!(
                "default align centres the grid (got {})",
                as_control(&cell).get_position()
            ),
        );
        table.bind_mut().set_table_top(true);
    }

    fn case5_top_and_left(&mut self) {
        let (Some(mut table), Some(cell)) = (self.table.clone(), self.cell.clone()) else {
            return;
        };
        self.check(
            near(as_control(&cell).get_position(), Vector2::new(150.0, 0.0)),
            &format!(
                "set_table_top hugs the top edge (got {})",
                as_control(&cell).get_position()
            ),
        );
        table.bind_mut().set_table_left(true);
    }

    fn case5_left_and_case6(&mut self) {
        let (Some(_table), Some(cell)) = (self.table.clone(), self.cell.clone()) else {
            return;
        };
        self.check(
            near(as_control(&cell).get_position(), Vector2::ZERO),
            &format!(
                "set_table_left hugs the left edge (got {})",
                as_control(&cell).get_position()
            ),
        );
        if let Some(holder) = self.holder.take() {
            holder.free();
        }

        godot_print!("case 6: colspan cells add only their deficit (Arc Table.computeSize)");
        // code-instantiated: measured tree built by the oracle.
        let mut center = CenterContainer::new_alloc();
        center.set_size(Vector2::new(800.0, 400.0));
        self.attach(&center);
        let mut grid = MindTable::new_alloc();
        center.add_child(&grid);
        let label_cell = leaf(100.0, 20.0);
        let field_cell = leaf(400.0, 20.0);
        grid.bind_mut().add(&label_cell);
        grid.bind_mut().add(&field_cell);
        grid.bind_mut().row();
        let wide = leaf(484.0, 30.0);
        grid.bind_mut().add(&wide).colspan(2);
        self.center = Some(center);
        self.table = Some(grid);
        self.sibling = Some(field_cell);
        self.span = Some(wide);
    }

    // -- case 6: colspan ------------------------------------------------------

    fn case6_assert_and_widen(&mut self) {
        let (Some(grid), Some(field_cell), Some(wide)) =
            (self.table.clone(), self.sibling.clone(), self.span.clone())
        else {
            return;
        };
        self.check(
            near(as_control(&grid).get_size(), Vector2::new(500.0, 50.0)),
            &format!(
                "label + field columns stay 100 + 400 (got {})",
                as_control(&grid).get_size()
            ),
        );
        self.check(
            (as_control(&field_cell).get_position().x - 100.0).abs() < f32::EPSILON,
            &format!(
                "the field starts after the 100 px label column (got {})",
                as_control(&field_cell).get_position().x
            ),
        );
        let mut control = as_control(&wide);
        let mut size = control.get_custom_minimum_size();
        size.x = 600.0;
        control.set_custom_minimum_size(size);
    }

    fn case6_spread_and_finish(&mut self) {
        let (Some(grid), Some(field_cell)) = (self.table.clone(), self.sibling.clone()) else {
            return;
        };
        self.check(
            (as_control(&grid).get_size().x - 600.0).abs() < f32::EPSILON,
            &format!(
                "a wider span spreads its 100 px deficit (got {})",
                as_control(&grid).get_size().x
            ),
        );
        // Columns 150 + 450; the 400 px field centres in its column (150 + 25).
        self.check(
            (as_control(&field_cell).get_position().x - 175.0).abs() < f32::EPSILON,
            &format!(
                "deficit split evenly: columns 150 + 450 (field x {})",
                as_control(&field_cell).get_position().x
            ),
        );
        if let Some(center) = self.center.take() {
            center.free();
        }
        self.finish();
    }
}

// ---------------------------------------------------------------------------
// MindUiWidgetsCheck
// ---------------------------------------------------------------------------

/// The four atlas regions the check widget rotates through (`CHECK_REGIONS`).
const CHECK_REGIONS: [&str; 4] = ["check-off", "check-on", "check-over", "check-on-over"];
/// `Elems.check` row height lower bound (`CHECK_ROW_MIN_HEIGHT`).
const CHECK_ROW_MIN_HEIGHT: f32 = 32.0;
/// `SettingsMenuDialog` check cell (`.height(45f).padTop(7f)`).
const SETTINGS_CHECK_HEIGHT: f32 = 45.0;
/// `Styles.defaultSlider` row height (`sliderKnob` is 42 px tall).
const SLIDER_ROW_HEIGHT: f32 = 42.0;

/// `ui_widgets_check.gd`: the widget/theme/atlas oracle.
///
/// The GDScript oracle also probed per-dialog GDScript internals (`_grid`,
/// `_sliders`, `_cells`, `_select_columns`, …). Those members are private in the
/// native controllers, and the native ports have no GDScript failure mode to
/// guard (no `Cell.left()` to miss, no script `_ready` ordering): the equivalent
/// coverage is the widget atlas checks below, the `MindTable` layout oracle
/// ([`MindUiLayoutCheck`]), and `kit::theme::verify`. What survives verbatim is
/// the dialog-stack contract: a Back button must close through `MindUi`, not
/// just hide the node (the stack entry keeps the pause governor and the menu
/// modal guard latched).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindUiWidgetsCheck {
    base: Base<Node>,
    step: usize,
    failures: i32,
    check: Option<Gd<MindCheck>>,
    table: Option<Gd<MindTable>>,
    dialog: Option<Gd<MindDialog>>,
}

#[godot_api]
impl INode for MindUiWidgetsCheck {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            step: 0,
            failures: 0,
            check: None,
            table: None,
            dialog: None,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        self.run_step();
    }
}

impl MindUiWidgetsCheck {
    /// One assertion; prints `ok`/`FAIL` like the GDScript oracle.
    fn check(&mut self, condition: bool, message: &str) {
        if condition {
            godot_print!("UICHECK: ok {message}");
        } else {
            self.failures += 1;
            godot_error!("UICHECK: FAIL {message}");
        }
    }

    fn attach<N: Inherits<Node>>(&mut self, node: &Gd<N>) {
        let Some(mut root) = scene_root(&self.base()) else {
            return;
        };
        root.add_child(node);
    }

    fn run_step(&mut self) {
        match self.step {
            0 => self.phase_atlas_and_check(),
            1 => self.phase_indicator_and_cell(),
            2 => self.phase_cell_assert_and_shell(),
            3 => self.phase_stack_open(),
            4 => self.phase_stack_close(),
            _ => self.finish(),
        }
        self.step += 1;
    }

    fn finish(&mut self) {
        if let Some(dialog) = self.dialog.take() {
            dialog.free();
        }
        godot_print!("UICHECK: failed={}", self.failures);
        let code = if self.failures > 0 { 1 } else { 0 };
        quit_with_code(&self.base(), code);
    }

    /// Autoloads, atlas regions, the check widget and the theme/slider gates.
    fn phase_atlas_and_check(&mut self) {
        let assets = kit::assets();
        self.check(assets.is_some(), "MindAssets autoload is present");
        let Some(mut assets) = assets else {
            return;
        };
        for region in CHECK_REGIONS {
            self.check(
                assets
                    .bind_mut()
                    .find_region(GString::from(region))
                    .is_some(),
                &format!("atlas region {region} resolves"),
            );
        }

        // code-instantiated: the widget under test builds its icon+label row in
        // code (`Elems.check`); the oracle drives that exact path.
        let mut check = MindCheck::new_alloc();
        check.bind_mut().setup(
            GString::from("Conveyor Placement Pathfinding"),
            true,
            Callable::invalid(),
        );
        let min_height = as_control(&check).get_custom_minimum_size().y;
        self.check(
            min_height >= CHECK_ROW_MIN_HEIGHT,
            &format!("check row minimum height >= 32 (got {min_height})"),
        );
        let indicator = find_texture_rect(&check.clone().upcast::<Node>());
        let on = assets.bind_mut().find_region(GString::from("check-on"));
        self.check(
            indicator.is_some()
                && on.is_some()
                && indicator
                    .as_ref()
                    .and_then(|rect| rect.get_texture())
                    .map(|t| t.instance_id())
                    == on.map(|t| t.instance_id()),
            "checked indicator uses check-on",
        );
        check.bind_mut().set_checked(false);
        let off = assets.bind_mut().find_region(GString::from("check-off"));
        self.check(
            indicator.is_some()
                && off.is_some()
                && indicator
                    .as_ref()
                    .and_then(|rect| rect.get_texture())
                    .map(|t| t.instance_id())
                    == off.map(|t| t.instance_id()),
            "unchecked indicator uses check-off",
        );
        check.bind_mut().set_checked(true);
        self.check = Some(check);

        // Theme gate: every manifest name resolves and the slider row is the
        // 42 px `sliderBack`/`sliderKnob` pair (PAR-0413).
        let theme = kit::theme::build();
        let manifest = kit::theme::load_manifest();
        self.check(
            kit::theme::verify(&theme, &manifest),
            "the theme registers every styles_manifest name",
        );
        let slider_box = theme.get_stylebox("slider", "defaultSlider");
        self.check(
            slider_box
                .as_ref()
                .is_some_and(|b| (b.get_minimum_size().y - SLIDER_ROW_HEIGHT).abs() < f32::EPSILON),
            &format!(
                "defaultSlider box min height equals the 42 px row (got {})",
                slider_box
                    .as_ref()
                    .map(|b| b.get_minimum_size().y)
                    .unwrap_or(-1.0)
            ),
        );
        let knob = theme.get_icon("grabber", "defaultSlider");
        self.check(
            knob.as_ref().is_some_and(|icon| icon.get_height() == 42),
            &format!(
                "defaultSlider knob is the 42 px sliderKnob region (got {})",
                knob.as_ref().map(|icon| icon.get_height()).unwrap_or(-1)
            ),
        );
        let mut probe = super::kit::widgets::factory::slider(0.0, 100.0, 1.0);
        probe.set_theme(&theme);
        self.attach(&probe);
        let probe_height = probe.get_combined_minimum_size().y;
        self.check(
            (probe_height - SLIDER_ROW_HEIGHT).abs() < f32::EPSILON,
            &format!("a settings slider is 42 px tall (got {probe_height})"),
        );
        probe.free();
    }

    /// Rebuilds the settings-row cell (another frame settles the layout).
    fn phase_indicator_and_cell(&mut self) {
        if let Some(check) = self.check.clone() {
            let indicator = find_texture_rect(&check.clone().upcast::<Node>());
            let assets = kit::assets();
            let on = assets.and_then(|mut a| a.bind_mut().find_region(GString::from("check-on")));
            self.check(
                indicator
                    .as_ref()
                    .and_then(|rect| rect.get_texture())
                    .map(|t| t.instance_id())
                    == on.map(|t| t.instance_id()),
                "set_checked(true) restores the check-on indicator",
            );
        }

        // `SettingsMenuDialog` places each check in a 45px cell with 7px top
        // pad (`.height(45f).padTop(7f)`): fill the cell instead of growing it
        // so extra table height is not absorbed into the rows.
        // code-instantiated: the measured settings cell is built by the oracle.
        let mut table = MindTable::new_alloc();
        as_control(&table).set_size(Vector2::new(500.0, 100.0));
        self.attach(&table);
        if let Some(check) = self.check.clone() {
            table
                .bind_mut()
                .add(&check)
                .grow_x()
                .fill_y()
                .min_height(SETTINGS_CHECK_HEIGHT)
                .pad_top(7.0);
        }
        table.bind_mut().row();
        table.bind_mut().sort_now();
        self.table = Some(table);
    }

    /// Asserts the 45 px cell, then builds the dialog shell in code.
    fn phase_cell_assert_and_shell(&mut self) {
        if let (Some(table), Some(check)) = (self.table.clone(), self.check.clone()) {
            let height = as_control(&check).get_size().y;
            self.check(
                (height - SETTINGS_CHECK_HEIGHT).abs() < f32::EPSILON,
                &format!("settings check cell keeps the 45px row (got {height})"),
            );
            let _ = table;
        }
        if let Some(table) = self.table.take() {
            table.free();
        }
        self.check = None;

        // code-instantiated: the shell every dialog scene inherits, rebuilt in
        // code so the oracle drives the native `MindDialog` (the scene files
        // still carry their GDScript until the cutover).
        let mut dialog = MindDialog::new_alloc();
        dialog.set_name("ui_widgets_check_dialog");
        let mut center = CenterContainer::new_alloc();
        center.set_name("Center");
        let mut panel = PanelContainer::new_alloc();
        panel.set_name("Panel");
        let mut layout = VBoxContainer::new_alloc();
        layout.set_name("Layout");
        let mut title = Label::new_alloc();
        title.set_name("Title");
        let mut body = ScrollContainer::new_alloc();
        body.set_name("Body");
        let mut content = VBoxContainer::new_alloc();
        content.set_name("Content");
        let mut buttons = HBoxContainer::new_alloc();
        buttons.set_name("Buttons");
        body.add_child(&content);
        layout.add_child(&title);
        layout.add_child(&body);
        layout.add_child(&buttons);
        panel.add_child(&layout);
        center.add_child(&panel);
        dialog.add_child(&center);
        self.attach(&dialog);
        self.dialog = Some(dialog);
    }

    /// Registers + opens the dialog through `MindUi`, then presses Back.
    fn phase_stack_open(&mut self) {
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        let Some(mut ui) = kit::ui() else {
            self.check(false, "MindUi autoload is present");
            return;
        };
        let name = GString::from("ui_widgets_check_dialog");
        let node: Gd<Node> = dialog.clone().upcast();
        ui.call(
            "register_dialog",
            &[name.to_variant(), node.to_variant(), false.to_variant()],
        );
        let opened = ui
            .call(
                "open_dialog",
                &[name.to_variant(), GString::new().to_variant()],
            )
            .try_to_relaxed::<bool>()
            .unwrap_or(false);
        let has_dialog = ui
            .call("has_dialog", &[])
            .try_to_relaxed::<bool>()
            .unwrap_or(false);
        self.check(
            opened && has_dialog,
            "open_dialog pushes the dialog onto the MindUi stack",
        );
        let mut button = dialog.clone().bind_mut().add_close_button(210.0);
        button.emit_signal("pressed", &[]);
    }

    /// Back must have closed through `MindUi` (stack popped, hide started).
    fn phase_stack_close(&mut self) {
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let has_dialog = ui
            .call("has_dialog", &[])
            .try_to_relaxed::<bool>()
            .unwrap_or(true);
        self.check(
            !has_dialog,
            "Back button pops the dialog off the MindUi stack",
        );
        self.check(
            !dialog.bind().is_shown(),
            "Back button starts hiding the dialog node",
        );
    }
}

/// Depth-first `TextureRect` search (the check widget's indicator).
fn find_texture_rect(node: &Gd<Node>) -> Option<Gd<TextureRect>> {
    for child in node.get_children().iter_shared() {
        if let Ok(rect) = child.clone().try_cast::<TextureRect>() {
            return Some(rect);
        }
        if let Some(found) = find_texture_rect(&child) {
            return Some(found);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// MindDialogSweepCheck
// ---------------------------------------------------------------------------

/// Post-cutover dialog sweep: every manifest dialog scene instantiates through
/// its native controller, builds its button row, and shows/hides/re-shows.
///
/// The editor gate only loads scenes; a controller builds its widgets inside
/// `DialogController::ready`/`shown`, which only runs when a dialog enters the
/// tree. This oracle drives all 59 manifest dialogs together: build, show, hide
/// through the 0.1 s fade, re-show, hide and free. A `bind_mut` panic inside a
/// builder aborts that dialog's `ready`, so the visibility assertions catch it.
/// It also presses `custom_rules`' `@edit` footer button *while the dialog is
/// mutably bound*: `open_edit_menu` mutates the dialog, so it must queue that
/// bind out of the signal instead of re-binding inside it (`PAR-0470`).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindDialogSweepCheck {
    base: Base<Node>,
    dialogs: Vec<(String, Gd<MindDialog>)>,
    step: usize,
    delay: i32,
    poll: i32,
    failures: i32,
}

#[godot_api]
impl INode for MindDialogSweepCheck {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            dialogs: Vec::new(),
            step: 0,
            delay: 0,
            poll: 0,
            failures: 0,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        if self.delay > 0 {
            self.delay -= 1;
            return;
        }
        match self.step {
            0 => {
                self.instantiate_all();
                self.step = 1;
                self.delay = 2;
            }
            1 => {
                for (_, dialog) in &mut self.dialogs {
                    dialog.bind_mut().show_dialog();
                }
                self.step = 2;
                self.delay = 2;
            }
            2 => {
                self.assert_visibility("shows", true);
                // PAR-0470: the `@edit` overlay opener must survive a press made
                // while the dialog's mutable bind is live.
                self.press_custom_rules_edit();
                // PAR-0435/PAR-0426/PAR-0151: drive the measured pages before
                // the layout assertions in the next frame.
                self.press_settings_graphics();
                self.press_database_table_tab();
                self.step = 3;
                self.delay = 2;
            }
            3 => {
                self.assert_custom_rules_overlay();
                // The measured Java twin frames are 1152x648; these assertions
                // pin the three layout fixes from the headless fix pass.
                self.assert_host_layout();
                self.assert_settings_first_row();
                self.assert_database_rule();
                for (_, dialog) in &mut self.dialogs {
                    dialog.bind_mut().hide_dialog();
                }
                self.step = 4;
                self.poll = 0;
            }
            4 => {
                // `hide_dialog` fades for 0.1 s; poll until every fade completes.
                if self.all_hidden() {
                    self.step = 5;
                    self.delay = 2;
                } else {
                    self.poll += 1;
                    if self.poll > 60 {
                        self.step = 5;
                        self.delay = 2;
                    }
                }
            }
            5 => {
                self.assert_visibility("hides (fade completes)", false);
                for (_, dialog) in &mut self.dialogs {
                    dialog.bind_mut().show_dialog();
                }
                self.step = 6;
                self.delay = 2;
            }
            6 => {
                self.assert_visibility("reshows", true);
                for (_, dialog) in &mut self.dialogs {
                    dialog.bind_mut().hide_dialog();
                }
                self.step = 7;
                self.poll = 0;
            }
            7 => {
                if self.all_hidden() || self.poll > 60 {
                    self.step = 8;
                    self.delay = 1;
                } else {
                    self.poll += 1;
                }
            }
            _ => self.finish(),
        }
    }
}

impl MindDialogSweepCheck {
    fn check(&mut self, ok: bool, name: &str, what: &str) {
        if ok {
            godot_print!("DIALOGCHECK: ok {name}: {what}");
        } else {
            self.failures += 1;
            godot_error!("DIALOGCHECK: FAIL {name}: {what}");
        }
    }

    /// Instantiates one dialog per manifest entry and checks its shell.
    fn instantiate_all(&mut self) {
        // The real game applies the theme from `MindUiRoot.enter_tree`; this
        // oracle has no UiRoot. Install the same theme on the first process
        // frame (the autoload atlas is loaded by then) so the dialog layout is
        // comparable to the twin frames.
        let theme = kit::theme::build();
        let text = FileAccess::get_file_as_string("res://ui/dialogs_manifest.json");
        let Ok(manifest) = mind_core::ui::manifest::DialogsManifest::from_json(&text.to_string())
        else {
            self.check(false, "<manifest>", "ui/dialogs_manifest.json parses");
            return;
        };
        let mut loader = ResourceLoader::singleton();
        let mut pending: Vec<(String, Gd<MindDialog>)> = Vec::new();
        for entry in &manifest.dialogs {
            let name = entry.name.clone();
            let Some(resource) = loader.load(&GString::from(entry.scene.as_str())) else {
                self.check(false, &name, "scene loads");
                continue;
            };
            let Ok(packed) = resource.try_cast::<PackedScene>() else {
                self.check(false, &name, "scene is a PackedScene");
                continue;
            };
            let Some(node) = packed.instantiate() else {
                self.check(false, &name, "scene instantiates");
                continue;
            };
            let Ok(dialog) = node.try_cast::<MindDialog>() else {
                self.check(false, &name, "scene roots on MindDialog");
                continue;
            };
            let shell: Gd<Node> = dialog.clone().upcast();
            let buttons = shell
                .get_node_or_null("Center/Panel/Layout/Buttons")
                .is_some();
            self.check(buttons, &name, "button row exists");
            pending.push((name, dialog));
        }
        for (_, dialog) in &pending {
            // The real game's dialogs live under `MindUiRoot`, which carries the
            // theme; this oracle has no UiRoot, so set it per dialog.
            let mut dialog_for_theme = dialog.clone();
            dialog_for_theme.set_theme(&theme);
            let node: Gd<Node> = dialog.clone().upcast();
            self.base_mut().add_child(&node);
        }
        self.dialogs = pending;
    }

    /// Emits `custom_rules`' `@edit` footer button *while the dialog is mutably
    /// bound* (`PAR-0470`).
    ///
    /// `open_edit_menu` needs `&mut MindDialog`, so a synchronous bind inside
    /// the `pressed` handler re-enters the live bind and panics
    /// (`bind_mut() failed, already bound`); with the MCP debugger's
    /// break-on-error that wedges Play's frame loop, and the overlay never
    /// appears. The port queues the bind, so this press must stay bind-free and
    /// the overlay must exist one idle later.
    fn press_custom_rules_edit(&mut self) {
        let Some((_, dialog)) = self.dialogs.iter().find(|(name, _)| name == "custom_rules") else {
            self.check(false, "custom_rules", "dialog instantiated");
            return;
        };
        let mut dialog = dialog.clone();
        let mut bound = dialog.bind_mut();
        let Some(row) = bound.buttons_row() else {
            self.check(false, "custom_rules", "button row exists");
            return;
        };
        let wanted = kit::t("@edit").to_string();
        let mut edit: Option<Gd<Button>> = None;
        for child in row.get_children().iter_shared() {
            let Ok(button) = child.try_cast::<Button>() else {
                continue;
            };
            // An icon button keeps its label in a child `Label`; the `Button`'s
            // own text is empty.
            let labels = button
                .clone()
                .find_children_ex("*")
                .type_("Label")
                .owned(false)
                .done();
            let matches = labels
                .iter_shared()
                .filter_map(|node| node.try_cast::<Label>().ok())
                .any(|label| label.get_text().to_string() == wanted);
            if matches {
                edit = Some(button);
                break;
            }
        }
        let Some(mut edit) = edit else {
            self.check(false, "custom_rules", "`@edit` button exists");
            return;
        };
        // Keep `bound` live across the emission: that is the hazard under test.
        let _ = edit.emit_signal("pressed", &[]);
        self.check(true, "custom_rules", "`@edit` pressed under a dialog bind");
    }

    /// The deferred `@edit` opener must have built the overlay after the press.
    fn assert_custom_rules_overlay(&mut self) {
        let built = self
            .dialogs
            .iter()
            .find(|(name, _)| name == "custom_rules")
            .and_then(|(_, dialog)| dialog.get_node_or_null("rules_overlay"))
            .is_some();
        self.check(
            built,
            "custom_rules",
            "`@edit` overlay built outside the bind",
        );
    }

    /// One instantiated manifest dialog by registry key.
    fn dialog_named(&self, name: &str) -> Option<Gd<MindDialog>> {
        self.dialogs
            .iter()
            .find(|(dialog_name, _)| dialog_name == name)
            .map(|(_, dialog)| dialog.clone())
    }

    /// A named descendant `Control` of a dialog.
    fn descendant(&self, node: &Gd<Node>, name: &str) -> Option<Gd<Control>> {
        let found = node.clone().find_children_ex(name).owned(false).done();
        found
            .iter_shared()
            .find_map(|child| child.try_cast::<Control>().ok())
    }

    /// The first descendant `Button` whose label text equals `wanted`.
    fn button_with_label(&self, node: &Gd<Node>, wanted: &str) -> Option<Gd<Button>> {
        let found = node
            .clone()
            .find_children_ex("*")
            .type_("Button")
            .owned(false)
            .done();
        for child in found.iter_shared() {
            let Ok(button) = child.try_cast::<Button>() else {
                continue;
            };
            let labels = button
                .clone()
                .find_children_ex("*")
                .type_("Label")
                .owned(false)
                .done();
            let wanted = GString::from(wanted);
            let matches = labels
                .iter_shared()
                .filter_map(|label| label.try_cast::<Label>().ok())
                .any(|label| label.get_text() == wanted);
            if matches {
                return Some(button);
            }
        }
        None
    }

    /// `SettingsMenuDialog` category entry: press `@settings.graphics` so the
    /// next frame's assertion measures the Graphics list (`PAR-0435`).
    fn press_settings_graphics(&mut self) {
        let Some(settings) = self.dialog_named("settings") else {
            self.check(false, "settings", "dialog instantiated");
            return;
        };
        let node: Gd<Node> = settings.clone().upcast();
        let wanted = kit::t("@settings.graphics").to_string();
        let Some(mut button) = self.button_with_label(&node, &wanted) else {
            self.check(false, "settings", "`@settings.graphics` menu button exists");
            return;
        };
        let _ = button.emit_signal("pressed", &[]);
    }

    /// PAR-0435: the first Graphics row sits at Java's y=67 in the 1152x648
    /// twin frame (`runs/20261017-L-twin/java/36-graphics.png`). The pre-cutover
    /// GDScript dialog started at 58; the native shell measures 67, so this
    /// pins the settings body inset (`PANE_INSET_TOP` + `prefs.margin(14f)`).
    fn assert_settings_first_row(&mut self) {
        let Some(settings) = self.dialog_named("settings") else {
            self.check(false, "settings", "dialog instantiated");
            return;
        };
        let node: Gd<Node> = settings.clone().upcast();
        let found = node
            .clone()
            .find_children_ex("*")
            .type_("HSlider")
            .owned(false)
            .done();
        let Some(slider) = found
            .iter_shared()
            .find_map(|child| child.try_cast::<HSlider>().ok())
        else {
            self.check(false, "settings", "Graphics page built (a slider exists)");
            return;
        };
        let y = slider.get_global_rect().position.y;
        self.check(
            (y - 67.0).abs() <= 2.0,
            "settings",
            &format!("Graphics first row top {y:.1} px (Java 67)"),
        );
    }

    /// The database opens on the all-content tab; the sweep scene has no
    /// campaign, so select the port-only achievements tab to make the shared
    /// category header/rule stack render (`PAR-0426`).
    fn press_database_table_tab(&mut self) {
        let Some(database) = self.dialog_named("database") else {
            self.check(false, "database", "dialog instantiated");
            return;
        };
        let node: Gd<Node> = database.clone().upcast();
        let Some(button) = self.descendant(&node, "tab-achievements") else {
            self.check(false, "database", "achievements tab exists");
            return;
        };
        let mut button: Gd<Button> = match button.try_cast::<Button>() {
            Ok(button) => button,
            Err(_) => {
                self.check(false, "database", "achievements tab is a button");
                return;
            }
        };
        let _ = button.emit_signal("pressed", &[]);
    }

    /// PAR-0426: the category header stack places its accent rule at Java's
    /// y=168 in the 1152x648 twin frame
    /// (`runs/20261018-s2-L01-T3/diff/coredb-side.png`). The pre-cutover
    /// GDScript dialog sat 10-16 px low; the native shell measures 166 here
    /// (2 px), so this pins the shared search-row/rail/header geometry. The
    /// sweep scene has no campaign, so the port-only achievements tab drives
    /// the same header + rule builder.
    fn assert_database_rule(&mut self) {
        let Some(database) = self.dialog_named("database") else {
            self.check(false, "database", "dialog instantiated");
            return;
        };
        let node: Gd<Node> = database.clone().upcast();
        let found = node
            .clone()
            .find_children_ex("database-rule")
            .owned(false)
            .done();
        let Some(rule) = found
            .iter_shared()
            .find_map(|child| child.try_cast::<ColorRect>().ok())
        else {
            self.check(false, "database", "category rule rendered");
            return;
        };
        let y = rule.get_global_rect().position.y;
        self.check(
            (y - 168.0).abs() <= 3.0,
            "database",
            &format!("first category rule top {y:.1} px (Java 168)"),
        );
    }

    /// PAR-0151: Java's floating host layout — 300x70 name + colour and port
    /// rows, then `[65 spacer][300x70 @host][65x70 ?]`, with no gamemode row
    /// (`HostDialog.java:22-73`).
    fn assert_host_layout(&mut self) {
        let Some(host) = self.dialog_named("host") else {
            self.check(false, "host", "dialog instantiated");
            return;
        };
        let node: Gd<Node> = host.clone().upcast();
        let near = |a: f32, b: f32| (a - b).abs() <= 2.0;
        for (name, width, height) in [
            ("host-name-row", 300.0, 70.0),
            ("host-port-row", 300.0, 70.0),
            ("host-button", 300.0, 70.0),
            ("host-help", 65.0, 70.0),
            ("host-swatch", 54.0, 54.0),
        ] {
            let Some(control) = self.descendant(&node, name) else {
                self.check(false, "host", &format!("`{name}` exists"));
                continue;
            };
            let rect = control.get_global_rect();
            self.check(
                near(rect.size.x, width) && near(rect.size.y, height),
                "host",
                &format!("{name} is {width}x{height} (got {})", rect.size),
            );
        }
        // `cont.add().width(65f)` spacer before `@host`.
        if let Some(spacer) = self.descendant(&node, "host-spacer") {
            self.check(
                near(spacer.get_global_rect().size.x, 65.0),
                "host",
                "spacer is 65 px wide",
            );
        }
        // No gamemode row: Java's HostDialog has none (the port invented one).
        let mode_label = kit::t("@mode.sandbox").to_string();
        self.check(
            self.button_with_label(&node, &mode_label).is_none(),
            "host",
            "no gamemode row (Java has none)",
        );
        // `BaseDialog.setFillParent(true)`: the full-screen `fullDialog` shell
        // (no boxed `defaultDialog` panel around the floating rows).
        if let Some(panel) = host.get_node_or_null("Center/Panel")
            && let Ok(panel) = panel.try_cast::<Control>()
        {
            self.check(
                panel.get_theme_type_variation() == "fullDialog",
                "host",
                &format!(
                    "panel uses the fullDialog shell (got `{}`)",
                    panel.get_theme_type_variation()
                ),
            );
        }
    }

    fn assert_visibility(&mut self, what: &str, expected: bool) {
        let states: Vec<(String, bool)> = self
            .dialogs
            .iter()
            .map(|(name, dialog)| (name.clone(), dialog.bind().is_shown()))
            .collect();
        for (name, shown) in states {
            self.check(shown == expected, &name, what);
        }
    }

    fn all_hidden(&self) -> bool {
        self.dialogs
            .iter()
            .all(|(_, dialog)| !dialog.bind().is_shown())
    }

    fn finish(&mut self) {
        for (_, dialog) in self.dialogs.drain(..) {
            dialog.free();
        }
        godot_print!("DIALOGCHECK: failed={}", self.failures);
        let code = if self.failures > 0 { 1 } else { 0 };
        quit_with_code(&self.base(), code);
    }
}
