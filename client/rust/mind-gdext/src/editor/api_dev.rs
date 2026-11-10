// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Dev/MCP oracle hooks over the editor op log and the grid.
//! Upstream: core/src/mindustry/editor/MapEditorDialog.java

use super::*;

#[godot_api(secondary)]
impl MindEditor {
    // --- dev / MCP oracle (plan 19 §3.1, §7b) ---

    /// Draws a line with the current draw block (`dev_draw_line`).
    #[func]
    pub fn dev_draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
        self.mark_tiles_changed();
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let mut world = WorldEditorGrid::new(
            &mut self.grid,
            content,
            &mut self.ecs.0,
            &self.hooks,
            &self.render,
        );
        mind_core::editor::tool::touched_line(
            &mut self.editor,
            EditorTool::Line,
            &mut world,
            content,
            x1,
            y1,
            x2,
            y2,
        );
        true
    }

    /// Draws a filled rectangle with single-tile brushes (`dev_draw_rect`).
    #[func]
    pub fn dev_draw_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let previous = self.editor.brush_size;
        self.editor.brush_size = 0.5;
        {
            let mut world = WorldEditorGrid::new(
                &mut self.grid,
                content,
                &mut self.ecs.0,
                &self.hooks,
                &self.render,
            );
            for y in y0.min(y1)..=y0.max(y1) {
                for x in x0.min(x1)..=x0.max(x1) {
                    self.editor.draw_blocks(&mut world, content, x, y);
                }
            }
            self.editor.flush_op();
        }
        self.editor.brush_size = previous;
        true
    }

    /// Draws a circle of the current draw block with radius `radius`.
    #[func]
    pub fn dev_draw_circle(&mut self, x: i32, y: i32, radius: f64) -> bool {
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let previous = self.editor.brush_size;
        self.editor.brush_size = (radius as f32).clamp(0.5, 20.0);
        {
            let mut world = WorldEditorGrid::new(
                &mut self.grid,
                content,
                &mut self.ecs.0,
                &self.hooks,
                &self.render,
            );
            self.editor.draw_blocks(&mut world, content, x, y);
            self.editor.flush_op();
        }
        self.editor.brush_size = previous;
        true
    }

    /// The retained undo operations as packed op lists (`dev_op_log`).
    #[func]
    pub fn dev_op_log(&self) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        for operations in self.editor.op_log() {
            let mut packed = PackedInt64Array::new();
            for op in operations {
                packed.push(op as i64);
            }
            out.push(&packed.to_variant());
        }
        out
    }

    /// Applies an op log (same shape as `dev_op_log`) onto the world.
    #[func]
    pub fn dev_apply_op_log(&mut self, log: Array<Variant>) -> bool {
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let mut operations: Vec<Vec<u64>> = Vec::with_capacity(log.len());
        for index in 0..log.len() {
            let Some(entry) = log.get(index) else {
                return false;
            };
            let Ok(packed) = entry.try_to::<PackedInt64Array>() else {
                return false;
            };
            let mut ops = Vec::with_capacity(packed.len());
            for value in packed.as_slice() {
                ops.push(*value as u64);
            }
            operations.push(ops);
        }
        for ops in operations {
            let mut operation = DrawOperation::from_ops(ops);
            {
                let mut world = WorldEditorGrid::new(
                    &mut self.grid,
                    content,
                    &mut self.ecs.0,
                    &self.hooks,
                    &self.render,
                );
                operation.redo(&mut world, content);
            }
            self.editor.push_op(operation);
        }
        true
    }

    /// Undoes every retained operation (`dev_undo_all`).
    #[func]
    pub fn dev_undo_all(&mut self) -> i64 {
        let mut count = 0i64;
        while let Some(content) = self.content.as_ref() {
            if !self.editor.can_undo() {
                break;
            }
            let mut world = WorldEditorGrid::new(
                &mut self.grid,
                content,
                &mut self.ecs.0,
                &self.hooks,
                &self.render,
            );
            self.editor.undo(&mut world, content);
            count += 1;
        }
        count
    }

    /// Redoes every available operation (`dev_redo_all`).
    #[func]
    pub fn dev_redo_all(&mut self) -> i64 {
        let mut count = 0i64;
        while let Some(content) = self.content.as_ref() {
            if !self.editor.can_redo() {
                break;
            }
            let mut world = WorldEditorGrid::new(
                &mut self.grid,
                content,
                &mut self.ecs.0,
                &self.hooks,
                &self.render,
            );
            self.editor.redo(&mut world, content);
            count += 1;
        }
        count
    }

    /// FNV-1a checksum of the editor world (`dev_state_digest`).
    #[func]
    pub fn dev_state_digest(&self) -> i64 {
        let mut checksummer = Checksummer::new();
        checksummer.part(&self.grid);
        checksummer.finish().value() as i64
    }

    /// Saves under `name` (sets the tag first), returning the `save()` result.
    #[func]
    pub fn dev_save_as(&mut self, name: GString) -> VarDictionary {
        self.editor.tags.insert("name".to_owned(), name.to_string());
        self.save()
    }

    /// Opens `user://maps/<name>.msav` in the editor (`dev_open`).
    #[func]
    pub fn dev_open(&mut self, name: GString) -> bool {
        let path = format!("user://maps/{}.msav", name);
        self.begin_edit_map(GString::from(path.as_str()))
    }

    /// Editor world invariants for the MCP oracle (`check_invariants`).
    #[func]
    pub fn check_invariants(&self) -> VarDictionary {
        let content_ok = self.content.is_some();
        let draw_block_editable = self
            .content
            .as_ref()
            .and_then(|content| content.block(self.editor.draw_block))
            .is_some_and(|def| def.in_editor);
        let mut out = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        out.set(&key("content_ok"), &content_ok.to_variant());
        out.set(&key("width"), &(self.grid.width() as i64).to_variant());
        out.set(&key("height"), &(self.grid.height() as i64).to_variant());
        out.set(
            &key("retained_ops"),
            &(self.editor.retained_ops() as i64).to_variant(),
        );
        out.set(
            &key("current_ops"),
            &(self.editor.ops() as i64).to_variant(),
        );
        out.set(
            &key("recording_allowed"),
            &self
                .editor
                .should_record_ops(false, self.grid.generating)
                .to_variant(),
        );
        out.set(
            &key("draw_block_editable"),
            &draw_block_editable.to_variant(),
        );
        out.set(
            &key("ok"),
            &(content_ok && draw_block_editable).to_variant(),
        );
        out
    }
}
