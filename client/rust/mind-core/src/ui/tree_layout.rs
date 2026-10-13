// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Node-tree layout algorithms (`ui/layout/TreeLayout.java`,
//! `ui/layout/BranchTreeLayout.java`) plus the research-tree placement
//! (`ui/dialogs/ResearchDialog.java:332-389 treeLayout`).
//!
//! Java walks a `TreeNode` object graph; this port uses an arena
//! (`Vec<LayoutNode>` with index links) so no `Rc`/`RefCell` is needed. The
//! algorithm is a modified abego TreeLayout walker — every value, cache and
//! call order is transcribed exactly; do not "simplify" it. `alignment` is
//! always `awayFromRoot` for the research dialog (its only user).

/// `TreeLayout.TreeNode`: one node of a layout tree.
#[derive(Clone, Debug)]
pub struct LayoutNode {
    /// Node width (`TreeNode.width`).
    pub width: f32,
    /// Node height (`TreeNode.height`).
    pub height: f32,
    /// Final x position (Java space).
    pub x: f32,
    /// Final y position (Java space, y-up).
    pub y: f32,
    /// Child indices.
    pub children: Vec<usize>,
    /// Parent index.
    pub parent: Option<usize>,
    /// `TreeNode.mode` (internal).
    mode: f32,
    /// `TreeNode.prelim` (internal).
    prelim: f32,
    /// `TreeNode.change` (internal).
    change: f32,
    /// `TreeNode.shift` (internal).
    shift: f32,
    /// `TreeNode.cachedWidth` (internal; starts at `-1` like Java).
    cached_width: f32,
    /// `TreeNode.number` (internal; starts at `-1` like Java).
    number: i32,
    /// `TreeNode.thread` (internal).
    thread: Option<usize>,
    /// `TreeNode.ancestor` (internal).
    ancestor: Option<usize>,
}

impl Default for LayoutNode {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            x: 0.0,
            y: 0.0,
            children: Vec::new(),
            parent: None,
            mode: 0.0,
            prelim: 0.0,
            change: 0.0,
            shift: 0.0,
            cached_width: -1.0,
            number: -1,
            thread: None,
            ancestor: None,
        }
    }
}

impl LayoutNode {
    /// A node with fixed width/height (`ResearchDialog.LayoutNode` uses
    /// `nodeSize` for both).
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            ..Self::default()
        }
    }

    /// `TreeNode.isLeaf`: `children == null || children.length == 0`.
    fn is_leaf(nodes: &[LayoutNode], index: usize) -> bool {
        nodes[index].children.is_empty()
    }

    /// `TreeNode.calcWidth`: total subtree width with the `cachedWidth > 0`
    /// cache (a leaf's width is its own).
    fn calc_width(nodes: &mut [LayoutNode], index: usize) -> f32 {
        if nodes[index].cached_width > 0.0 {
            return nodes[index].cached_width;
        }
        let mut c_width = 0.0;
        for child_index in 0..nodes[index].children.len() {
            let child = nodes[index].children[child_index];
            c_width += Self::calc_width(nodes, child);
        }
        let result = nodes[index].width.max(c_width);
        nodes[index].cached_width = result;
        result
    }
}

/// `BranchTreeLayout.TreeLocation`; only the two variants the research dialog
/// uses (`top`, `bottom`) are ported — both change levels on the Y axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TreeLocation {
    /// Children below the root.
    #[default]
    Top,
    /// Children above the root (used for the right half).
    Bottom,
}

/// `BranchTreeLayout` (abego TreeLayout port, `alignment = awayFromRoot`).
#[derive(Clone, Debug)]
pub struct BranchTreeLayout {
    /// `BranchTreeLayout.rootLocation`.
    pub root_location: TreeLocation,
    /// `BranchTreeLayout.gapBetweenLevels`.
    pub gap_between_levels: f32,
    /// `BranchTreeLayout.gapBetweenNodes`.
    pub gap_between_nodes: f32,
    /// `BranchTreeLayout.sizeOfLevel` (per `layout()` call).
    size_of_level: Vec<f32>,
}

