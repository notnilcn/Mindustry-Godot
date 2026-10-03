// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/UiTreeBuilder.java (`UiTreeSink`),
//         core/src/mindustry/ui/builder/UiBuilder.java.

//! Godot-free widget materialization (plan 14 §3.6).
//!
//! `UiTreeBuilder` walks a server `UiNode` tree and calls a sink per element.
//! The Godot sink (`mind-gdext::ui::dsl_factory`) instantiates `Control`s; the
//! sink described here is the Godot-free *spec* — widget kind, resolved style,
//! id, condition and typed properties — so the same walk is testable headless
//! and `mind-headless ui builder` can lock a golden. [`dump_json`] is the
//! headless serializer.

use serde_json::{Value, json};

use crate::ui::builder::style_lookup::{StyleKind, StyleLookup};
use crate::ui::builder::tree_builder::{BuildContext, eval_condition};
use crate::ui::builder::ui_key::UiKey;
use crate::ui::builder::ui_node::{UiNode, UiValue};
use crate::ui::manifest::StylesManifest;

/// A style name resolved to its scene2d kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStyle {
    /// Style name (e.g. `grayt`).
    pub name: String,
    /// Owning manifest kind.
    pub kind: StyleKind,
}

/// A property value after coercion.
#[derive(Debug, Clone, PartialEq)]
pub enum FactoryProp {
    /// String property.
    Str(String),
    /// Float property.
    F32(f32),
    /// Boolean property.
    Bool(bool),
}

impl FactoryProp {
    fn to_json(&self) -> Value {
        match self {
            FactoryProp::Str(value) => Value::String(value.clone()),
            FactoryProp::F32(value) => json!(value),
            FactoryProp::Bool(value) => json!(value),
        }
    }
}

/// One materialized widget.
#[derive(Debug, Clone, PartialEq)]
pub struct FactoryNode {
    /// Widget kind (`UiKey` node type; `Row` marks a table row break).
    pub kind: UiKey,
    /// Resolved `style:` reference, when present and resolvable.
    pub style: Option<ResolvedStyle>,
    /// Element id (`id:`).
    pub id: Option<String>,
    /// Visibility condition (`condition:`), already evaluated for children.
    pub condition: Option<String>,
    /// Non-node properties in declaration order (`row` included as `Bool(true)`).
    pub props: Vec<(String, FactoryProp)>,
    /// Materialized child widgets (condition-failing children are dropped).
    pub children: Vec<FactoryNode>,
}

