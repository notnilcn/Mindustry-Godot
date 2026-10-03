// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/UiTreeBuilder.java.

//! Server-menu tree materialization semantics (plan 14 §3.6).
//!
//! The Godot-free half: condition evaluation (`portrait`/`landscape`/
//! `width|height`), id collection, and server-streamed image region collection
//! (plan-21 `TextureStreamEvent` equivalent). The Godot sink lives in
//! `mind-gdext`; `mind-headless ui menu-tree` uses [`dump_json`] for goldens.

use serde_json::{Value, json};

use crate::ui::builder::menu_result::{MenuResult, MenuValue};
use crate::ui::builder::ui_key::UiKey;
use crate::ui::builder::ui_node::{UiNode, UiValue};

/// Prefix for server-streamed image regions (`DataImagePacker.serverRegionPrefix`).
pub const SERVER_REGION_PREFIX: &str = "net-";

/// Maximum nodes in one server menu tree (R9 cap).
pub const MAX_NODES: usize = 4096;
/// Maximum nesting depth of a server menu tree (R9 cap).
pub const MAX_DEPTH: usize = 48;
/// Maximum length of a single string value in a server menu tree (R9 cap).
pub const MAX_STRING_LEN: usize = 4096;

/// Server-menu tree cap violation (R9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeCapsError {
    /// More than [`MAX_NODES`] nodes.
    TooManyNodes(usize),
    /// Deeper than [`MAX_DEPTH`].
    TooDeep(usize),
    /// A string value exceeds [`MAX_STRING_LEN`].
    StringTooLong(usize),
}

impl std::fmt::Display for TreeCapsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreeCapsError::TooManyNodes(count) => {
                write!(f, "menu tree has {count} nodes (max {MAX_NODES})")
            }
            TreeCapsError::TooDeep(depth) => {
                write!(f, "menu tree depth {depth} exceeds {MAX_DEPTH}")
            }
            TreeCapsError::StringTooLong(len) => {
                write!(f, "menu string {len} chars exceeds {MAX_STRING_LEN}")
            }
        }
    }
}

impl std::error::Error for TreeCapsError {}

/// Validates the R9 node/depth/string caps on a server-supplied tree.
pub fn validate_caps(root: &UiNode) -> Result<(), TreeCapsError> {
    let mut state = CapState {
        nodes: 0,
        max_depth: 0,
    };
    check_node(root, 1, &mut state)
}

struct CapState {
    nodes: usize,
    max_depth: usize,
}

fn check_node(node: &UiNode, depth: usize, state: &mut CapState) -> Result<(), TreeCapsError> {
    state.nodes += 1;
    state.max_depth = state.max_depth.max(depth);
    if state.nodes > MAX_NODES {
        return Err(TreeCapsError::TooManyNodes(state.nodes));
    }
    if depth > MAX_DEPTH {
        return Err(TreeCapsError::TooDeep(depth));
    }
    for entry in &node.entries {
        match &entry.value {
            UiValue::Str(value) => {
                if value.chars().count() > MAX_STRING_LEN {
                    return Err(TreeCapsError::StringTooLong(value.chars().count()));
                }
            }
            UiValue::Node(child) => check_node(child, depth + 1, state)?,
            _ => {}
        }
    }
    Ok(())
}

/// A value captured from an id-bearing element (`UiTreeBuilder.fireResult`).
#[derive(Debug, Clone, PartialEq)]
pub enum ElementValue {
    /// Slider value.
    F32(f32),
    /// Text-field text.
    Str(String),
    /// Check box / toggle button state.
    Bool(bool),
}

/// One materialized id-bearing element.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    /// Node type.
    pub kind: UiKey,
    /// Click/enter result id, if wired.
    pub result: Option<String>,
    /// Captured value at click time.
    pub value: ElementValue,
    /// Whether a button has a toggle style (participates in results).
    pub checkable: bool,
}

/// Result of the Godot-free materialization walk.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Materialized {
    /// Ids of elements that declare one, in tree order.
    pub ids: Vec<String>,
    /// Server-streamed image regions, in tree order.
    pub images: Vec<String>,
    /// Id → captured element, insertion order.
    pub elements: indexmap::IndexMap<String, Element>,
}

impl Materialized {
    /// `UiTreeBuilder.fireResult`: builds a `MenuResult` from the clicked id and
    /// the current values of every id-bearing element.
    pub fn fire_result(&self, result_id: &str, token: i64) -> MenuResult {
        let mut result = MenuResult::from_result(result_id);
        result.token = token;
        for (id, element) in &self.elements {
            match element.value {
                ElementValue::F32(value) => result.insert(id.clone(), MenuValue::F32(value)),
                ElementValue::Str(ref value) => {
                    result.insert(id.clone(), MenuValue::Str(value.clone()));
                }
                ElementValue::Bool(value) => result.insert(id.clone(), MenuValue::Bool(value)),
            }
        }
        result
    }
}

