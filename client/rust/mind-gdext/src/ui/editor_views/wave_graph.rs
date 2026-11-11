// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The wave plot, its unit legend with hide toggles and the counts/totals/health mode row.
//! Upstream: core/src/mindustry/editor/WaveGraph.java

use super::*;

// ---------------------------------------------------------------------------
// WaveGraph (`wave_graph.gd`, `WaveGraph.java`)
// ---------------------------------------------------------------------------

/// `Mode` names, indexed by the enum ord (`Mode.COUNTS`, `TOTALS`, `HEALTH`).
const MODE_NAMES: [&str; 3] = ["counts", "totals", "health"];
/// `defaultSpace = Scl.scl(40f)`: the column spacing at zoom 1.
const DEFAULT_SPACE: f32 = 40.0;
/// `Pal.health` (`ff341c`).
const HEALTH_COLOR: Color = Color::from_rgba8(0xff, 0x34, 0x1c, 0xff);
/// `Lines.stroke(Scl.scl(3f))`.
const STROKE: f32 = 3.0;

/// One legend toggle (`colors`): the `fullTogglet` button, its colour swatch
/// and the unit icon.
struct LegendEntry {
    name: String,
    button: Gd<Button>,
    swatch: Gd<ColorRect>,
    icon: Gd<TextureRect>,
}

/// `MindWaveGraph` — the editor's per-wave line plot (`WaveGraph`).
///
/// Presentation only: the per-wave series, the axis maxima (`nextStep`) and the
/// participating units come from `MindEditor.wave_graph(from, to)`; this
/// `PanelContainer` draws the `Tex.pane` table of `WaveGraph` — the line plot
/// (pan, wheel zoom, hover column), the unit legend with its hide toggles and
/// the counts/totals/health mode row.
#[derive(GodotClass)]
#[class(base=PanelContainer)]
pub struct MindWaveGraph {
    base: Base<PanelContainer>,

    /// Path to the MindEditor facade.
    #[export]
    pub editor_path: NodePath,

    editor: Option<Gd<Node>>,
    mode: i64,
    pan: f32,
    zoom: f32,
    from: i64,
    to: i64,
    last_from: i64,
    last_to: i64,
    /// `rebuild()` state: the Rust series for `from..to`.
    data: VarDictionary,
    /// `hidden`: unit names whose lines are hidden.
    hidden: Vec<String>,
    plot: Option<Gd<Control>>,
    colors: Option<Gd<HBoxContainer>>,
    hide_button: Option<Gd<Button>>,
    legend: Vec<LegendEntry>,
    mode_buttons: Vec<Gd<Button>>,
    dragging: bool,
}

