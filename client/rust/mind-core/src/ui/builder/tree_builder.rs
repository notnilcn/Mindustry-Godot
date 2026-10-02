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

use crate::ui::builder::ui_key::UiKey;
use crate::ui::builder::ui_node::{UiNode, UiValue};

/// Prefix for server-streamed image regions (`DataImagePacker.serverRegionPrefix`).
pub const SERVER_REGION_PREFIX: &str = "net-";

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
}
