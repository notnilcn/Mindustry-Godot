// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! I03 T12 display and T14 canvas conformance.
//! Upstream: core/src/mindustry/logic/LExecutor.java.

use super::*;

// ---- I03 T12: displays (tileable linking, root routing, CPU raster) ----

/// `TileableLogicDisplay.linkDisplays` (`TileableLogicDisplay.java:41-116`):
/// a 2x2 group merges into one bottom-left root, 2x2 tiles, rectangular.
#[test]
fn tileable_2x2_links_root_bottom_left() {
    use crate::logic::blocks::display::link_displays;
    let mut sim = campaign_sim();
    let a = place(&mut sim, "tile-logic-display", 20, 20);
    let b = place(&mut sim, "tile-logic-display", 21, 20);
    let c = place(&mut sim, "tile-logic-display", 20, 21);
    let d = place(&mut sim, "tile-logic-display", 21, 21);
    tick(&mut sim, 1);
    link_displays(&mut sim.ecs.0, a);
    for e in [a, b, c, d] {
        let state = sim
            .ecs
            .0
            .get::<LogicDisplayState>(e)
            .expect("display state");
        assert_eq!(state.root_display, Some(a), "bottom-left root");
        assert_eq!(state.tiles_width, 2);
        assert_eq!(state.tiles_height, 2);
        assert!(state.rectangular, "2x2 is rectangular");
        assert_eq!((state.origin_x, state.origin_y), (20, 20));
        assert!(!state.needs_update, "linkDisplays clears the flag");
    }
    // The 2x2 root's buffer is `32 * 2 - 6 * 2` per axis.
    let state = sim.ecs.0.get::<LogicDisplayState>(a).expect("root");
    assert_eq!(state.display_size, 32, "tileable per-tile display size");
    assert_events_drained(&sim);
}

/// `TileableLogicDisplayBuild.draw`: a member forwards `drawflush` to the
/// root's queue (`TileableLogicDisplay.java:255-261`).
#[test]
fn flush_draw_forwards_to_root() {
    use crate::logic::blocks::display::link_displays;
    let mut sim = campaign_sim();
    let root = place(&mut sim, "tile-logic-display", 20, 20);
    let member = place(&mut sim, "tile-logic-display", 21, 20);
    tick(&mut sim, 1);
    link_displays(&mut sim.ecs.0, root);
    place_linked_world_processor(
        &mut sim,
        10,
        10,
        "draw clear 255 0 0\ndrawflush display1\nstop\n",
        &[LogicLink::new("display1", 11, 10)],
    );
    tick(&mut sim, 2);
    let (root_commands, root_operations) = sim
        .ecs
        .0
        .get::<LogicDisplayState>(root)
        .map(|state| (state.commands.len(), state.operations))
        .expect("root state");
    assert_eq!(root_commands, 1, "the member forwarded the clear command");
    assert_eq!(root_operations, 1, "one flush");
    let member_commands = sim
        .ecs
        .0
        .get::<LogicDisplayState>(member)
        .map(|state| state.commands.len())
        .unwrap_or(usize::MAX);
    assert_eq!(member_commands, 0, "the member's own queue stays empty");
    assert_events_drained(&sim);
}

/// `LExecutor.DrawI` image packing: a display source packs
/// `(rootDisplay.index << 5) | displayDrawType` (`LExecutor.java:1005-1007`).
#[test]
fn image_packs_root_index() {
    use crate::logic::blocks::display::link_displays;
    use crate::logic::executor::draw::{DISPLAY_DRAW_TYPE, cmd_p1, cmd_p4, unpack_sign};
    let mut sim = campaign_sim();
    let root = place(&mut sim, "tile-logic-display", 20, 20);
    let member = place(&mut sim, "tile-logic-display", 21, 20);
    let target = place(&mut sim, "logic-display", 30, 30);
    tick(&mut sim, 1);
    link_displays(&mut sim.ecs.0, root);
    let root_index = sim
        .ecs
        .0
        .get::<LogicDisplayState>(root)
        .map(|state| state.index)
        .expect("root index");
    place_linked_world_processor(
        &mut sim,
        10,
        10,
        "draw image 0 0 display1 10 10 0\ndrawflush display2\n",
        &[
            LogicLink::new("display1", 11, 10),
            LogicLink::new("display2", 20, 20),
        ],
    );
    tick(&mut sim, 2);
    let command = sim
        .ecs
        .0
        .get::<LogicDisplayState>(target)
        .and_then(|state| state.commands.front().copied())
        .expect("one image command queued");
    let p1 = unpack_sign(cmd_p1(command));
    let p4 = unpack_sign(cmd_p4(command));
    let packed = (p4 << 10) | p1;
    assert_eq!(packed & 0x1f, DISPLAY_DRAW_TYPE, "display draw type");
    assert_eq!(packed >> 5, root_index, "the ROOT's arena index is packed");
    // The member itself is not the packed index (it is `root_index + 1`).
    let member_index = sim
        .ecs
        .0
        .get::<LogicDisplayState>(member)
        .map(|state| state.index)
        .expect("member index");
    assert_ne!(packed >> 5, member_index);
    assert_events_drained(&sim);
}