#[godot_api]
impl IPanelContainer for MindWaveGraph {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            base,
            editor_path: NodePath::from("/root/Spine/MindEditor"),
            editor: None,
            mode: 0,
            pan: 0.0,
            zoom: 1.0,
            from: 0,
            to: 20,
            last_from: -1,
            last_to: -1,
            data: VarDictionary::new(),
            hidden: Vec::new(),
            plot: None,
            colors: None,
            hide_button: None,
            legend: Vec::new(),
            mode_buttons: Vec::new(),
            dragging: false,
        }
    }

    fn ready(&mut self) {
        self.editor = self.editor();
        // `background(Tex.pane)`; `MindThemeBuilder._nine_patch` falls back to
        // a flat #141419 box when the region is missing.
        match self.nine_patch("pane", 12.0) {
            Some(pane) => self.base_mut().add_theme_stylebox_override("panel", &pane),
            None => {
                let mut flat = StyleBoxFlat::new_gd();
                flat.set_bg_color(Color::from_rgba(0.08, 0.08, 0.10, 1.0));
                self.base_mut().add_theme_stylebox_override("panel", &flat);
            }
        }
        // code-instantiated: the WaveGraph table rows (plot, legend, modes).
        let mut rows = VBoxContainer::new_alloc();
        rows.add_theme_constant_override("separation", 0);
        self.base_mut().add_child(&rows);
        // code-instantiated: the plot cell's `.pad(4).padBottom(10).grow()`.
        let mut plot_margin = MarginContainer::new_alloc();
        plot_margin.add_theme_constant_override("margin_left", 4);
        plot_margin.add_theme_constant_override("margin_top", 4);
        plot_margin.add_theme_constant_override("margin_right", 4);
        plot_margin.add_theme_constant_override("margin_bottom", 10);
        plot_margin.set_v_size_flags(SizeFlags::EXPAND_FILL);
        rows.add_child(&plot_margin);
        // code-instantiated: the clipped plot canvas (`_plot`), drawn from its
        // `draw` signal exactly like the GDScript.
        let mut plot = Control::new_alloc();
        plot.set_clip_contents(true);
        plot.set_mouse_filter(MouseFilter::STOP);
        plot.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let graph = self.to_gd();
        let plot_for_draw = plot.clone();
        let draw_callable =
            Callable::from_fn("wave_graph_plot_draw", move |_args: &[&Variant]| {
                if !graph.is_instance_valid() || !plot_for_draw.is_instance_valid() {
                    return;
                }
                let mut graph = graph.clone();
                let mut plot = plot_for_draw.clone();
                graph.bind_mut().draw_plot(&mut plot);
            });
        plot.connect("draw", &draw_callable);
        let graph = self.to_gd();
        let input_callable =
            Callable::from_fn("wave_graph_plot_input", move |args: &[&Variant]| {
                let Some(event) = args
                    .first()
                    .and_then(|value| value.try_to::<Gd<InputEvent>>().ok())
                else {
                    return;
                };
                if !graph.is_instance_valid() {
                    return;
                }
                let mut graph = graph.clone();
                graph.bind_mut().plot_input(event);
            });
        plot.connect("gui_input", &input_callable);
        let redraw = plot.callable("queue_redraw");
        plot.connect("mouse_exited", &redraw);
        plot_margin.add_child(&plot);
        self.plot = Some(plot);
        // `table(t -> colors = t).growX()`.
        // code-instantiated: the legend row rebuilt per participating unit.
        let mut colors = HBoxContainer::new_alloc();
        colors.add_theme_constant_override("separation", 0);
        rows.add_child(&colors);
        self.colors = Some(colors);
        // `t.left()` + one `fullTogglet` 130x35 button per `Mode`.
        // code-instantiated: the fixed counts/totals/health mode row.
        let mut modes = HBoxContainer::new_alloc();
        modes.add_theme_constant_override("separation", 0);
        rows.add_child(&modes);
        for (index, mode_name) in MODE_NAMES.iter().enumerate() {
            let mut button = factory::button(&bundle(&format!("wavemode.{mode_name}")));
            button.set_theme_type_variation("fullTogglet");
            button.set_toggle_mode(true);
            button.set_pressed_no_signal(index as i64 == self.mode);
            button.set_custom_minimum_size(Vector2::new(130.0, 35.0));
            let mut args = VarArray::new();
            args.push(&(index as i64).to_variant());
            button.connect("pressed", &self.base().callable("set_mode").bindv(&args));
            modes.add_child(&button);
            self.mode_buttons.push(button);
        }
    }
}

#[godot_api]
impl MindWaveGraph {
    /// Forces `rebuild()` on the next draw (the dialog's groups changed).
    #[func]
    pub fn refresh(&mut self) {
        self.last_from = -1;
        self.last_to = -1;
        if let Some(mut plot) = self.plot.clone() {
            plot.queue_redraw();
        }
    }

    /// `modeRow` click: `_mode = mode` and re-check the row.
    #[func]
    pub fn set_mode(&mut self, mode: i64) {
        self.mode = mode;
        for (index, button) in self.mode_buttons.iter_mut().enumerate() {
            button.set_pressed_no_signal(index as i64 == self.mode);
        }
        if let Some(mut plot) = self.plot.clone() {
            plot.queue_redraw();
        }
    }

