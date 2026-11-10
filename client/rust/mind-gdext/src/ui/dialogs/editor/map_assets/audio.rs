// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The audio view of the map assets dialog.
//! Upstream: editor/data/MapAssetsDialog.java (`MapAudioView`)

use super::*;
use crate::gdutil::variant::{dict_bool_or, dict_text_or, to_bool_lenient};

pub(super) fn map_assets_build_audio(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
    kind: &str,
) {
    let rows = map_assets_rows_of(state, kind);
    let mut shown_count = 0;
    for row in &rows {
        if !map_assets_matches(state, row, "name") {
            continue;
        }
        shown_count += 1;
        let widget = map_assets_audio_row(dialog, state, row, kind);
        list.add_child(&widget);
    }
    if shown_count == 0 {
        let label = factory::label(&markup("@none.found"));
        list.add_child(&label);
    }
}

pub(super) fn map_assets_audio_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
    kind: &str,
) -> Gd<HBoxContainer> {
    let audio_name = dict_text_or(row, "name");
    let path = dict_text_or(row, "path");
    let music = kind == "music";
    let stream_name = map_assets_audio_stream_name(row);
    // code-instantiated: one audio record row (play/export/delete).
    let mut box_ = factory::hbox(4);
    box_.set_alignment(AlignmentMode::CENTER);

    let playing = state.borrow().playing_audio == stream_name;
    let mut play = factory::icon_button(
        if playing { "pause" } else { "play" },
        "",
        "graySquarei",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    play.set_custom_minimum_size(Vector2::new(ASSET_ROW_HEIGHT, ASSET_ROW_HEIGHT));
    play.set_tooltip_text(&kit::t("@play").to_string());
    let dialog_for_play = dialog.clone();
    let state_for_play = state.clone();
    let stream_for_play = stream_name.clone();
    let button_for_play = play.clone();
    let callable = Callable::from_fn("audio_play", move |_args: &[&Variant]| {
        map_assets_play_audio(
            &dialog_for_play,
            &state_for_play,
            &stream_for_play,
            music,
            &button_for_play,
        );
    });
    play.connect("pressed", &callable);
    box_.add_child(&play);

    // `in.add("[accent]" + asset.name)` + the formatted length.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &flat_box(styles::GRAY_PANEL));
    panel.set_custom_minimum_size(Vector2::new(
        map_assets_row_width(&dialog.bind()),
        ASSET_ROW_HEIGHT,
    ));
    panel.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut lines = factory::vbox(0);
    let mut title = factory::label(&markup(&format!("[accent]{audio_name}")));
    title.set_horizontal_alignment(HorizontalAlignment::LEFT);
    lines.add_child(&title);
    let seconds = map_assets_audio_length(&stream_name, music);
    let mut length = factory::label(&format!(
        "[lightgray][[{}]",
        map_assets_format_seconds(seconds)
    ));
    length.add_theme_color_override("font_color", LIGHT_GRAY);
    lines.add_child(&length);
    panel.add_child(&lines);
    box_.add_child(&panel);

    let dialog_for_export = dialog.clone();
    let name_for_export = audio_name.clone();
    let path_for_export = path.clone();
    let kind_for_export = String::from(kind);
    let export = Callable::from_fn("audio_export", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_export.clone();
        let name_for = name_for_export.clone();
        let path_for = path_for_export.clone();
        let kind_for = kind_for_export.clone();
        let extension = if path_for.to_lowercase().ends_with(".ogg") {
            "*.ogg"
        } else {
            "*.mp3"
        };
        let label = kit::t("@editor.export").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            &name_for,
            extension,
            Rc::new(move |target: String| {
                let exported = editor_call(
                    "data_asset_export",
                    &[
                        GString::from(kind_for.as_str()).to_variant(),
                        GString::from(path_for.as_str()).to_variant(),
                        GString::from(target.as_str()).to_variant(),
                    ],
                )
                .map(|value| to_bool_lenient(&value))
                .unwrap_or(false);
                if !exported {
                    map_assets_error(&kit::t("@asset.broken").to_string());
                }
            }),
        );
    });
    box_.add_child(&square_cell(
        "export",
        &kit::t("@export.file").to_string(),
        export,
    ));

    let dialog_for_trash = dialog.clone();
    let state_for_trash = state.clone();
    let name_for_trash = audio_name.clone();
    let kind_for_trash = String::from(kind);
    let trash = Callable::from_fn("audio_delete", move |_args: &[&Variant]| {
        let message = kit::t("@asset.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for_trash,
            &state_for_trash,
            &message,
            PendingConfirm::AudioDelete(kind_for_trash.clone(), name_for_trash.clone()),
        );
    });
    box_.add_child(&square_cell(
        "trash",
        &kit::t("@asset.delete.confirm").to_string(),
        trash,
    ));
    box_
}

