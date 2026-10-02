// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FogControl` (plan 12 M7).
//!
//! Ported from `core/src/mindustry/game/FogControl.java`. Upstream runs two
//! background threads (static + dynamic fog); this port folds both into the
//! deterministic fixed-step tick (deviation 2): static events drain in queue
//! order each tick, dynamic flushes are quantized to
//! [`DYNAMIC_UPDATE_INTERVAL_TICKS`], and every buffer is bit-identical and
//! replayable. The `static-fog-data` save chunk keeps the upstream RLE layout
//! (§6.5).

use super::rules::DYNAMIC_UPDATE_INTERVAL_TICKS;

/// Packed fog event `(x:16 | y:16 | radius:16 | team:8)`.
pub const fn fog_event(x: i32, y: i32, radius: i32, team: u8) -> u64 {
    (x as u64 & 0xFFFF)
        | ((y as u64 & 0xFFFF) << 16)
        | ((radius as u64 & 0xFFFF) << 32)
        | ((team as u64 & 0xFF) << 48)
}

/// X component of a packed fog event.
pub const fn event_x(event: u64) -> i32 {
    (event & 0xFFFF) as u16 as i16 as i32
}

/// Y component of a packed fog event.
pub const fn event_y(event: u64) -> i32 {
    ((event >> 16) & 0xFFFF) as u16 as i16 as i32
}

/// Radius component of a packed fog event.
pub const fn event_radius(event: u64) -> i32 {
    ((event >> 32) & 0xFFFF) as u16 as i16 as i32
}

/// Team component of a packed fog event.
pub const fn event_team(event: u64) -> u8 {
    ((event >> 48) & 0xFF) as u8
}

/// Bit array equivalent to Arc `Bits` (only the operations `FogControl` uses).
#[derive(Debug, Clone, PartialEq)]
pub struct FogBits {
    words: Vec<u64>,
    len: usize,
}

impl Default for FogBits {
    fn default() -> Self {
        Self::new(0)
    }
}

impl FogBits {
    /// A zeroed bit array of `len` bits.
    pub fn new(len: usize) -> Self {
        Self {
            words: vec![0; len.div_ceil(64)],
            len,
        }
    }

    /// Number of addressable bits.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the array has no bits.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Bit `index` (`false` out of range).
    pub fn get(&self, index: usize) -> bool {
        index < self.len
            && self
                .words
                .get(index / 64)
                .is_some_and(|word| (word >> (index % 64)) & 1 != 0)
    }

    /// Sets bit `index`.
    pub fn set_bit(&mut self, index: usize, value: bool) {
        if index >= self.len {
            return;
        }
        if let Some(word) = self.words.get_mut(index / 64) {
            let mask = 1u64 << (index % 64);
            if value {
                *word |= mask;
            } else {
                *word &= !mask;
            }
        }
    }

    /// Sets `[from, to)` (`Bits.set`).
    pub fn set(&mut self, from: usize, to: usize) {
        let to = to.min(self.len);
        for index in from.min(self.len)..to {
            self.set_bit(index, true);
        }
    }

    /// Zeroes every bit (`Bits.clear`).
    pub fn clear(&mut self) {
        self.words.fill(0);
    }
}

/// Per-team fog buffers (`FogControl.FogData`).
#[derive(Debug, Clone, PartialEq)]
pub struct FogData {
    /// Dynamic (live) visibility double buffer.
    pub read: FogBits,
    /// Dynamic visibility being written.
    pub write: FogBits,
    /// Static exploration fog.
    pub static_data: FogBits,
    /// Last dynamic flush tick.
    pub last_dynamic_tick: u64,
    /// A dynamic flush is pending.
    pub dynamic_updated: bool,
}

impl FogData {
    /// Buffers sized for a `ww * wh` world.
    pub fn new(len: usize) -> Self {
        Self {
            read: FogBits::new(len),
            write: FogBits::new(len),
            static_data: FogBits::new(len),
            last_dynamic_tick: 0,
            dynamic_updated: true,
        }
    }
}

/// One fog-emitting building/unit (`hasFogRadius` sources).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogSource {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Fog radius in tiles.
    pub radius: i32,
    /// Owning team id.
    pub team: u8,
}