impl BranchTreeLayout {
    /// New layout with the research dialog's spacing.
    pub fn new(root_location: TreeLocation, gap: f32) -> Self {
        Self {
            root_location,
            gap_between_levels: gap,
            gap_between_nodes: gap,
            size_of_level: Vec::new(),
        }
    }

    /// `BranchTreeLayout.layout`.
    pub fn layout(&mut self, nodes: &mut [LayoutNode], root: usize) {
        self.first_walk(nodes, root, None);
        self.calc_size_of_levels(nodes, root, 0);
        let m = -nodes[root].prelim;
        self.second_walk(nodes, root, m, 0, 0.0);
    }

    /// `isLevelChangeInYAxis` (both ported locations are horizontal cuts).
    fn is_level_change_in_y_axis(&self) -> bool {
        matches!(self.root_location, TreeLocation::Top | TreeLocation::Bottom)
    }

    /// `getLevelChangeSign`.
    fn level_change_sign(&self) -> f32 {
        if matches!(self.root_location, TreeLocation::Bottom) {
            -1.0
        } else {
            1.0
        }
    }

    /// `getNodeThickness` = `getWidthOrHeightOfNode(node, !isLevelChangeInYAxis)`.
    fn node_thickness(&self, nodes: &mut [LayoutNode], index: usize) -> f32 {
        if self.is_level_change_in_y_axis() {
            nodes[index].height
        } else {
            LayoutNode::calc_width(nodes, index)
        }
    }

    /// `getNodeSize` = `getWidthOrHeightOfNode(node, isLevelChangeInYAxis)`
    /// (the subtree width).
    fn node_size(&self, nodes: &mut [LayoutNode], index: usize) -> f32 {
        if self.is_level_change_in_y_axis() {
            LayoutNode::calc_width(nodes, index)
        } else {
            nodes[index].height
        }
    }

    /// `getDistance`: half of both subtree sizes plus the node gap.
    fn distance(&self, nodes: &mut [LayoutNode], v: usize, w: usize) -> f32 {
        let size_of_nodes = self.node_size(nodes, v) + self.node_size(nodes, w);
        size_of_nodes / 2.0 + self.gap_between_nodes
    }

    /// `nextLeft`: first child, or the thread for a leaf.
    fn next_left(nodes: &[LayoutNode], v: usize) -> Option<usize> {
        if LayoutNode::is_leaf(nodes, v) {
            nodes[v].thread
        } else {
            nodes[v].children.first().copied()
        }
    }

    /// `nextRight`: last child, or the thread for a leaf.
    fn next_right(nodes: &[LayoutNode], v: usize) -> Option<usize> {
        if LayoutNode::is_leaf(nodes, v) {
            nodes[v].thread
        } else {
            nodes[v].children.last().copied()
        }
    }

    /// `getNumber`: lazily numbers the parent's children `1..k`.
    fn get_number(nodes: &mut [LayoutNode], node: usize, parent: usize) -> i32 {
        if nodes[node].number == -1 {
            for (number, child_index) in (1..).zip(0..nodes[parent].children.len()) {
                let child = nodes[parent].children[child_index];
                nodes[child].number = number;
            }
        }
        nodes[node].number
    }

    /// `getAncestor` + `ancestor(...)`: the ancestor when it stays inside the
    /// parent's subtree, else the default.
    fn ancestor_of(
        nodes: &[LayoutNode],
        v_i_minus: usize,
        parent_of_v: usize,
        default_ancestor: usize,
    ) -> usize {
        let ancestor = nodes[v_i_minus].ancestor.unwrap_or(v_i_minus);
        if nodes[ancestor].parent == Some(parent_of_v) {
            ancestor
        } else {
            default_ancestor
        }
    }