    /// One legend toggle (`_toggle_unit(name)`).
    #[func]
    fn on_toggle_unit(&mut self, name: GString) {
        let name = name.to_string();
        if let Some(index) = self.hidden.iter().position(|entry| entry == &name) {
            self.hidden.remove(index);
        } else {
            self.hidden.push(name);
        }
        let units = dict_array(&self.data, "units");
        self.sync_legend(&units);
        if let Some(mut plot) = self.plot.clone() {
            plot.queue_redraw();
        }
    }

    /// `hidden.size == used.size ? clear : addAll(used)`.
    #[func]
    fn on_toggle_all(&mut self) {
        let units = dict_array(&self.data, "units");
        if self.all_hidden(&units) {
            self.hidden.clear();
        } else {
            for unit_value in units.iter_shared() {
                let unit = unit_value.try_to::<VarDictionary>().unwrap_or_default();
                self.hidden.push(dict_string(&unit, "name"));
            }
        }
        self.sync_legend(&units);
        if let Some(mut plot) = self.plot.clone() {
            plot.queue_redraw();
        }
    }
}

impl MindWaveGraph {
    /// The stored `MindEditor` facade, re-resolved through the exported path
    /// when it went invalid.
    fn editor(&self) -> Option<Gd<Node>> {
        cached_editor(
            &self.editor,
            &self.to_gd().upcast::<Node>(),
            &self.editor_path,
        )
    }

    /// `MindEditor.wave_graph(from, to)` (`{}` when absent/invalid).
    fn wave_graph_data(&self, from: i64, to: i64) -> VarDictionary {
        let Some(mut editor) = self.editor() else {
            return VarDictionary::new();
        };
        if !editor.has_method("wave_graph") {
            return VarDictionary::new();
        }
        editor
            .call("wave_graph", &[from.to_variant(), to.to_variant()])
            .try_to::<VarDictionary>()
            .unwrap_or_default()
    }