/// `LogicDisplayBuild.draw`: `LogicDisplay.maxDisplayBuffer` caps the queue at
/// 1024; `operations` still increments (`LogicDisplay.java:131-140`).
#[test]
fn queue_cap_1024() {
    use crate::logic::executor::draw::{COMMAND_CLEAR, pack_cmd, pack_sign};
    let mut sim = campaign_sim();
    let display = place(&mut sim, "logic-display", 20, 20);
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<LogicDisplayState>(display)
            .expect("display state");
        for _ in 0..1024 {
            state.commands.push_back(0);
        }
    }
    let mut exec = crate::logic::executor::Executor::new();
    exec.team = 1;
    exec.graphics_buffer.push(pack_cmd(
        COMMAND_CLEAR,
        pack_sign(255),
        pack_sign(0),
        pack_sign(0),
        0,
        0,
        0,
    ));
    crate::logic::blocks::io::flush_draw(
        &mut sim.ecs.0,
        &exec,
        Some(&LogicObject::Building(display)),
    );
    let state = sim
        .ecs
        .0
        .get::<LogicDisplayState>(display)
        .expect("display state");
    assert_eq!(state.commands.len(), 1024, "queue stays at the cap");
    assert_eq!(state.operations, 1, "operations increments past the cap");
    assert_events_drained(&sim);
}

/// T12/T13 seam: `process_display` drains the root queue into the CPU raster.
#[test]
fn display_process_drains_into_raster() {
    use crate::logic::display_raster::{LogicDisplayRasters, NoImages, process_display};
    let mut sim = campaign_sim();
    let display = place(&mut sim, "logic-display", 20, 20);
    place_linked_world_processor(
        &mut sim,
        10,
        10,
        "draw clear 255 0 0\ndraw rect 10 10 20 20\ndrawflush display1\n",
        &[LogicLink::new("display1", 10, 10)],
    );
    tick(&mut sim, 2);
    assert!(process_display(&mut sim.ecs.0, display, &NoImages));
    let rasters = sim
        .ecs
        .0
        .get_resource::<LogicDisplayRasters>()
        .expect("rasters");
    let raster = rasters.rasters.get(&display).expect("raster");
    assert_eq!(raster.width, 80, "logic-display is 80x80");
    assert_eq!(raster.pixel(5, 5), Some(0xff00_00ff), "cleared red");
    assert_eq!(raster.pixel(20, 20), Some(0xffff_ffff), "rect is white");
    assert!(
        sim.ecs
            .0
            .get::<LogicDisplayState>(display)
            .expect("state")
            .commands
            .is_empty()
    );
    assert_events_drained(&sim);
}

/// `LogicDisplay.sense(displayWidth/displayHeight/bufferSize/operations)`
/// (`LogicDisplay.java:117-124`, `TileableLogicDisplay.java:143-148`).
#[test]
fn display_sense_dimensions() {
    use crate::logic::blocks::display::link_displays;
    let mut sim = campaign_sim();
    let plain = place(&mut sim, "logic-display", 20, 20);
    let root = place(&mut sim, "tile-logic-display", 24, 20);
    let member = place(&mut sim, "tile-logic-display", 25, 20);
    tick(&mut sim, 1);
    link_displays(&mut sim.ecs.0, root);
    place_linked_world_processor(
        &mut sim,
        10,
        10,
        "sensor w display1 @displayWidth\nsensor h display1 @displayHeight\nsensor tw display2 @displayWidth\nsensor th display2 @displayHeight\nsensor b display2 @bufferSize\n",
        &[
            LogicLink::new("display1", 10, 10),
            LogicLink::new("display2", 14, 10),
        ],
    );
    tick(&mut sim, 3);
    let num = |name: &str| {
        sim.ecs
            .0
            .get::<LogicBlockState>(sim.grid.entity_at(TilePos::new(10, 10)).expect("processor"))
            .and_then(|state| {
                state
                    .executor
                    .arena
                    .get_id(name)
                    .map(|id| state.executor.arena.get(id).num())
            })
    };
    assert_eq!(num("w"), Some(80.0), "logic-display displayWidth");
    assert_eq!(num("h"), Some(80.0));
    assert_eq!(num("tw"), Some(52.0), "2x1 tileable: 2*32 - 12");
    assert_eq!(num("th"), Some(20.0), "1*32 - 12");
    assert_eq!(
        num("b"),
        Some(0.0),
        "member bufferSize reads the ROOT queue"
    );
    let _ = (plain, member, root);
    assert_events_drained(&sim);
}

// ---- I03 T14: canvas blocks ----