/// `FogControl` deterministic port.
#[derive(Debug, Clone)]
pub struct FogControl {
    /// World width in tiles.
    pub ww: u16,
    /// World height in tiles.
    pub wh: u16,
    /// Per-team data indexed by team id.
    pub fog: Box<[Option<FogData>; 256]>,
    /// Pending static (exploration) events in queue order.
    pub static_events: Vec<u64>,
    /// Dynamic events accumulated for the next flush.
    pub dynamic_event_queue: Vec<u64>,
    /// Per-team unit events for this update.
    pub unit_event_queue: Vec<u64>,
    /// Dynamic events being drawn.
    pub dynamic_events: Vec<u64>,
    /// Initial static pass already run.
    pub loaded_static: bool,
    /// A world was just loaded (forces an immediate dynamic pass).
    pub just_loaded: bool,
    /// Rotating building-chunk cursor.
    pub last_entity_update_index: usize,
}

impl Default for FogControl {
    fn default() -> Self {
        Self::new()
    }
}

impl FogControl {
    /// Empty controller (`FogControl()`).
    pub fn new() -> Self {
        Self {
            ww: 0,
            wh: 0,
            fog: Box::new(std::array::from_fn(|_| None)),
            static_events: Vec::new(),
            dynamic_event_queue: Vec::new(),
            unit_event_queue: Vec::new(),
            dynamic_events: Vec::new(),
            loaded_static: false,
            just_loaded: false,
            last_entity_update_index: 0,
        }
    }

    /// `WorldLoadEvent` handler: sizes the grid and runs the initial static pass.
    pub fn on_world_load(
        &mut self,
        width: u16,
        height: u16,
        fog_enabled: bool,
        static_fog: bool,
        blocks: &[FogSource],
    ) {
        self.stop();
        self.loaded_static = false;
        self.just_loaded = true;
        self.ww = width;
        self.wh = height;
        if fog_enabled && static_fog {
            self.push_static_blocks(blocks, true);
            self.update_static();
            self.loaded_static = true;
        }
    }

    /// `getDiscovered`.
    pub fn get_discovered(&self, team: u8) -> Option<&FogBits> {
        self.fog[team as usize]
            .as_ref()
            .map(|data| &data.static_data)
    }

    /// `isDiscovered`.
    pub fn is_discovered(
        &self,
        team: u8,
        x: i32,
        y: i32,
        static_fog: bool,
        fog_enabled: bool,
        is_ai: bool,
    ) -> bool {
        if !static_fog || !fog_enabled || is_ai {
            return true;
        }
        let Some(data) = self.get_discovered(team) else {
            return false;
        };
        if x < 0 || y < 0 || x >= self.ww as i32 || y >= self.wh as i32 {
            return false;
        }
        data.get(x as usize + y as usize * self.ww as usize)
    }

    /// `isVisibleTile`.
    pub fn is_visible_tile(
        &self,
        team: u8,
        x: i32,
        y: i32,
        fog_enabled: bool,
        is_ai: bool,
    ) -> bool {
        if !fog_enabled || is_ai {
            return true;
        }
        let Some(data) = self.fog[team as usize].as_ref() else {
            return false;
        };
        let cx = x.clamp(0, self.ww as i32 - 1) as usize;
        let cy = y.clamp(0, self.wh as i32 - 1) as usize;
        data.read.get(cx + cy * self.ww as usize)
    }

    /// `isVisible` (world pixels -> tile).
    pub fn is_visible(&self, team: u8, x: f32, y: f32, fog_enabled: bool, is_ai: bool) -> bool {
        use crate::content::registries::blocks::TILE_SIZE;
        self.is_visible_tile(
            team,
            (x / TILE_SIZE).floor() as i32,
            (y / TILE_SIZE).floor() as i32,
            fog_enabled,
            is_ai,
        )
    }

    /// `resetFog`.
    pub fn reset_fog(&mut self) {
        *self.fog = std::array::from_fn(|_| None);
    }

    /// `stop`: clears every buffer and queue.
    pub fn stop(&mut self) {
        self.last_entity_update_index = 0;
        *self.fog = std::array::from_fn(|_| None);
        self.static_events.clear();
        self.dynamic_event_queue.clear();
        self.unit_event_queue.clear();
        self.dynamic_events.clear();
    }