/// Style names whose button style exposes a `checked` drawable (`UiTreeBuilder`
/// distinguishes toggle buttons solely by their style).
pub fn is_checkable_style(name: &str) -> bool {
    matches!(
        name,
        "flatTogglet"
            | "logicTogglet"
            | "flatToggleMenut"
            | "togglet"
            | "clearTogglet"
            | "fullTogglet"
            | "squareTogglet"
            | "emptyTogglei"
            | "squareTogglei"
            | "grayTogglei"
            | "clearTogglei"
            | "clearNoneTogglei"
    )
}

/// Godot-free materialization: ids, streamed images and element values.
pub fn materialize(root: &UiNode, ctx: &BuildContext) -> Materialized {
    let mut out = Materialized::default();
    walk_materialized(root, ctx, &mut out);
    out
}

fn walk_materialized(node: &UiNode, ctx: &BuildContext, out: &mut Materialized) {
    for entry in &node.entries {
        let UiValue::Node(child) = &entry.value else {
            continue;
        };
        if let Some(cond) = child.str_value(UiKey::Condition)
            && !eval_condition(cond, ctx)
        {
            continue;
        }
        if let Some(region) = collect_region(child) {
            out.images.push(region);
        }
        if let Some(id) = child.str_value(UiKey::Id) {
            out.ids.push(id.to_owned());
            if let Some(element) = element_for(child) {
                out.elements.insert(id.to_owned(), element);
            }
        }
        walk_materialized(child, ctx, out);
    }
}

fn collect_region(child: &UiNode) -> Option<String> {
    match child.node_type {
        UiKey::Image => {
            let region = child
                .str_value(UiKey::Region)
                .or_else(|| child.str_value(UiKey::Icon))
                .unwrap_or("error");
            region
                .starts_with(SERVER_REGION_PREFIX)
                .then(|| region.to_owned())
        }
        UiKey::Button | UiKey::ImageButton => child
            .str_value(UiKey::Icon)
            .filter(|icon| icon.starts_with(SERVER_REGION_PREFIX))
            .map(str::to_owned),
        _ => None,
    }
}

fn element_for(child: &UiNode) -> Option<Element> {
    let style = child.str_value(UiKey::Style).unwrap_or("");
    let element = match child.node_type {
        UiKey::Slider => Element {
            kind: UiKey::Slider,
            result: None,
            value: ElementValue::F32(
                child
                    .num(UiKey::DefaultValue)
                    .unwrap_or_else(|| child.num_or(UiKey::Min, 0.0)),
            ),
            checkable: false,
        },
        UiKey::Field => Element {
            kind: UiKey::Field,
            result: child.str_value(UiKey::Enter).map(str::to_owned),
            value: ElementValue::Str(child.str_or(UiKey::Text, "").to_owned()),
            checkable: false,
        },
        UiKey::Check => Element {
            kind: UiKey::Check,
            result: None,
            value: ElementValue::Bool(child.bool_or(UiKey::Checked, false)),
            checkable: false,
        },
        UiKey::Button | UiKey::ButtonTable | UiKey::ImageButton => Element {
            kind: child.node_type,
            result: child.str_value(UiKey::Clicked).map(str::to_owned),
            value: ElementValue::Bool(child.bool_or(UiKey::Checked, false)),
            checkable: is_checkable_style(style),
        },
        _ => return None,
    };
    Some(element)
}

/// Viewport context used by [`eval_condition`].
#[derive(Debug, Clone, Copy)]
pub struct BuildContext {
    /// Portrait orientation.
    pub portrait: bool,
    /// Scene width in scaled pixels.
    pub width: f32,
    /// Scene height in scaled pixels.
    pub height: f32,
}

impl Default for BuildContext {
    fn default() -> Self {
        Self {
            portrait: false,
            width: 1920.0,
            height: 1080.0,
        }
    }
}

/// Result of materializing a tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildResult {
    /// Ids of elements that declare one, in tree order.
    pub ids: Vec<String>,
    /// Region names streamed from the server, in tree order.
    pub images: Vec<String>,
}

