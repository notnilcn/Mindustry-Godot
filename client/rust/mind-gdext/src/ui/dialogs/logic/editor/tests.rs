// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's unit tests (content pickers, select popups, field rows).
//! Upstream: logic/LStatement.java, logic/LogicDialog.java

use super::*;

#[test]
fn content_and_sound_fields_have_list_pickers() {
    // `LStatements.java` `showSelectTable` sites: `playsound` field 1 is the
    // sound browser, `ubind` field 0 the unit grid (GAPS #46: these are list
    // pickers, not raw text fields).
    let flag = [String::from("false")];
    assert_eq!(picker_kind("playsound", 1, &flag), "sound");
    assert_eq!(content_picker("ubind", 0), "unit");
    assert_eq!(content_picker("status", 1), "status");
    assert_eq!(content_picker("weatherset", 0), "weather");
    assert_eq!(content_picker("sensor", 2), "sensor");
    assert_eq!(content_picker("setprop", 0), "prop");
    // Discriminator-gated pickers stay off for the other variants.
    assert_eq!(content_picker("setmarker", 2), "layer");
    assert_eq!(
        picker_kind("setmarker", 2, &[String::from("drawLayer")]),
        "layer"
    );
    assert_eq!(picker_kind("setmarker", 2, &[String::from("flush")]), "");
    assert_eq!(content_picker("ulocate", 3), "ore");
    assert_eq!(picker_kind("ulocate", 3, &[String::from("building")]), "");
    // A field with no picker renders as a plain styled widget.
    assert_eq!(content_picker("noop", 0), "");
}

/// `LStatement.showSelect`'s `cols` argument per call site
/// (`LStatements.java`): the enum popups are 2, 3 or 4 columns, not a
/// single fixed 4.
#[test]
fn enum_popup_columns_follow_the_java_call_sites() {
    // `DrawStatement.rebuild`: `showSelect(..., 2, ...)` (`:140`).
    assert_eq!(select_columns("draw", 0), 2);
    assert_eq!(select_columns("draw", 1), 4);
    // `ControlStatement` type (`:431`) / radar targets + sort (`:487,501`).
    assert_eq!(select_columns("control", 0), 2);
    assert_eq!(select_columns("radar", 0), 2);
    assert_eq!(select_columns("radar", 3), 2);
    assert_eq!(select_columns("uradar", 3), 2);
    assert_eq!(select_columns("radar", 4), 4);
    // `ULocateStatement` (`:1142`) and `UnitControlStatement` (`:1073`).
    assert_eq!(select_columns("ulocate", 0), 2);
    assert_eq!(select_columns("ulocate", 1), 2);
    assert_eq!(select_columns("ulocate", 2), 4);
    assert_eq!(select_columns("ucontrol", 0), 2);
    assert_eq!(select_columns("ucontrol", 1), 4);
    // `JumpStatement`/`SelectStatement` condition (`:987`, 3 columns).
    assert_eq!(select_columns("jump", 1), 3);
    assert_eq!(select_columns("select", 1), 3);
    // `SetMarkerStatement` layer list (`:1281`, 3) and `makeMarker` (2).
    assert_eq!(select_columns("setmarker", 0), 3);
    assert_eq!(select_columns("setmarker", 2), 4);
    assert_eq!(select_columns("makemarker", 0), 2);
    // `setrule`/`message`/`fetch`/`cutscene` (`:1639,1703,1929,1750`).
    assert_eq!(select_columns("setrule", 0), 2);
    assert_eq!(select_columns("message", 0), 2);
    assert_eq!(select_columns("fetch", 0), 2);
    assert_eq!(select_columns("cutscene", 0), 3);
    // Default `LStatement.showSelect` (4).
    assert_eq!(select_columns("noop", 0), 4);
}

/// The per-call-site cell overrides (`showSelect(..., cols, sizer)`), else
/// the 60x38 default (`LStatement.showSelect`).
#[test]
fn enum_popup_cell_sizes_follow_the_java_sizers() {
    assert_eq!(select_cell_size("draw", 0), Vector2::new(120.0, 50.0));
    assert_eq!(select_cell_size("control", 0), Vector2::new(110.0, 50.0));
    assert_eq!(select_cell_size("radar", 0), Vector2::new(110.0, 50.0));
    assert_eq!(select_cell_size("radar", 3), Vector2::new(100.0, 50.0));
    assert_eq!(select_cell_size("ulocate", 1), Vector2::new(120.0, 50.0));
    assert_eq!(select_cell_size("setmarker", 0), Vector2::new(140.0, 50.0));
    assert_eq!(select_cell_size("noop", 0), Vector2::new(60.0, 38.0));
}

/// `EffectStatement` opens `ui.effects` (`LStatements.java:1806`), not a
/// content picker; the list pickers must not claim the effect field.
#[test]
fn effect_statement_uses_the_effects_dialog_not_a_content_picker() {
    assert_eq!(content_picker("effect", 0), "");
    assert_eq!(content_picker("effect", 1), "");
}