    /// `pushStaticBlocks`.
    pub fn push_static_blocks(&mut self, blocks: &[FogSource], initial: bool) {
        for block in blocks {
            let index = block.team as usize;
            if self.fog[index].is_none() {
                self.fog[index] = Some(FogData::new(self.world_len()));
            }
            let event = fog_event(block.x, block.y, block.radius, block.team);
            self.push_event(event, initial);
        }
    }

    /// `pushEvent`.
    pub fn push_event(&mut self, event: u64, skip_render: bool) {
        let _ = skip_render;
        // Static fog gate is enforced by the caller (`state.rules.staticFog`).
        self.static_events.push(event);
    }

    /// `forceUpdate`.
    pub fn force_update(&mut self, x: i32, y: i32, radius: i32, team: u8, static_fog: bool) {
        if let Some(data) = self.fog[team as usize].as_mut() {
            data.dynamic_updated = true;
            if static_fog {
                let event = fog_event(x, y, radius, team);
                self.push_event(event, false);
            }
        }
    }

    /// `FogControl.update` (static drain + chunked visibility + dynamic flush).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        tick: u64,
        present_teams: &[u8],
        blocks: &[FogSource],
        units: &[FogSource],
        fog_enabled: bool,
        static_fog: bool,
        headless: bool,
    ) {
        if static_fog && !self.loaded_static {
            self.push_static_blocks(blocks, false);
            self.update_static();
            self.loaded_static = true;
        }

        self.dynamic_event_queue.clear();

        // Chunked building visibility refresh (`updateFogVisibility`).
        if fog_enabled && !headless && !blocks.is_empty() {
            let size = blocks.len();
            let chunk_size = 5usize;
            let chunks = chunk_size.min(size);
            let iterated = (size / chunks).max(1);
            let mut steps = 0;
            let mut i = self.last_entity_update_index % size;
            while steps < iterated {
                if let Some(block) = blocks.get(i)
                    && let Some(data) = self.fog[block.team as usize].as_mut()
                {
                    data.dynamic_updated = true;
                }
                steps += 1;
                i += 1;
                if i >= size {
                    i = 0;
                }
            }
            self.last_entity_update_index = i;
        }

        for &team in present_teams {
            self.unit_event_queue.clear();
            let index = team as usize;

            for unit in units.iter().filter(|unit| unit.team == team) {
                if unit.radius <= 0 {
                    continue;
                }
                let event = fog_event(unit.x, unit.y, unit.radius, team);
                self.unit_event_queue.push(event);
                self.push_event(event, false);
                if let Some(data) = self.fog[index].as_mut() {
                    data.dynamic_updated = true;
                }
            }

            let flush = self.fog[index].as_ref().is_some_and(|data| {
                data.dynamic_updated
                    && tick.saturating_sub(data.last_dynamic_tick) >= DYNAMIC_UPDATE_INTERVAL_TICKS
            });
            if flush {
                if let Some(data) = self.fog[index].as_mut() {
                    data.dynamic_updated = false;
                    data.last_dynamic_tick = tick;
                }
                for block in blocks.iter().filter(|block| block.team == team) {
                    self.dynamic_event_queue.push(fog_event(
                        block.x,
                        block.y,
                        block.radius,
                        block.team,
                    ));
                }
                let unit_events = std::mem::take(&mut self.unit_event_queue);
                self.dynamic_event_queue.extend(unit_events);
            }
        }

        // Deterministic replacement for the upstream static fog thread: drain
        // every queued static event in order on the same tick.
        self.update_static();

        if !self.dynamic_event_queue.is_empty() {
            self.dynamic_events = std::mem::take(&mut self.dynamic_event_queue);
            if self.just_loaded {
                let mut cleared = vec![false; 256];
                self.update_dynamic(&mut cleared);
                self.just_loaded = false;
            }
        }
    }

    /// `updateStatic`: drains the static queue into `static_data`.
    pub fn update_static(&mut self) {
        let events = std::mem::take(&mut self.static_events);
        for event in events {
            let x = event_x(event);
            let y = event_y(event);
            let radius = event_radius(event);
            let team = event_team(event) as usize;
            if let Some(data) = self.fog[team].as_mut() {
                let (ww, wh) = (self.ww, self.wh);
                circle(&mut data.static_data, x, y, radius, ww, wh);
            }
        }
    }

    /// `updateDynamic`: draws into `write` and swaps the double buffers.
    pub fn update_dynamic(&mut self, cleared: &mut [bool]) {
        cleared.fill(false);
        let events = std::mem::take(&mut self.dynamic_events);
        for event in events {
            let x = event_x(event);
            let y = event_y(event);
            let radius = event_radius(event);
            let team = event_team(event) as usize;
            if radius <= 0 {
                continue;
            }
            if let Some(data) = self.fog[team].as_mut() {
                if !cleared[team] {
                    cleared[team] = true;
                    data.write.clear();
                }
                let (ww, wh) = (self.ww, self.wh);
                circle(&mut data.write, x, y, radius + 1, ww, wh);
            }
        }
        for (team, is_cleared) in cleared.iter().enumerate().take(256) {
            if *is_cleared && let Some(data) = self.fog[team].as_mut() {
                std::mem::swap(&mut data.read, &mut data.write);
            }
        }
    }

    /// `shouldWrite`.
    pub fn should_write(&self, fog_enabled: bool, static_fog: bool) -> bool {
        fog_enabled && static_fog && self.fog.iter().any(Option::is_some)
    }

    /// `CustomChunk.write` (`static-fog-data`, §6.5).
    pub fn write(&self) -> Vec<u8> {
        let used = self.fog.iter().filter(|data| data.is_some()).count();
        let mut out = Vec::new();
        out.push(used as u8);
        out.extend_from_slice(&self.ww.to_be_bytes());
        out.extend_from_slice(&self.wh.to_be_bytes());
        let size = self.world_len();
        for (team, data) in self.fog.iter().enumerate() {
            if let Some(data) = data {
                out.push(team as u8);
                let mut pos = 0usize;
                while pos < size {
                    let cur = data.static_data.get(pos);
                    let mut consecutives = 0usize;
                    while consecutives < 127 && pos < size && cur == data.static_data.get(pos) {
                        consecutives += 1;
                        pos += 1;
                    }
                    let mask = if cur { 0b1000_0000u8 } else { 0 };
                    out.push(mask | (consecutives as u8));
                }
            }
        }
        out
    }

    /// `CustomChunk.read` (`static-fog-data`, §6.5).
    pub fn read(&mut self, bytes: &[u8]) {
        let mut cursor = 0usize;
        let Some(&teams) = bytes.get(cursor) else {
            return;
        };
        cursor += 1;
        let read_u16 = |bytes: &[u8], at: usize| -> u16 {
            bytes
                .get(at..at + 2)
                .map(|slice| u16::from_be_bytes([slice[0], slice[1]]))
                .unwrap_or(0)
        };
        let w = read_u16(bytes, cursor);
        cursor += 2;
        let h = read_u16(bytes, cursor);
        cursor += 2;
        self.ww = w;
        self.wh = h;
        let len = w as usize * h as usize;
        for _ in 0..teams {
            let Some(&team) = bytes.get(cursor) else {
                break;
            };
            cursor += 1;
            let mut data = FogData::new(len);
            let mut pos = 0usize;
            while pos < len {
                let Some(&raw) = bytes.get(cursor) else {
                    break;
                };
                cursor += 1;
                let sign = raw & 0b1000_0000 != 0;
                let consec = (raw & 0b0111_1111) as usize;
                if sign {
                    data.static_data.set(pos, pos + consec);
                }
                pos += consec;
            }
            self.fog[team as usize] = Some(data);
        }
    }

    fn world_len(&self) -> usize {
        self.ww as usize * self.wh as usize
    }
}