/// Evaluates a condition string (`UiTreeBuilder.evalCondition`).
///
/// `"portrait"`/`"landscape"` check orientation; `"width|height >=|>|<|<= N"`
/// compares a dimension. Malformed conditions return `true` so layout is never
/// silently blocked.
pub fn eval_condition(cond: &str, ctx: &BuildContext) -> bool {
    let cond = cond.trim();
    if cond == "portrait" {
        return ctx.portrait;
    }
    if cond == "landscape" {
        return !ctx.portrait;
    }
    let parts: Vec<&str> = cond.split_whitespace().collect();
    if parts.len() != 3 {
        return true;
    }
    let dim = match parts[0] {
        "width" => ctx.width,
        "height" => ctx.height,
        _ => return true,
    };
    let Ok(num) = parts[2].parse::<f32>() else {
        return true;
    };
    match parts[1] {
        ">=" => dim >= num,
        ">" => dim > num,
        "<=" => dim <= num,
        "<" => dim < num,
        _ => true,
    }
}

/// Materializes the tree into a [`BuildResult`].
pub fn build(root: &UiNode, ctx: &BuildContext) -> BuildResult {
    let mut result = BuildResult::default();
    walk(root, ctx, &mut result);
    result
}

fn walk(node: &UiNode, ctx: &BuildContext, result: &mut BuildResult) {
    for entry in &node.entries {
        let UiValue::Node(child) = &entry.value else {
            continue;
        };
        if let Some(cond) = child.str_value(UiKey::Condition)
            && !eval_condition(cond, ctx)
        {
            continue;
        }
        if let Some(id) = child.str_value(UiKey::Id) {
            result.ids.push(id.to_owned());
        }
        match child.node_type {
            UiKey::Image => {
                let region = child
                    .str_value(UiKey::Region)
                    .or_else(|| child.str_value(UiKey::Icon))
                    .unwrap_or("error");
                if region.starts_with(SERVER_REGION_PREFIX) {
                    result.images.push(region.to_owned());
                }
            }
            UiKey::Button | UiKey::ImageButton => {
                if let Some(icon) = child.str_value(UiKey::Icon)
                    && icon.starts_with(SERVER_REGION_PREFIX)
                {
                    result.images.push(icon.to_owned());
                }
            }
            _ => {}
        }
        walk(child, ctx, result);
    }
}

/// Serializes the materialized tree to a normalized JSON dump (headless golden).
pub fn dump_json(root: &UiNode, ctx: &BuildContext) -> Value {
    let mut result = BuildResult::default();
    let tree = dump_node(root, ctx, &mut result);
    json!({
        "tree": tree,
        "ids": result.ids,
        "images": result.images,
    })
}

fn dump_node(node: &UiNode, ctx: &BuildContext, result: &mut BuildResult) -> Value {
    let mut children: Vec<Value> = Vec::new();
    let mut props: Vec<Value> = Vec::new();
    let mut defaults: Vec<Value> = Vec::new();

    for entry in &node.entries {
        match &entry.value {
            UiValue::Node(child) => {
                if entry.key == UiKey::Defaults {
                    for inner in &child.entries {
                        defaults.push(entry_json(inner));
                    }
                    continue;
                }
                if let Some(cond) = child.str_value(UiKey::Condition)
                    && !eval_condition(cond, ctx)
                {
                    continue;
                }
                if let Some(id) = child.str_value(UiKey::Id) {
                    result.ids.push(id.to_owned());
                }
                match child.node_type {
                    UiKey::Image => {
                        let region = child
                            .str_value(UiKey::Region)
                            .or_else(|| child.str_value(UiKey::Icon))
                            .unwrap_or("error");
                        if region.starts_with(SERVER_REGION_PREFIX) {
                            result.images.push(region.to_owned());
                        }
                    }
                    UiKey::Button | UiKey::ImageButton => {
                        if let Some(icon) = child.str_value(UiKey::Icon)
                            && icon.starts_with(SERVER_REGION_PREFIX)
                        {
                            result.images.push(icon.to_owned());
                        }
                    }
                    _ => {}
                }
                children.push(dump_node(child, ctx, result));
            }
            UiValue::Bool(true) if entry.key == UiKey::Row => {
                props.push(json!({"row": true}));
            }
            _ => props.push(entry_json(entry)),
        }
    }

    json!({
        "type": node.node_type.name(),
        "props": props,
        "defaults": defaults,
        "children": children,
    })
}

