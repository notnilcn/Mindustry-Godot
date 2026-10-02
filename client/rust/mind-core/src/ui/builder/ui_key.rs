// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/UiKey.java.

//! Frozen `UiKey` ordinals (plan 14 §6.2).
//!
//! Declaration order is the Java ABI: node types first, then `row`, then
//! node-specific properties, then cell properties. Ordinals are encoded in the
//! `ui_node` wire format and locked by `tests/goldens/ui/ui_keys.txt`.

/// Enum of properties encoded as single bytes in the `ui_node` wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum UiKey {
    // node types
    /// Table container.
    Table,
    /// Scroll pane.
    Pane,
    /// Stack.
    Stack,
    /// Label.
    Label,
    /// Image.
    Image,
    /// Text button.
    Button,
    /// Image button.
    ImageButton,
    /// Text field.
    Field,
    /// Check box.
    Check,
    /// Slider.
    Slider,
    /// Spacer.
    Space,
    /// Defaults block.
    Defaults,
    /// Button-table container.
    ButtonTable,
    /// Row marker (first non-node key).
    Row,

    // node-specific properties
    /// Label/field/button text.
    Text,
    /// Wrap flag.
    Wrap,
    /// Image region.
    Region,
    /// Button icon.
    Icon,
    /// Placeholder drawable.
    Placeholder,
    /// Image scaling.
    Scaling,
    /// Table background.
    Background,
    /// Table margin.
    Margin,
    /// Element id.
    Id,
    /// Field hint.
    Hint,
    /// Field max length.
    MaxLength,
    /// Checked flag.
    Checked,
    /// Slider min.
    Min,
    /// Slider max.
    Max,
    /// Slider step.
    Step,
    /// Slider default value.
    DefaultValue,
    /// Click result.
    Clicked,
    /// Field enter result.
    Enter,
    /// Style name.
    Style,
    /// Button group.
    Group,
    /// Visibility condition.
    Condition,
    /// Element color.
    Color,
    /// Disabled flag.
    Disabled,

    // cell properties
    /// Grow cell.
    Grow,
    /// Grow cell on x.
    GrowX,
    /// Grow cell on y.
    GrowY,
    /// Fill cell.
    Fill,
    /// Fill cell on x.
    FillX,
    /// Fill cell on y.
    FillY,
    /// Expand cell.
    Expand,
    /// Expand cell on x.
    ExpandX,
    /// Expand cell on y.
    ExpandY,
    /// Cell width.
    Width,
    /// Cell height.
    Height,
    /// Cell size.
    Size,
    /// Cell min width.
    MinWidth,
    /// Cell max width.
    MaxWidth,
    /// Cell min height.
    MinHeight,
    /// Cell max height.
    MaxHeight,
    /// Cell padding.
    Pad,
    /// Cell top padding.
    PadTop,
    /// Cell left padding.
    PadLeft,
    /// Cell bottom padding.
    PadBottom,
    /// Cell right padding.
    PadRight,
    /// Cell alignment.
    Align,
    /// Label alignment.
    LabelAlign,
    /// Cell colspan.
    Colspan,
    /// Cell uniform.
    Uniform,
    /// Cell uniform on x.
    UniformX,
    /// Cell uniform on y.
    UniformY,
}

impl UiKey {
    /// All variants in declaration (ordinal) order.
    pub const ALL: [UiKey; 64] = [
        UiKey::Table,
        UiKey::Pane,
        UiKey::Stack,
        UiKey::Label,
        UiKey::Image,
        UiKey::Button,
        UiKey::ImageButton,
        UiKey::Field,
        UiKey::Check,
        UiKey::Slider,
        UiKey::Space,
        UiKey::Defaults,
        UiKey::ButtonTable,
        UiKey::Row,
        UiKey::Text,
        UiKey::Wrap,
        UiKey::Region,
        UiKey::Icon,
        UiKey::Placeholder,
        UiKey::Scaling,
        UiKey::Background,
        UiKey::Margin,
        UiKey::Id,
        UiKey::Hint,
        UiKey::MaxLength,
        UiKey::Checked,
        UiKey::Min,
        UiKey::Max,
        UiKey::Step,
        UiKey::DefaultValue,
        UiKey::Clicked,
        UiKey::Enter,
        UiKey::Style,
        UiKey::Group,
        UiKey::Condition,
        UiKey::Color,
        UiKey::Disabled,
        UiKey::Grow,
        UiKey::GrowX,
        UiKey::GrowY,
        UiKey::Fill,
        UiKey::FillX,
        UiKey::FillY,
        UiKey::Expand,
        UiKey::ExpandX,
        UiKey::ExpandY,
        UiKey::Width,
        UiKey::Height,
        UiKey::Size,
        UiKey::MinWidth,
        UiKey::MaxWidth,
        UiKey::MinHeight,
        UiKey::MaxHeight,
        UiKey::Pad,
        UiKey::PadTop,
        UiKey::PadLeft,
        UiKey::PadBottom,
        UiKey::PadRight,
        UiKey::Align,
        UiKey::LabelAlign,
        UiKey::Colspan,
        UiKey::Uniform,
        UiKey::UniformX,
        UiKey::UniformY,
    ];