    /// `moveSubtree`.
    fn move_subtree(
        &self,
        nodes: &mut [LayoutNode],
        w_minus: usize,
        w_plus: usize,
        parent: usize,
        shift: f32,
    ) {
        let subtrees =
            Self::get_number(nodes, w_plus, parent) - Self::get_number(nodes, w_minus, parent);
        nodes[w_plus].change -= shift / subtrees as f32;
        nodes[w_plus].shift += shift;
        nodes[w_minus].change += shift / subtrees as f32;
        nodes[w_plus].prelim += shift;
        nodes[w_plus].mode += shift;
    }

    /// `apportion`: the abego conflict resolver.
    fn apportion(
        &mut self,
        nodes: &mut [LayoutNode],
        v: usize,
        mut default_ancestor: usize,
        left_sibling: Option<usize>,
        parent_of_v: usize,
    ) -> usize {
        let Some(left_sibling) = left_sibling else {
            return default_ancestor;
        };

        let mut v_o_plus = v;
        let mut v_i_plus = v;
        let mut v_i_minus = left_sibling;
        let mut v_o_minus = nodes[parent_of_v].children[0];

        let mut s_i_plus = nodes[v_i_plus].mode;
        let mut s_o_plus = nodes[v_o_plus].mode;
        let mut s_i_minus = nodes[v_i_minus].mode;
        let mut s_o_minus = nodes[v_o_minus].mode;

        let mut next_right_v_i_minus = Self::next_right(nodes, v_i_minus);
        let mut next_left_v_i_plus = Self::next_left(nodes, v_i_plus);

        while let (Some(next_right), Some(next_left)) = (next_right_v_i_minus, next_left_v_i_plus) {
            v_i_minus = next_right;
            v_i_plus = next_left;
            // Java assigns `nextLeft(vOMinus)`/`nextRight(vOPlus)`
            // unconditionally; on these trees a contour is never null while
            // the loop runs (leaves carry threads). Keep the previous node if
            // it would be, so a Rust-side panic cannot replace the Java NPE.
            if let Some(next) = Self::next_left(nodes, v_o_minus) {
                v_o_minus = next;
            }
            if let Some(next) = Self::next_right(nodes, v_o_plus) {
                v_o_plus = next;
            }
            nodes[v_o_plus].ancestor = Some(v);

            let shift = (nodes[v_i_minus].prelim + s_i_minus) - (nodes[v_i_plus].prelim + s_i_plus)
                + self.distance(nodes, v_i_minus, v_i_plus);
            if shift > 0.0 {
                let ancestor = Self::ancestor_of(nodes, v_i_minus, parent_of_v, default_ancestor);
                self.move_subtree(nodes, ancestor, v, parent_of_v, shift);
                s_i_plus += shift;
                s_o_plus += shift;
            }
            s_i_minus += nodes[v_i_minus].mode;
            s_i_plus += nodes[v_i_plus].mode;
            s_o_minus += nodes[v_o_minus].mode;
            s_o_plus += nodes[v_o_plus].mode;

            next_right_v_i_minus = Self::next_right(nodes, v_i_minus);
            next_left_v_i_plus = Self::next_left(nodes, v_i_plus);
        }

        if next_right_v_i_minus.is_some() && Self::next_right(nodes, v_o_plus).is_none() {
            nodes[v_o_plus].thread = next_right_v_i_minus;
            let mode = nodes[v_o_plus].mode;
            nodes[v_o_plus].mode = mode + s_i_minus - s_o_plus;
        }
        if next_left_v_i_plus.is_some() && Self::next_left(nodes, v_o_minus).is_none() {
            nodes[v_o_minus].thread = next_left_v_i_plus;
            let mode = nodes[v_o_minus].mode;
            nodes[v_o_minus].mode = mode + s_i_plus - s_o_minus;
            default_ancestor = v;
        }
        default_ancestor
    }