/// `CanvasBuild.getPixel`/`setPixel` use the LINEAR pixel position with
/// LSB-first bit packing (`CanvasBlock.java:143-195`).
#[test]
fn canvas_linear_pixels_roundtrip() {
    use crate::logic::blocks::canvas::CanvasBlockState;
    let mut state = CanvasBlockState::new(12, 3, 8);
    // `ceil(12 * 12 * 3 / 8)` = 54 bytes.
    assert_eq!(state.data.len(), 54);
    state.set_pixel(0, 1);
    assert_eq!(state.get_pixel(0), 1.0);
    state.set_pixel(143, 7);
    assert_eq!(state.get_pixel(143), 7.0);
    // Out of range reads are NaN.
    assert!(state.get_pixel(144).is_nan());
    assert!(state.get_pixel(-1).is_nan());
    // Neighbouring pixels are untouched.
    assert_eq!(state.get_pixel(1), 0.0);
}

/// `CanvasBuild.setPixel` ignores out-of-range positions and palette indices
/// (`CanvasBlock.java:143-149`).
#[test]
fn canvas_write_out_of_range_ignored() {
    use crate::logic::blocks::canvas::CanvasBlockState;
    let mut state = CanvasBlockState::new(8, 3, 8);
    state.set_pixel(-1, 1);
    state.set_pixel(64, 1);
    state.set_pixel(0, 8);
    state.set_pixel(0, -1);
    assert_eq!(state.get_pixel(0), 0.0);
    state.set_pixel(63, 7);
    assert_eq!(state.get_pixel(63), 7.0);
}

/// `CanvasBuild.read`/`write`: a program writes a palette index and reads it
/// back (`CanvasBlock.java:283-295`).
#[test]
fn canvas_logic_read_write() {
    use crate::logic::blocks::canvas::CanvasBlockState;
    let mut sim = campaign_sim();
    let canvas = place(&mut sim, "canvas", 20, 20);
    place_linked_world_processor(
        &mut sim,
        10,
        10,
        "write 3 canvas1 10\nread r canvas1 10\nstop\n",
        &[LogicLink::new("canvas1", 10, 10)],
    );
    tick(&mut sim, 3);
    let state = sim
        .ecs
        .0
        .get::<CanvasBlockState>(canvas)
        .expect("canvas state");
    assert_eq!(state.get_pixel(10), 3.0, "write 3 c 10");
    let r = sim
        .ecs
        .0
        .get::<LogicBlockState>(sim.grid.entity_at(TilePos::new(10, 10)).expect("processor"))
        .and_then(|state| {
            state
                .executor
                .arena
                .get_id("r")
                .map(|id| state.executor.arena.get(id).num())
        });
    assert_eq!(r, Some(3.0), "read r c 10");
    // A palette-out-of-range write is ignored (`index >= palette.length`).
    place_linked_world_processor(
        &mut sim,
        10,
        12,
        "write 9 canvas2 11\nstop\n",
        &[LogicLink::new("canvas2", 10, 10)],
    );
    tick(&mut sim, 3);
    let state = sim
        .ecs
        .0
        .get::<CanvasBlockState>(canvas)
        .expect("canvas state");
    assert_eq!(state.get_pixel(11), 0.0, "index 9 is out of the palette");
    assert_events_drained(&sim);
}

/// Canvas save body: `i(len) + bytes`, with the length check on read
/// (`CanvasBlock.java:331-349`).
#[test]
fn canvas_save_roundtrip() {
    use crate::io::entity::{EntityReader, EntityWriter};
    use crate::logic::blocks::canvas::CanvasBlockState;
    use crate::world::block::BlockTable;
    let mut sim = campaign_sim();
    let a = place(&mut sim, "canvas", 20, 20);
    let b = place(&mut sim, "canvas", 22, 20);
    {
        let mut state = sim.ecs.0.get_mut::<CanvasBlockState>(a).expect("state");
        state.set_pixel(5, 2);
        state.set_pixel(70, 6);
    }
    let behavior = sim
        .ecs
        .0
        .get_resource::<BlockTable>()
        .and_then(|table| {
            sim.ecs
                .0
                .get::<crate::entities::comp::Building>(a)
                .map(|building| building.block)
                .and_then(|block| table.get(block))
        })
        .map(|inst| inst.behavior.clone())
        .expect("behavior");
    let mut buf: Vec<u8> = Vec::new();
    behavior.write(&sim.ecs.0, a, &mut EntityWriter::new(&mut buf));
    let mut reader = EntityReader::new(&buf);
    behavior.read(&mut sim.ecs.0, b, &mut reader, 0);
    assert_eq!(
        sim.ecs.0.get::<CanvasBlockState>(a).map(|s| s.data.clone()),
        sim.ecs.0.get::<CanvasBlockState>(b).map(|s| s.data.clone()),
        "the canvas data round-trips"
    );
}