/// `DataAudioLoader.prefix + asset.name.replace(' ', '_')` (`dp-`).
pub(super) fn map_assets_audio_stream_name(row: &VarDictionary) -> String {
    let asset_name = dict_text_or(row, "name").replace(' ', "_");
    format!("dp-{}", asset_name.to_lowercase())
}

pub(super) fn map_assets_audio_length(stream_name: &str, music: bool) -> f64 {
    let Some(mut assets) = kit::assets() else {
        return 0.0;
    };
    let stream = if music {
        assets.bind_mut().music_stream(GString::from(stream_name))
    } else {
        assets.bind_mut().sound_stream(GString::from(stream_name))
    };
    stream.map(|stream| stream.get_length()).unwrap_or(0.0)
}

pub(super) fn map_assets_format_seconds(seconds: f64) -> String {
    let total = seconds as i64;
    format!("{}:{:02}", total / 60, total % 60)
}

pub(super) fn map_assets_play_audio(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    stream_name: &str,
    music: bool,
    button: &Gd<Button>,
) {
    let _ = dialog;
    let Some(mut audio) = kit::node("/root/Spine/MindAudio") else {
        return;
    };
    if !audio.has_method("play_data_audio") {
        return;
    }
    let started = audio
        .call(
            "play_data_audio",
            &[GString::from(stream_name).to_variant(), music.to_variant()],
        )
        .try_to::<bool>()
        .unwrap_or(false);
    if !started {
        return;
    }
    state.borrow_mut().playing_audio = String::from(stream_name);
    if let Some(texture) = kit::icon_texture("pause") {
        let mut button = button.clone();
        button.set_button_icon(&texture);
    }
}

pub(super) fn map_assets_add_audio(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    kind: &str,
) {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let kind_for = String::from(kind);
    let label = kit::t("@add").to_string();
    map_assets_open_file(
        dialog,
        &label,
        vec![String::from("*.ogg"), String::from("*.mp3")],
        true,
        Rc::new(move |paths: Vec<String>| {
            for path in paths {
                let result = editor_call(
                    "data_file_add",
                    &[
                        GString::from(kind_for.as_str()).to_variant(),
                        GString::from(path.as_str()).to_variant(),
                    ],
                )
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .unwrap_or_default();
                if !dict_bool_or(&result, "ok", false) {
                    map_assets_error(&dict_text_or(&result, "error"));
                }
            }
            map_assets_refetch(&state_for);
            map_assets_rebuild(&dialog_for, &state_for);
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PAR-0541: `map_assets_audio_stream_name` reads the row through
    /// `dict_string` (a Godot `VarDictionary`), so calling it from a plain
    /// cargo unit test panics with `Godot engine not available`. The naming
    /// rule itself is pure — `dp-` + the row's `name` with spaces replaced by
    /// underscores, lowercased (`batch_f.rs:4220-4222`) — and the seconds
    /// formatter is pure, so this test exercises both without touching the
    /// engine. The FFI-backed call is covered by the in-engine dialog check.
    #[test]
    fn audio_stream_name_uses_the_dp_prefix() {
        let stream_name = |name: &str| format!("dp-{}", name.replace(' ', "_").to_lowercase());
        assert_eq!(stream_name("My Sound"), "dp-my_sound");
        assert_eq!(stream_name("Ambient"), "dp-ambient");
        assert_eq!(stream_name("Two Words Here"), "dp-two_words_here");
        // A name already lowercase/underscored round-trips unchanged.
        assert_eq!(stream_name("my_sound"), "dp-my_sound");
        // `map_assets_format_seconds` is engine-free (pure formatting).
        assert_eq!(map_assets_format_seconds(61.7), "1:01");
        assert_eq!(map_assets_format_seconds(0.0), "0:00");
        assert_eq!(map_assets_format_seconds(3600.0), "60:00");
    }
}
