// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Undo/redo stack (`editor/OperationStack.java`, plan 19 §3.3).
//!
//! Exact port of the negative-index semantics: `index <= 0`, `add` truncates any
//! redo tail, the 30-entry cap evicts the oldest operation, and `canRedo` is
//! false immediately after an `add` (index 0 is not `> -1` false). These quirks
//! are load-bearing for the op-log oracle, so they are preserved verbatim.

use crate::content::ContentRegistry;

use super::EditorGrid;
use super::draw_op::DrawOperation;

/// Maximum retained operations (`OperationStack.maxSize`).
pub const MAX_SIZE: usize = 30;

/// The editor undo stack (`OperationStack`).
#[derive(Debug, Default, Clone)]
pub struct OperationStack {
    stack: Vec<DrawOperation>,
    index: i32,
}

impl OperationStack {
    /// An empty stack.
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            index: 0,
        }
    }

    /// Removes every operation (`OperationStack.clear`).
    pub fn clear(&mut self) {
        self.stack.clear();
        self.index = 0;
    }

    /// Number of retained operations (debug/`ops()` parity).
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Whether the stack holds no operations.
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Retained operations in insertion order (plan 19 M3 dev/MCP op log).
    pub fn ops(&self) -> &[DrawOperation] {
        &self.stack
    }

    /// Pushes an operation, dropping any redo tail (`OperationStack.add`).
    pub fn add(&mut self, action: DrawOperation) {
        let len = self.stack.len() as i32;
        self.stack.truncate((len + self.index).max(0) as usize);
        self.index = 0;
        self.stack.push(action);
        if self.stack.len() > MAX_SIZE {
            self.stack.remove(0);
        }
    }

    /// Pushes an operation while recycling every dropped operation (redo tail +
    /// evicted oldest) into `pool`, so steady-state editing reuses allocations
    /// (plan 19 §7d). Observable stack semantics are identical to [`Self::add`].
    pub fn add_recycling(&mut self, action: DrawOperation, pool: &mut Vec<DrawOperation>) {
        let len = self.stack.len() as i32;
        let keep = (len + self.index).max(0) as usize;
        while self.stack.len() > keep {
            if let Some(dropped) = self.stack.pop() {
                pool.push(dropped);
            }
        }
        self.index = 0;
        self.stack.push(action);
        if self.stack.len() > MAX_SIZE {
            let evicted = self.stack.remove(0);
            pool.push(evicted);
        }
    }

    /// Whether an undo is available (`OperationStack.canUndo`).
    pub fn can_undo(&self) -> bool {
        !(self.stack.len() as i32 - 1 + self.index < 0)
    }

    /// Whether a redo is available (`OperationStack.canRedo`).
    pub fn can_redo(&self) -> bool {
        !(self.index > -1 || self.stack.len() as i32 + self.index < 0)
    }

    /// Undoes the current operation (`OperationStack.undo`).
    pub fn undo(&mut self, world: &mut dyn EditorGrid, content: &ContentRegistry) {
        if !self.can_undo() {
            return;
        }
        let i = self.stack.len() as i32 - 1 + self.index;
        self.stack[i as usize].undo(world, content);
        self.index -= 1;
    }

    /// Redoes the next operation (`OperationStack.redo`).
    pub fn redo(&mut self, world: &mut dyn EditorGrid, content: &ContentRegistry) {
        if !self.can_redo() {
            return;
        }
        self.index += 1;
        let i = self.stack.len() as i32 - 1 + self.index;
        self.stack[i as usize].redo(world, content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::editor::test_grid::TestGrid;

    fn op(block: BlockId) -> DrawOperation {
        let mut op = DrawOperation::new();
        op.add(crate::editor::tile_op::TileOp::get(
            0,
            0,
            crate::editor::tile_op::OP_BLOCK,
            block.raw() as i32,
        ));
        op
    }

    /// `editor::tests::operation_stack_trace` (plan 19 §3.3): `A,B,C → undo,C →
    /// add D → [A,B,D]`, full-undo redo availability and the 30-cap eviction.
    #[test]
    fn operation_stack_trace() {
        let content = crate::content::test_support::test_registry();
        let mut world = TestGrid::new(2, 2);
        let a = BlockId::new(80);
        let b = BlockId::new(81);
        let c = BlockId::new(82);
        let d = BlockId::new(83);

        let mut stack = OperationStack::new();
        for block in [a, b, c] {
            let mut operation = op(block);
            operation.redo(&mut world, &content);
            stack.add(operation);
        }
        assert_eq!(world.block_id(0, 0), c);
        assert_eq!(stack.len(), 3);

        stack.undo(&mut world, &content);
        assert_eq!(world.block_id(0, 0), b);
        // After an undo the redo pointer is valid.
        assert!(stack.can_redo());

        // Adding a new op truncates the redo tail: [A, B, D].
        let mut new_op = op(d);
        new_op.redo(&mut world, &content);
        stack.add(new_op);
        assert_eq!(stack.len(), 3);
        // Undo D returns to the state before it (B), then walk back to A and air.
        stack.undo(&mut world, &content);
        assert_eq!(world.block_id(0, 0), b);
        stack.undo(&mut world, &content);
        assert_eq!(world.block_id(0, 0), a);
        stack.undo(&mut world, &content);
        assert_eq!(world.block_id(0, 0), BlockId::AIR);
        assert!(!stack.can_undo());
        // A further undo is a no-op.
        stack.undo(&mut world, &content);
        assert_eq!(world.block_id(0, 0), BlockId::AIR);
    }

    #[test]
    fn operation_stack_thirty_cap_evicts_oldest() {
        let content = crate::content::test_support::test_registry();
        let mut world = TestGrid::new(2, 2);
        let mut stack = OperationStack::new();
        for i in 0..MAX_SIZE + 5 {
            let mut operation = op(BlockId::new(80 + i as u16));
            operation.redo(&mut world, &content);
            stack.add(operation);
        }
        assert_eq!(stack.len(), MAX_SIZE);
        // The five oldest operations were evicted; the oldest retained op (value
        // 85) restores the state it replaced (84) on undo.
        while stack.can_undo() {
            stack.undo(&mut world, &content);
        }
        assert_eq!(world.block_id(0, 0), BlockId::new(84));
    }
}