fn entry_json(entry: &crate::ui::builder::ui_node::UiEntry) -> Value {
    let value = match &entry.value {
        UiValue::Str(value) => Value::String(value.clone()),
        UiValue::F32(value) => json!(value),
        UiValue::Bool(value) => json!(value),
        UiValue::Node(_) => Value::Null,
    };
    json!({"key": entry.key.name(), "value": value})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::builder::ui_node::UiNode;

    fn table_with_bg() -> UiNode {
        UiNode::new(UiKey::Table).str(UiKey::Background, "grayPanel")
    }

    #[test]
    fn tree_builder_conditions() {
        let ctx = BuildContext {
            portrait: true,
            width: 600.0,
            height: 1000.0,
        };
        assert!(eval_condition("portrait", &ctx));
        assert!(!eval_condition("landscape", &ctx));
        assert!(eval_condition("width >= 500", &ctx));
        assert!(!eval_condition("width > 600", &ctx));
        assert!(eval_condition("height <= 1000", &ctx));
        assert!(!eval_condition("height < 500", &ctx));
        // Malformed conditions never block layout.
        assert!(eval_condition("bogus", &ctx));
        assert!(eval_condition("width ~ 500", &ctx));

        let mut root = table_with_bg();
        root.push(
            UiKey::Button,
            UiValue::Node(Box::new(
                UiNode::new(UiKey::Button)
                    .str(UiKey::Text, "hidden")
                    .str(UiKey::Condition, "landscape"),
            )),
        );
        root.push(
            UiKey::Button,
            UiValue::Node(Box::new(
                UiNode::new(UiKey::Button)
                    .str(UiKey::Text, "shown")
                    .str(UiKey::Condition, "portrait"),
            )),
        );
        let dump = dump_json(&root, &ctx);
        let children = dump["tree"]["children"].as_array().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0]["props"][0]["value"], json!("shown"));
    }

    #[test]
    fn tree_builder_ids_and_images() {
        let ctx = BuildContext::default();
        let mut root = table_with_bg();
        root.push(
            UiKey::Slider,
            UiValue::Node(Box::new(
                UiNode::new(UiKey::Slider)
                    .str(UiKey::Id, "volume")
                    .f32(UiKey::Min, 0.0),
            )),
        );
        root.push(
            UiKey::Image,
            UiValue::Node(Box::new(
                UiNode::new(UiKey::Image).str(UiKey::Region, "net-icon"),
            )),
        );
        root.push(
            UiKey::Image,
            UiValue::Node(Box::new(
                UiNode::new(UiKey::Image).str(UiKey::Region, "vanilla-icon"),
            )),
        );
        let result = build(&root, &ctx);
        assert_eq!(result.ids, vec!["volume".to_owned()]);
        assert_eq!(result.images, vec!["net-icon".to_owned()]);
    }

    #[test]
    fn materialize_collects_element_values_and_fires_result() {
        let ctx = BuildContext::default();
        let root = crate::ui::builder::dsl::parse(
            "table {\n  row\n  slider: \"vol\" { id: \"volume\" min: 0 max: 1 defaultValue: 0.25 }\n  field: \"base\" { id: \"name\" }\n  check: \"on\" { id: \"enabled\" checked: true }\n  button: \"Buy\" { id: \"buy\" clicked: \"buy\" }\n}\n",
        )
        .unwrap();
        let materialized = materialize(&root, &ctx);
        assert_eq!(materialized.ids, vec!["volume", "name", "enabled", "buy"]);
        // A plain `defaultt` button is not checkable and contributes no value.
        assert!(!materialized.elements["buy"].checkable);

        let result = materialized.fire_result("buy", 7);
        assert_eq!(result.token, 7);
        assert!(result.is("buy"));
        assert_eq!(result.get_f32("volume"), 0.25);
        assert_eq!(result.get_str("name"), Some("base"));
        assert!(result.get_bool("enabled"));
    }

    #[test]
    fn caps_reject_oversized_trees() {
        let ok = UiNode::new(UiKey::Table).child(UiNode::new(UiKey::Space));
        assert!(validate_caps(&ok).is_ok());

        // Oversized string.
        let long = UiNode::new(UiKey::Label).str(UiKey::Text, "x".repeat(MAX_STRING_LEN + 1));
        assert_eq!(
            validate_caps(&long),
            Err(TreeCapsError::StringTooLong(MAX_STRING_LEN + 1))
        );

        // Too deep.
        let mut current = UiNode::new(UiKey::Table);
        for _ in 0..MAX_DEPTH {
            let next = UiNode::new(UiKey::Table);
            current = next.child(current);
        }
        let root = UiNode::new(UiKey::Table).child(current);
        assert!(matches!(
            validate_caps(&root),
            Err(TreeCapsError::TooDeep(_))
        ));
    }

    #[test]
    fn checkable_style_detection() {
        assert!(is_checkable_style("togglet"));
        assert!(is_checkable_style("flatTogglet"));
        assert!(!is_checkable_style("defaultt"));
        assert!(!is_checkable_style("grayt"));
    }
}