    /// The ordinal byte written to the wire / golden.
    pub fn ordinal(self) -> u8 {
        self as u8
    }

    /// The Java identifier (DSL / golden name).
    pub fn name(self) -> &'static str {
        match self {
            UiKey::Table => "table",
            UiKey::Pane => "pane",
            UiKey::Stack => "stack",
            UiKey::Label => "label",
            UiKey::Image => "image",
            UiKey::Button => "button",
            UiKey::ImageButton => "imageButton",
            UiKey::Field => "field",
            UiKey::Check => "check",
            UiKey::Slider => "slider",
            UiKey::Space => "space",
            UiKey::Defaults => "defaults",
            UiKey::ButtonTable => "buttonTable",
            UiKey::Row => "row",
            UiKey::Text => "text",
            UiKey::Wrap => "wrap",
            UiKey::Region => "region",
            UiKey::Icon => "icon",
            UiKey::Placeholder => "placeholder",
            UiKey::Scaling => "scaling",
            UiKey::Background => "background",
            UiKey::Margin => "margin",
            UiKey::Id => "id",
            UiKey::Hint => "hint",
            UiKey::MaxLength => "maxLength",
            UiKey::Checked => "checked",
            UiKey::Min => "min",
            UiKey::Max => "max",
            UiKey::Step => "step",
            UiKey::DefaultValue => "defaultValue",
            UiKey::Clicked => "clicked",
            UiKey::Enter => "enter",
            UiKey::Style => "style",
            UiKey::Group => "group",
            UiKey::Condition => "condition",
            UiKey::Color => "color",
            UiKey::Disabled => "disabled",
            UiKey::Grow => "grow",
            UiKey::GrowX => "growX",
            UiKey::GrowY => "growY",
            UiKey::Fill => "fill",
            UiKey::FillX => "fillX",
            UiKey::FillY => "fillY",
            UiKey::Expand => "expand",
            UiKey::ExpandX => "expandX",
            UiKey::ExpandY => "expandY",
            UiKey::Width => "width",
            UiKey::Height => "height",
            UiKey::Size => "size",
            UiKey::MinWidth => "minWidth",
            UiKey::MaxWidth => "maxWidth",
            UiKey::MinHeight => "minHeight",
            UiKey::MaxHeight => "maxHeight",
            UiKey::Pad => "pad",
            UiKey::PadTop => "padTop",
            UiKey::PadLeft => "padLeft",
            UiKey::PadBottom => "padBottom",
            UiKey::PadRight => "padRight",
            UiKey::Align => "align",
            UiKey::LabelAlign => "labelAlign",
            UiKey::Colspan => "colspan",
            UiKey::Uniform => "uniform",
            UiKey::UniformX => "uniformX",
            UiKey::UniformY => "uniformY",
        }
    }

    /// Parses a Java identifier back into a key.
    pub fn from_name(name: &str) -> Option<UiKey> {
        UiKey::ALL.iter().copied().find(|key| key.name() == name)
    }

    /// `true` for the container/element node types (all before `row`).
    pub fn is_node_type(self) -> bool {
        self.ordinal() < UiKey::Row.ordinal()
    }

    /// Parses a node type from its ordinal byte.
    pub fn from_ordinal(ordinal: u8) -> Option<UiKey> {
        UiKey::ALL
            .iter()
            .copied()
            .find(|key| key.ordinal() == ordinal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_key_ordinals_frozen() {
        for (index, key) in UiKey::ALL.iter().enumerate() {
            assert_eq!(key.ordinal() as usize, index, "ordinal drift at {index}");
            assert_eq!(UiKey::from_name(key.name()), Some(*key));
        }
        assert_eq!(UiKey::Table.ordinal(), 0);
        assert_eq!(UiKey::Row.ordinal(), 13);
        assert_eq!(UiKey::Text.ordinal(), 14);
        assert_eq!(UiKey::Grow.ordinal(), 37);
        assert!(UiKey::ButtonTable.is_node_type());
        assert!(!UiKey::Row.is_node_type());
        assert!(!UiKey::Grow.is_node_type());
        assert_eq!(UiKey::ALL.len(), 64);
    }

    #[test]
    fn ui_key_ordinals_match_committed_golden() {
        let golden = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/goldens/ui/ui_keys.txt"
        ));
        let mut index = 0usize;
        for line in golden.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (ordinal, name) = line.split_once('\t').expect("ordinal\tname");
            let ordinal: u8 = ordinal.parse().expect("ordinal");
            assert_eq!(ordinal as usize, index, "golden ordinal drift");
            assert_eq!(name, UiKey::ALL[index].name(), "golden name drift");
            assert_eq!(UiKey::from_name(name), Some(UiKey::ALL[index]));
            index += 1;
        }
        assert_eq!(index, UiKey::ALL.len(), "golden entry count");
    }
}