/// Arc `FogControl.circle`: midpoint circle rasterization into `bits`.
pub fn circle(bits: &mut FogBits, x: i32, y: i32, radius: i32, ww: u16, wh: u16) {
    let mut f = 1 - radius;
    let mut dd_fx = 1;
    let mut dd_fy = -2 * radius;
    let mut px = 0;
    let mut py = radius;

    hline(bits, x, x, y + radius, ww, wh);
    hline(bits, x, x, y - radius, ww, wh);
    hline(bits, x - radius, x + radius, y, ww, wh);

    while px < py {
        if f >= 0 {
            py -= 1;
            dd_fy += 2;
            f += dd_fy;
        }
        px += 1;
        dd_fx += 2;
        f += dd_fx;
        hline(bits, x - px, x + px, y + py, ww, wh);
        hline(bits, x - px, x + px, y - py, ww, wh);
        hline(bits, x - py, x + py, y + px, ww, wh);
        hline(bits, x - py, x + py, y - px, ww, wh);
    }
}

/// Arc `FogControl.hline`: clipped horizontal span `[x1, x2]`.
pub fn hline(bits: &mut FogBits, x1: i32, x2: i32, y: i32, ww: u16, wh: u16) {
    let (ww, wh) = (ww as i32, wh as i32);
    if y < 0 || y >= wh {
        return;
    }
    let (mut x1, mut x2) = if x1 > x2 { (x2, x1) } else { (x1, x2) };
    if x1 >= ww || x2 < 0 {
        return;
    }
    if x1 < 0 {
        x1 = 0;
    }
    if x2 >= ww {
        x2 = ww - 1;
    }
    x2 += 1;
    let off = y * ww;
    bits.set((off + x1) as usize, (off + x2) as usize);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(x: i32, y: i32, radius: i32, team: u8) -> FogSource {
        FogSource { x, y, radius, team }
    }

    #[test]
    fn event_packing_roundtrips() {
        let event = fog_event(12, 300, 40, 2);
        assert_eq!(event_x(event), 12);
        assert_eq!(event_y(event), 300);
        assert_eq!(event_radius(event), 40);
        assert_eq!(event_team(event), 2);
    }

    #[test]
    fn circle_and_hline_clip_at_edges() {
        let mut bits = FogBits::new(16 * 16);
        // Circle at the corner is clipped, not out-of-bounds.
        circle(&mut bits, 0, 0, 4, 16, 16);
        assert!(bits.get(0));
        assert!(bits.get(4));
        assert!(!bits.get(15));
        // Hline fully outside is a no-op.
        hline(&mut bits, -10, -1, 3, 16, 16);
        hline(&mut bits, 5, 9, -1, 16, 16);
        assert!(!bits.get(3 * 16 + 5));
    }

    #[test]
    fn static_fog_exploration_roundtrip() {
        let mut fog = FogControl::new();
        let blocks = vec![source(8, 8, 3, 0)];
        fog.on_world_load(32, 32, true, true, &blocks);
        // Center discovered, far corner not.
        assert!(fog.is_discovered(0, 8, 8, true, true, false));
        assert!(!fog.is_discovered(0, 31, 31, true, true, false));

        // RLE chunk round-trips through a fresh controller.
        let bytes = fog.write();
        let mut restored = FogControl::new();
        restored.read(&bytes);
        assert_eq!(restored.ww, 32);
        assert_eq!(restored.wh, 32);
        assert_eq!(
            restored.fog[0].as_ref().unwrap().static_data,
            fog.fog[0].as_ref().unwrap().static_data
        );
    }

    #[test]
    fn dynamic_visibility_cadence_is_deterministic() {
        let mut fog = FogControl::new();
        let blocks = vec![source(4, 4, 3, 0)];
        let units = vec![source(10, 10, 4, 0)];
        fog.on_world_load(16, 16, true, true, &blocks);
        // First update at the milestone tick flushes and swaps buffers.
        fog.update(40, &[0], &blocks, &units, true, true, false);
        assert!(fog.is_visible_tile(0, 10, 10, true, false));
        assert!(fog.is_visible_tile(0, 4, 4, true, false));

        // Same seed of inputs + ticks gives identical buffers.
        let mut other = FogControl::new();
        other.on_world_load(16, 16, true, true, &blocks);
        other.update(40, &[0], &blocks, &units, true, true, false);
        assert_eq!(other.write(), fog.write());
        let a = other.fog[0].as_ref().unwrap();
        let b = fog.fog[0].as_ref().unwrap();
        assert_eq!(a.read, b.read);
        assert_eq!(a.write, b.write);
    }

    #[test]
    fn ai_teams_ignore_fog() {
        let mut fog = FogControl::new();
        fog.on_world_load(8, 8, true, true, &[]);
        assert!(fog.is_discovered(1, 0, 0, true, true, true));
        assert!(fog.is_visible_tile(1, 0, 0, true, true));
    }

    #[test]
    fn should_write_gate() {
        let mut fog = FogControl::new();
        assert!(!fog.should_write(true, true));
        fog.on_world_load(8, 8, true, true, &[source(1, 1, 2, 0)]);
        assert!(fog.should_write(true, true));
        assert!(!fog.should_write(false, true));
        assert!(!fog.should_write(true, false));
    }
}