    /// `executeShifts`.
    fn execute_shifts(&self, nodes: &mut [LayoutNode], v: usize) {
        let mut shift = 0.0;
        let mut change = 0.0;
        let children = nodes[v].children.clone();
        for w in children.iter().rev() {
            let w = *w;
            change += nodes[w].change;
            nodes[w].prelim += shift;
            nodes[w].mode += shift;
            shift += nodes[w].shift + change;
        }
    }

    /// `firstWalk`.
    fn first_walk(&mut self, nodes: &mut [LayoutNode], v: usize, left_sibling: Option<usize>) {
        if LayoutNode::is_leaf(nodes, v) {
            if let Some(ls) = left_sibling {
                let distance = self.distance(nodes, v, ls);
                nodes[v].prelim = nodes[ls].prelim + distance;
            }
        } else {
            let children = nodes[v].children.clone();
            let mut default_ancestor = children[0];
            let mut previous_child = None;
            for w in children {
                self.first_walk(nodes, w, previous_child);
                default_ancestor = self.apportion(nodes, w, default_ancestor, previous_child, v);
                previous_child = Some(w);
            }
            self.execute_shifts(nodes, v);
            let first = nodes[v].children[0];
            let last = nodes[v].children.last().copied().unwrap_or(first);
            let midpoint = (nodes[first].prelim + nodes[last].prelim) / 2.0;
            if let Some(ls) = left_sibling {
                let distance = self.distance(nodes, v, ls);
                nodes[v].prelim = nodes[ls].prelim + distance;
                let prelim = nodes[v].prelim;
                nodes[v].mode = prelim - midpoint;
            } else {
                nodes[v].prelim = midpoint;
            }
        }
    }

    /// `calcSizeOfLevels`.
    fn calc_size_of_levels(&mut self, nodes: &mut [LayoutNode], node: usize, level: usize) {
        let old_size = if self.size_of_level.len() <= level {
            self.size_of_level.push(0.0);
            0.0
        } else {
            self.size_of_level[level]
        };
        let size = self.node_thickness(nodes, node);
        if old_size < size {
            self.size_of_level[level] = size;
        }
        if !LayoutNode::is_leaf(nodes, node) {
            let children = nodes[node].children.clone();
            for child in children {
                self.calc_size_of_levels(nodes, child, level + 1);
            }
        }
    }

    /// `getSizeOfLevel`.
    fn get_size_of_level(&self, level: usize) -> f32 {
        self.size_of_level.get(level).copied().unwrap_or(0.0)
    }

    /// `secondWalk` (`alignment = awayFromRoot`).
    fn second_walk(
        &mut self,
        nodes: &mut [LayoutNode],
        v: usize,
        m: f32,
        level: usize,
        level_start: f32,
    ) {
        let level_change_sign = self.level_change_sign();
        let level_change_on_y_axis = self.is_level_change_in_y_axis();
        let level_size = self.get_size_of_level(level);

        let mut x = nodes[v].prelim + m;
        let mut y =
            level_start + level_size - level_change_sign * (self.node_thickness(nodes, v) / 2.0);
        if !level_change_on_y_axis {
            std::mem::swap(&mut x, &mut y);
        }
        nodes[v].x = x;
        nodes[v].y = y;

        if !LayoutNode::is_leaf(nodes, v) {
            let next_level_start =
                level_start + (level_size + self.gap_between_levels) * level_change_sign;
            let mode = nodes[v].mode;
            let children = nodes[v].children.clone();
            for child in children {
                self.second_walk(nodes, child, m + mode, level + 1, next_level_start);
            }
        }
    }
}