impl FactoryNode {
    /// An empty widget of `kind`.
    pub fn empty(kind: UiKey) -> Self {
        Self {
            kind,
            style: None,
            id: None,
            condition: None,
            props: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Serializes this node (and subtree) to the normalized golden shape.
    pub fn to_json(&self) -> Value {
        let props: Vec<Value> = self
            .props
            .iter()
            .map(|(key, value)| json!({"key": key, "value": value.to_json()}))
            .collect();
        json!({
            "type": self.kind.name(),
            "style": self.style.as_ref().map(|style| json!({
                "name": style.name,
                "kind": style.kind.name(),
            })),
            "id": self.id,
            "condition": self.condition,
            "props": props,
            "children": self.children.iter().map(FactoryNode::to_json).collect::<Vec<_>>(),
        })
    }
}

/// Everything the Godot sink needs for one tree, plus the collected ids/images
/// (plan 14 §3.6: `MenuResult` capture and overlay image tracking).
#[derive(Debug, Clone, PartialEq)]
pub struct FactoryTree {
    /// Root widget.
    pub root: FactoryNode,
    /// Id-bearing elements, tree order.
    pub ids: Vec<String>,
    /// Server-streamed image regions (`net-` prefix), tree order.
    pub images: Vec<String>,
    /// Every resolvable style reference used by the tree, tree order.
    pub styles: Vec<ResolvedStyle>,
    /// Style names the manifest could not resolve (server bug / drift).
    pub unresolved: Vec<String>,
}

impl FactoryTree {
    /// Serializes the whole materialization to the golden shape.
    pub fn to_json(&self) -> Value {
        json!({
            "format": 1,
            "root": self.root.to_json(),
            "ids": self.ids,
            "images": self.images,
            "styles": self.styles.iter().map(|style| json!({
                "name": style.name,
                "kind": style.kind.name(),
            })).collect::<Vec<_>>(),
            "unresolved_styles": self.unresolved,
        })
    }
}

/// Materializes a server `UiNode` into a [`FactoryTree`], resolving styles
/// against `manifest` and evaluating `condition:` against `ctx`.
pub fn materialize(root: &UiNode, manifest: &StylesManifest, ctx: &BuildContext) -> FactoryTree {
    let mut tree = FactoryTree {
        root: FactoryNode::empty(root.node_type),
        ids: Vec::new(),
        images: Vec::new(),
        styles: Vec::new(),
        unresolved: Vec::new(),
    };
    let mut root_node = FactoryNode::empty(root.node_type);
    fill_node(root, manifest, ctx, &mut root_node, &mut tree);
    tree.root = root_node;
    tree
}

fn fill_node(
    source: &UiNode,
    manifest: &StylesManifest,
    ctx: &BuildContext,
    target: &mut FactoryNode,
    tree: &mut FactoryTree,
) {
    target.id = source.str_value(UiKey::Id).map(str::to_owned);
    target.condition = source.str_value(UiKey::Condition).map(str::to_owned);
    if let Some(style) = resolve_style(manifest, source.str_value(UiKey::Style)) {
        tree.styles.push(style.clone());
        target.style = Some(style);
    } else if let Some(name) = source.str_value(UiKey::Style) {
        tree.unresolved.push(name.to_owned());
    }
    // `background` is a drawable style reference too.
    if let Some(name) = source.str_value(UiKey::Background)
        && !name.is_empty()
        && StyleLookup::new(manifest).resolve(name) != Some(StyleKind::Drawable)
    {
        tree.unresolved.push(name.to_owned());
    }
    if let Some(id) = &target.id {
        tree.ids.push(id.clone());
    }
    if let Some(region) = collect_region(source) {
        tree.images.push(region);
    }

    for entry in &source.entries {
        match &entry.value {
            UiValue::Node(child) => {
                if entry.key == UiKey::Defaults {
                    // Defaults apply to the enclosing container; keep them as a
                    // childless marker so the sink can read cell defaults.
                    let mut defaults = FactoryNode::empty(UiKey::Defaults);
                    for inner in &child.entries {
                        defaults.props.push(prop_entry(inner));
                    }
                    target.children.push(defaults);
                    continue;
                }
                if let Some(cond) = child.str_value(UiKey::Condition)
                    && !eval_condition(cond, ctx)
                {
                    continue;
                }
                let mut node = FactoryNode::empty(child.node_type);
                fill_node(child, manifest, ctx, &mut node, tree);
                target.children.push(node);
            }
            UiValue::Bool(true) if entry.key == UiKey::Row => {
                target
                    .props
                    .push((entry.key.name().to_owned(), FactoryProp::Bool(true)));
            }
            _ => target.props.push(prop_entry(entry)),
        }
    }
}

fn prop_entry(entry: &crate::ui::builder::ui_node::UiEntry) -> (String, FactoryProp) {
    let value = match &entry.value {
        UiValue::Str(value) => FactoryProp::Str(value.clone()),
        UiValue::F32(value) => FactoryProp::F32(*value),
        UiValue::Bool(value) => FactoryProp::Bool(*value),
        UiValue::Node(_) => FactoryProp::Bool(false),
    };
    (entry.key.name().to_owned(), value)
}

fn resolve_style(manifest: &StylesManifest, name: Option<&str>) -> Option<ResolvedStyle> {
    let name = name?;
    if name.is_empty() {
        return None;
    }
    let kind = StyleLookup::new(manifest).resolve(name)?;
    Some(ResolvedStyle {
        name: name.to_owned(),
        kind,
    })
}

fn collect_region(node: &UiNode) -> Option<String> {
    match node.node_type {
        UiKey::Image => {
            let region = node
                .str_value(UiKey::Region)
                .or_else(|| node.str_value(UiKey::Icon))
                .unwrap_or("error");
            region.starts_with("net-").then(|| region.to_owned())
        }
        UiKey::Button | UiKey::ImageButton => node
            .str_value(UiKey::Icon)
            .filter(|icon| icon.starts_with("net-"))
            .map(str::to_owned),
        _ => None,
    }
}

/// Serializes a materialization to the golden shape (headless sink).
pub fn dump_json(root: &UiNode, manifest: &StylesManifest, ctx: &BuildContext) -> Value {
    materialize(root, manifest, ctx).to_json()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::builder::dsl;

    fn manifest() -> StylesManifest {
        StylesManifest::from_json(
            r#"{"format":1,"text_buttons":["defaultt","grayt"],"labels":["defaultLabel"],"drawables":["black","grayPanel"]}"#,
        )
        .unwrap()
    }

    #[test]
    fn materialize_resolves_styles_and_conditions() {
        let tree = dsl::parse(
            "background: grayPanel\nrow\nlabel: \"Hi\" { id: \"title\" style: \"defaultLabel\" }\nbutton: \"Buy\" { style: \"grayt\" clicked: \"buy\" }\nlabel: \"portrait\" { condition: \"portrait\" }\n",
        )
        .unwrap();
        let ctx = BuildContext {
            portrait: true,
            width: 600.0,
            height: 1000.0,
        };
        let factory = materialize(&tree, &manifest(), &ctx);
        assert_eq!(factory.ids, vec!["title".to_owned()]);
        assert_eq!(factory.styles.len(), 2);
        assert_eq!(factory.styles[0].kind, StyleKind::Label);
        assert_eq!(factory.styles[1].kind, StyleKind::TextButton);
        assert!(factory.unresolved.is_empty());
        // Three children, plus the row marker lives in props.
        let root = &factory.root;
        assert!(
            root.props
                .iter()
                .any(|(key, value)| key == "row" && value == &FactoryProp::Bool(true))
        );
        assert_eq!(root.children.len(), 3);
        assert_eq!(root.children[0].kind, UiKey::Label);
        assert_eq!(
            root.children[0].style.as_ref().unwrap().name,
            "defaultLabel"
        );
        assert_eq!(root.children[1].style.as_ref().unwrap().name, "grayt");
    }

    #[test]
    fn materialize_reports_unresolved_styles() {
        let tree = dsl::parse("button: \"x\" { style: \"bogus\" }\n").unwrap();
        let factory = materialize(&tree, &manifest(), &BuildContext::default());
        assert!(factory.root.children[0].style.is_none());
        assert_eq!(factory.unresolved, vec!["bogus".to_owned()]);
    }

    #[test]
    fn materialize_collects_server_images() {
        let tree = dsl::parse("image: net-icon\nimage: vanilla\n").unwrap();
        let factory = materialize(&tree, &manifest(), &BuildContext::default());
        assert_eq!(factory.images, vec!["net-icon".to_owned()]);
    }

    #[test]
    fn dump_json_is_stable() {
        let tree = dsl::parse("row\nlabel: \"Hi\" { style: \"defaultLabel\" }\n").unwrap();
        let value = dump_json(&tree, &manifest(), &BuildContext::default());
        assert_eq!(value["format"], json!(1));
        assert_eq!(value["root"]["type"], json!("table"));
        assert_eq!(value["root"]["children"][0]["type"], json!("label"));
        assert_eq!(value["styles"][0]["name"], json!("defaultLabel"));
    }
}