    /// A `Tex.*` 9-patch with uniform texture margins (`_nine_patch(region, m)`).
    fn nine_patch(&self, region: &str, margin: f32) -> Option<Gd<StyleBoxTexture>> {
        let texture = kit::icon_texture(region)?;
        let mut box_ = StyleBoxTexture::new_gd();
        box_.set_texture(&texture);
        for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
            box_.set_texture_margin(side, margin);
        }
        Some(box_)
    }

    /// `scrolled`: `zoom -= scroll * 2 / 10 * zoom`; `pan -= deltaX / zoom`.
    fn plot_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(button) = event.clone().try_cast::<InputEventMouseButton>() {
            let index = button.get_button_index();
            if button.is_pressed()
                && (index == MouseButton::WHEEL_DOWN || index == MouseButton::WHEEL_UP)
            {
                let scroll = if index == MouseButton::WHEEL_DOWN {
                    1.0
                } else {
                    -1.0
                };
                self.zoom = (self.zoom - scroll * 2.0 / 10.0 * self.zoom).clamp(0.5, 40.0);
                if let Some(mut plot) = self.plot.clone() {
                    plot.accept_event();
                }
            } else if index == MouseButton::LEFT {
                self.dragging = button.is_pressed();
            }
        } else if let Ok(motion) = event.try_cast::<InputEventMouseMotion>()
            && self.dragging
        {
            self.pan -= motion.get_relative().x / self.zoom;
        }
        if let Some(mut plot) = self.plot.clone() {
            plot.queue_redraw();
        }
    }

    /// `WaveGraph.rect(...)`, in Godot's y-down plot space (`y` = bottom - Java y).
    fn draw_plot(&mut self, plot: &mut Gd<Control>) {
        let width = plot.get_size().x;
        let height = plot.get_size().y;
        let Some(font) = self.base().get_theme_default_font() else {
            return;
        };
        let font_size = self.base().get_theme_default_font_size();
        // `lay.setText(font, "1"); fh = lay.height` (cap height).
        let fh = font.get_ascent_ex().font_size(font_size).done() * 0.7;
        let spacing = self.zoom * DEFAULT_SPACE;
        self.pan = self.pan.max((width / 2.0) / self.zoom - DEFAULT_SPACE);
        let offset_y = 22.0 + fh + 5.0;
        let graph_x = -self.pan * self.zoom + width / 2.0;
        let graph_h = height - offset_y;
        let base_y = graph_h;
        let left = (0.0 - graph_x) / spacing;
        let right = (width - graph_x) / spacing;
        self.from = left as i64 - 1;
        self.to = right as i64 + 1;
        if self.last_from != self.from || self.last_to != self.to {
            self.rebuild();
        }
        self.last_from = self.from;
        self.last_to = self.to;

        let series = dict_array(&self.data, "series");
        let units = dict_array(&self.data, "units");
        let max_y_values = self
            .data
            .get("max_y")
            .and_then(|value| value.try_to::<PackedInt64Array>().ok())
            .unwrap_or_default();
        let mode_index = usize::try_from(self.mode).unwrap_or(usize::MAX);
        let max_y = match max_y_values.as_slice().get(mode_index) {
            Some(value) => (*value).max(1) as f32,
            None => 1.0,
        };
        let unit_count = self
            .data
            .get("unit_count")
            .map(|value| variant_f64(&value))
            .unwrap_or(1.0)
            .max(1.0);
        let visible = self.visible_units(&units);

        let mouse = plot.get_local_mouse_position();
        let inside = Rect2::new(Vector2::ZERO, plot.get_size()).contains_point(mouse);
        let mut selcol = if inside {
            ((mouse.x - graph_x - self.from as f32 * spacing) / spacing).round() as i64
        } else {
            -1
        };
        if selcol + self.from <= -1 {
            selcol = -1;
        }

        let columns = series.len() as i64;
        if self.mode == 0 {
            for unit_index in &visible {
                let mut points = PackedVector2Array::new();
                for i in 0..columns {
                    let row = series
                        .at(i as usize)
                        .try_to::<PackedInt64Array>()
                        .unwrap_or_default();
                    let value = row
                        .as_slice()
                        .get(*unit_index as usize)
                        .copied()
                        .unwrap_or(0) as f32;
                    points.push(Vector2::new(
                        graph_x + (i + self.from) as f32 * spacing,
                        base_y - value * graph_h / max_y,
                    ));
                }
                let unit = units
                    .at(*unit_index as usize)
                    .try_to::<VarDictionary>()
                    .unwrap_or_default();
                let color = unit_color(&unit, unit_count);
                end_line(plot, &points, color);
            }
        } else {
            let mut points = PackedVector2Array::new();
            for i in 0..columns {
                let row = series
                    .at(i as usize)
                    .try_to::<PackedInt64Array>()
                    .unwrap_or_default();
                let mut sum = 0.0f32;
                for unit_index in &visible {
                    let mut weight = 1.0f32;
                    if self.mode == 2 {
                        let unit = units
                            .at(*unit_index as usize)
                            .try_to::<VarDictionary>()
                            .unwrap_or_default();
                        weight = dict_f64(&unit, "health") as f32;
                    }
                    let value = row
                        .as_slice()
                        .get(*unit_index as usize)
                        .copied()
                        .unwrap_or(0) as f32;
                    sum += weight * value;
                }
                points.push(Vector2::new(
                    graph_x + (i + self.from) as f32 * spacing,
                    base_y - sum * graph_h / max_y,
                ));
            }
            let color = if self.mode == 1 {
                styles::ACCENT
            } else {
                HEALTH_COLOR
            };
            end_line(plot, &points, color);
        }

        if selcol >= 0 && selcol < columns {
            let column_x = (selcol + self.from) as f32 * spacing + graph_x - spacing / 2.0;
            plot.draw_rect_ex(
                Rect2::new(Vector2::new(column_x, 0.0), Vector2::new(spacing, graph_h)),
                Color::from_rgba(1.0, 0.0, 0.0, 0.2),
            )
            .filled(true)
            .done();
            // `countStr`: `type.emoji() + " " + amount` per line, scale 1.5.
            let big = (font_size as f32 * 1.5) as i32;
            let line_h = font.get_height_ex().font_size(big).done();
            let mut y = 5.0;
            let row = series
                .at(selcol as usize)
                .try_to::<PackedInt64Array>()
                .unwrap_or_default();
            for unit_index in &visible {
                let amount = row
                    .as_slice()
                    .get(*unit_index as usize)
                    .copied()
                    .unwrap_or(0);
                if amount <= 0 {
                    continue;
                }
                let unit = units
                    .at(*unit_index as usize)
                    .try_to::<VarDictionary>()
                    .unwrap_or_default();
                let icon = kit::icon_texture(&format!("unit-{}-ui", dict_string(&unit, "name")));
                let mut x = column_x + 5.0;
                if let Some(icon) = &icon {
                    let icon_size = font.get_ascent_ex().font_size(big).done();
                    plot.draw_texture_rect(
                        icon,
                        Rect2::new(
                            Vector2::new(x, y + (line_h - icon_size) / 2.0),
                            Vector2::new(icon_size, icon_size),
                        ),
                        false,
                    );
                    x += font
                        .get_string_size_ex(" ")
                        .alignment(HorizontalAlignment::LEFT)
                        .width(-1.0)
                        .font_size(big)
                        .done()
                        .x;
                }
                let text = GString::from(amount.to_string().as_str());
                let ascent = font.get_ascent_ex().font_size(big).done();
                plot.draw_string_outline_ex(&font, Vector2::new(x, y + ascent), &text)
                    .alignment(HorizontalAlignment::LEFT)
                    .width(-1.0)
                    .font_size(big)
                    .size(3)
                    .modulate(Color::BLACK)
                    .done();
                plot.draw_string_ex(&font, Vector2::new(x, y + ascent), &text)
                    .alignment(HorizontalAlignment::LEFT)
                    .width(-1.0)
                    .font_size(big)
                    .modulate(Color::WHITE)
                    .done();
                y += line_h;
            }
        }

        // Horizontal marks: `totalMarks = clamp(maxY, 1, 10)`, faint light-gray
        // lines with the value at the left edge.
        let total_marks = max_y.clamp(1.0, 10.0);
        let mark_space = ((max_y / total_marks).ceil() as i64).max(1);
        let mut mark = 0i64;
        while (mark as f32) < max_y {
            let cy = base_y - mark as f32 * graph_h / max_y;
            let mut mark_color = styles::ARC_LIGHT_GRAY;
            mark_color.a = 0.1;
            plot.draw_line_ex(Vector2::new(0.0, cy), Vector2::new(width, cy), mark_color)
                .width(STROKE)
                .done();
            let text = GString::from(mark.to_string().as_str());
            plot.draw_string_ex(&font, Vector2::new(0.0, cy + fh / 2.0), &text)
                .alignment(HorizontalAlignment::LEFT)
                .width(-1.0)
                .font_size(font_size)
                .modulate(Color::WHITE)
                .done();
            mark += mark_space;
        }

        // Wave ticks along the bottom (`cy = y + fh`, `len = 4`), the hovered
        // column's wave number under its tick.
        let tick_y = height - fh;
        for i in 0..columns {
            let cx = graph_x + spacing * (i + self.from) as f32;
            if cx >= 0.0 && cx <= width {
                plot.draw_line_ex(
                    Vector2::new(cx, tick_y),
                    Vector2::new(cx, tick_y - 4.0),
                    styles::ARC_LIGHT_GRAY,
                )
                .width(STROKE)
                .done();
            }
            if i == selcol {
                let text = GString::from((i + self.from + 1).to_string().as_str());
                let text_w = font
                    .get_string_size_ex(&text)
                    .alignment(HorizontalAlignment::LEFT)
                    .width(-1.0)
                    .font_size(font_size)
                    .done()
                    .x;
                plot.draw_string_ex(
                    &font,
                    Vector2::new(cx - text_w / 2.0, tick_y + 2.0 + fh),
                    &text,
                )
                .alignment(HorizontalAlignment::LEFT)
                .width(-1.0)
                .font_size(font_size)
                .modulate(styles::ARC_LIGHT_GRAY)
                .done();
            }
        }
    }

    /// `used` minus `hidden`, as unit indices into the Rust `units` array.
    fn visible_units(&self, units: &VarArray) -> Vec<i64> {
        let mut out = Vec::new();
        for index in 0..units.len() {
            let unit = units
                .at(index)
                .try_to::<VarDictionary>()
                .unwrap_or_default();
            let name = dict_string(&unit, "name");
            if !self.hidden.iter().any(|hidden| hidden == &name) {
                out.push(index as i64);
            }
        }
        out
    }

    /// `rebuild()`: refetch the window and rebuild the legend.
    fn rebuild(&mut self) {
        self.data = self.wave_graph_data(self.from, self.to);
        self.rebuild_legend();
    }

    /// `colors`: the `@waves.units.hide`/`show` button (flatt, 130x32) and a
    /// horizontal pane of `fullTogglet` toggles (colour swatch + unit icon).
    fn rebuild_legend(&mut self) {
        let Some(mut colors) = self.colors.clone() else {
            return;
        };
        for child in colors.get_children().iter_shared() {
            let mut child = child.clone();
            colors.remove_child(&child);
            child.queue_free();
        }
        self.legend.clear();
        let units = dict_array(&self.data, "units");
        let unit_count = self
            .data
            .get("unit_count")
            .map(|value| variant_f64(&value))
            .unwrap_or(1.0)
            .max(1.0);
        // code-instantiated: the legend's hide/show-all button.
        let mut hide_button = factory::button("");
        hide_button.set_theme_type_variation("flatt");
        hide_button.set_custom_minimum_size(Vector2::new(130.0, 32.0));
        hide_button.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        hide_button.connect("pressed", &self.base().callable("on_toggle_all"));
        colors.add_child(&hide_button);
        self.hide_button = Some(hide_button);
        // code-instantiated: the horizontally scrolling legend strip.
        let mut scroll = MindScroll::new_alloc();
        scroll.bind_mut().set_scrolling_disabled(false, true);
        let mut scroll_control: Gd<Control> = scroll.clone().upcast();
        scroll_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        colors.add_child(&scroll_control);
        // code-instantiated: one toggle per participating unit type (row).
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 0);
        scroll_control.add_child(&row);
        for unit_value in units.iter_shared() {
            let unit = unit_value.try_to::<VarDictionary>().unwrap_or_default();
            let name = dict_string(&unit, "name");
            // code-instantiated: one unit legend toggle (`fullTogglet`).
            let mut button = Button::new_alloc();
            button.set_theme_type_variation("fullTogglet");
            button.set_toggle_mode(true);
            // code-instantiated: the toggle's swatch + icon row.
            let mut content = HBoxContainer::new_alloc();
            content.set_mouse_filter(MouseFilter::IGNORE);
            content.add_theme_constant_override("separation", 0);
            // `b.image().size(32f)` has no drawable, so Arc draws nothing there:
            // the colour only tints an empty image. Keep the 32 px slot, invisible.
            // code-instantiated: the invisible colour swatch of one legend toggle.
            let mut swatch = ColorRect::new_alloc();
            swatch.set_self_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.0));
            swatch.set_custom_minimum_size(Vector2::new(32.0, 32.0));
            swatch.set_mouse_filter(MouseFilter::IGNORE);
            swatch.set_meta("color", &unit_color(&unit, unit_count).to_variant());
            content.add_child(&swatch);
            // code-instantiated: the unit's `uiIcon` inside its legend toggle.
            let mut icon = TextureRect::new_alloc();
            icon.set_texture(kit::icon_texture(&format!("unit-{name}-ui")).as_ref());
            icon.set_custom_minimum_size(Vector2::new(32.0, 32.0));
            icon.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
            icon.set_mouse_filter(MouseFilter::IGNORE);
            content.add_child(&icon);
            // code-instantiated: `.padRight(20)` spacer after the icon.
            let mut pad = Control::new_alloc();
            pad.set_custom_minimum_size(Vector2::new(20.0, 0.0));
            pad.set_mouse_filter(MouseFilter::IGNORE);
            content.add_child(&pad);
            button.add_child(&content);
            button.set_custom_minimum_size(Vector2::new(84.0, 32.0));
            let mut args = VarArray::new();
            args.push(&GString::from(name.as_str()).to_variant());
            button.connect(
                "pressed",
                &self.base().callable("on_toggle_unit").bindv(&args),
            );
            row.add_child(&button);
            self.legend.push(LegendEntry {
                name,
                button,
                swatch,
                icon,
            });
        }
        self.sync_legend(&units);
    }

    /// `hidden.size == used.size` (false for an empty unit list).
    fn all_hidden(&self, units: &VarArray) -> bool {
        for index in 0..units.len() {
            let unit = units
                .at(index)
                .try_to::<VarDictionary>()
                .unwrap_or_default();
            let name = dict_string(&unit, "name");
            if !self.hidden.iter().any(|hidden| hidden == &name) {
                return false;
            }
        }
        !units.is_empty()
    }

    /// The legend `update` lambdas: checked = hidden; the swatch halves its
    /// colour and the icon greys out; the hide button reads show/hide.
    fn sync_legend(&mut self, units: &VarArray) {
        if let Some(mut hide) = self.hide_button.clone() {
            hide.set_text(&bundle(if self.all_hidden(units) {
                "waves.units.show"
            } else {
                "waves.units.hide"
            }));
        }
        for entry in &mut self.legend {
            let hidden = self.hidden.iter().any(|name| name == &entry.name);
            entry.button.set_pressed_no_signal(hidden);
            if entry.swatch.has_meta("color") {
                let color = entry
                    .swatch
                    .get_meta("color")
                    .try_to::<Color>()
                    .unwrap_or(Color::WHITE);
                entry.swatch.set_color(if hidden {
                    Color::from_rgba(color.r * 0.5, color.g * 0.5, color.b * 0.5, color.a * 0.5)
                } else {
                    color
                });
            }
            entry.icon.set_modulate(if hidden {
                styles::ARC_GRAY
            } else {
                Color::WHITE
            });
        }
    }
}

/// `color(type)`: `fromHsv(type.id / units.size * 360, 0.7, 1)`.
fn unit_color(unit: &VarDictionary, unit_count: f64) -> Color {
    Color::from_hsv(dict_f64(unit, "id") / unit_count, 0.7, 1.0)
}

/// `endLine()`: 3 px segments with round joints, a bigger dot on the last point.
fn end_line(plot: &mut Gd<Control>, points: &PackedVector2Array, color: Color) {
    let slice = points.as_slice();
    if slice.is_empty() {
        return;
    }
    for pair in slice.windows(2) {
        plot.draw_line_ex(pair[0], pair[1], color)
            .width(STROKE)
            .done();
        plot.draw_circle_ex(pair[0], STROKE / 2.0, color)
            .filled(true)
            .done();
    }
    if let Some(last) = slice.last() {
        plot.draw_circle_ex(*last, STROKE, color)
            .filled(true)
            .done();
    }
}