/// `ResearchDialog.treeLayout` (ResearchDialog.java:332-372).
///
/// `children[i]` is the child index list of node `i` (depth-first order, as
/// `TechStore::each` visits); returns the `(x, y)` position per input node in
/// Java y-up space. Nodes are `node_size` wide and tall (`nodeSize`, 60 in the
/// dialog). `spacing` (20) is fixed by the Java method.
///
/// The root `LayoutNode` object is shared between the two `BranchTreeLayout`
/// calls (`prelim`/`mode`/`cachedWidth` carry over), exactly like Java; each
/// call gets a fresh layout (fresh `sizeOfLevel`).
pub fn research_tree_layout(
    children: &[Vec<usize>],
    root: usize,
    node_size: f32,
) -> Vec<(f32, f32)> {
    const SPACING: f32 = 20.0;

    let mut nodes: Vec<LayoutNode> = children
        .iter()
        .map(|kids| LayoutNode {
            children: kids.clone(),
            ..LayoutNode::new(node_size, node_size)
        })
        .collect();
    for (parent, kids) in children.iter().enumerate() {
        for &kid in kids {
            if let Some(record) = nodes.get_mut(kid) {
                record.parent = Some(parent);
            }
        }
    }

    let all_children = nodes[root].children.clone();
    let split = (all_children.len() as f32 / 2.0).ceil() as usize;
    let left_half = all_children[..split].to_vec();
    let right_half = all_children[split..].to_vec();

    nodes[root].children = left_half.clone();
    let mut top = BranchTreeLayout::new(TreeLocation::Top, SPACING);
    top.layout(&mut nodes, root);
    let last_y = nodes[root].y;

    if !right_half.is_empty() {
        nodes[root].children = right_half;
        let mut bottom = BranchTreeLayout::new(TreeLocation::Bottom, SPACING);
        bottom.layout(&mut nodes, root);
        let amount = nodes[root].y - last_y;
        shift_y(&mut nodes, &left_half, amount);
    }

    nodes[root].children = all_children;
    let mut positions = vec![(0.0, 0.0); children.len()];
    copy_info(&nodes, root, &mut positions);
    positions
}

/// `ResearchDialog.shift`: adds `amount` to the y of every node in the given
/// subtrees.
fn shift_y(nodes: &mut [LayoutNode], children: &[usize], amount: f32) {
    for &node in children {
        nodes[node].y += amount;
        let kids = nodes[node].children.clone();
        if !kids.is_empty() {
            shift_y(nodes, &kids, amount);
        }
    }
}

/// `ResearchDialog.copyInfo`: copies layout positions onto the results.
fn copy_info(nodes: &[LayoutNode], index: usize, positions: &mut [(f32, f32)]) {
    positions[index] = (nodes[index].x, nodes[index].y);
    for &child in &nodes[index].children {
        copy_info(nodes, child, positions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_positions(found: &[(f32, f32)], expected: &[(f32, f32)]) {
        assert_eq!(found.len(), expected.len());
        for (index, (found, expected)) in found.iter().zip(expected).enumerate() {
            assert!(
                (found.0 - expected.0).abs() < 1e-4 && (found.1 - expected.1).abs() < 1e-4,
                "node {index}: got {found:?}, want {expected:?} (all: {found:?})"
            );
        }
    }

    #[test]
    fn three_leaf_children_split_layout() {
        // Root with three leaves: [1,2] lay out on top, [3] below, the left
        // half shifts down so both halves meet at the root.
        let children = vec![vec![1, 2, 3], vec![], vec![], vec![]];
        let positions = research_tree_layout(&children, 0, 60.0);
        assert_positions(
            &positions,
            &[(0.0, 90.0), (-40.0, 170.0), (40.0, 170.0), (0.0, 10.0)],
        );
    }

    #[test]
    fn subtree_width_spacing() {
        // Node 1 has two children ([3,4]); the distance uses the subtree width
        // (120), not the node width.
        let children = vec![vec![1, 2], vec![3, 4], vec![], vec![], vec![]];
        let positions = research_tree_layout(&children, 0, 60.0);
        assert_positions(
            &positions,
            &[
                (0.0, 90.0),
                (0.0, 170.0),
                (0.0, 10.0),
                (-40.0, 250.0),
                (40.0, 250.0),
            ],
        );
    }
}
