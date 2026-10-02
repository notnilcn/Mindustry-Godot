// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Trigger` registry — the per-frame event hooks of `EventType.Trigger`.
//!
//! Ported from `core/src/mindustry/game/EventType.java` (`enum Trigger`, all 41
//! values) and Arc `Events.run(Trigger, Runnable)`. Draw/universe triggers are
//! fired by `mind-gdext` only; `Sim` never fires them (plan 05 §3.7).

use bevy_ecs::world::World;

/// One `EventType.Trigger` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Trigger {
    /// `shock`.
    Shock = 0,
    /// `cannotUpgrade`.
    CannotUpgrade,
    /// `fireCreate`.
    FireCreate,
    /// `openConsole`.
    OpenConsole,
    /// `blastFreeze`.
    BlastFreeze,
    /// `impactPower`.
    ImpactPower,
    /// `blastGenerator`.
    BlastGenerator,
    /// `shockwaveTowerUse`.
    ShockwaveTowerUse,
    /// `forceProjectorBreak`.
    ForceProjectorBreak,
    /// `thoriumReactorOverheat`.
    ThoriumReactorOverheat,
    /// `neoplasmReact`.
    NeoplasmReact,
    /// `fireExtinguish`.
    FireExtinguish,
    /// `acceleratorUse`.
    AcceleratorUse,
    /// `newGame`.
    NewGame,
    /// `tutorialComplete`.
    TutorialComplete,
    /// `flameAmmo`.
    FlameAmmo,
    /// `resupplyTurret`.
    ResupplyTurret,
    /// `turretCool`.
    TurretCool,
    /// `enablePixelation`.
    EnablePixelation,
    /// `exclusionDeath`.
    ExclusionDeath,
    /// `suicideBomb`.
    SuicideBomb,
    /// `openWiki`.
    OpenWiki,
    /// `teamCoreDamage`.
    TeamCoreDamage,
    /// `socketConfigChanged`.
    SocketConfigChanged,
    /// `update`.
    Update,
    /// `beforeGameUpdate`.
    BeforeGameUpdate,
    /// `afterGameUpdate`.
    AfterGameUpdate,
    /// `unitCommandChange`.
    UnitCommandChange,
    /// `unitCommandPosition`.
    UnitCommandPosition,
    /// `unitCommandAttack`.
    UnitCommandAttack,
    /// `unitCommandBoost`.
    UnitCommandBoost,
    /// `importMod`.
    ImportMod,
    /// `draw` (view-only).
    Draw,
    /// `drawOver` (view-only).
    DrawOver,
    /// `preDraw` (view-only).
    PreDraw,
    /// `postDraw` (view-only).
    PostDraw,
    /// `uiDrawBegin` (view-only).
    UiDrawBegin,
    /// `uiDrawEnd` (view-only).
    UiDrawEnd,
    /// `universeDrawBegin` (view-only).
    UniverseDrawBegin,
    /// `universeDraw` (view-only).
    UniverseDraw,
    /// `universeDrawEnd` (view-only).
    UniverseDrawEnd,
}

impl Trigger {
    /// Number of triggers (`Trigger::COUNT`).
    pub const COUNT: usize = 41;

    /// Dense index.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Parity name (matches the Java enum constant).
    pub const fn name(self) -> &'static str {
        match self {
            Trigger::Shock => "shock",
            Trigger::CannotUpgrade => "cannotUpgrade",
            Trigger::FireCreate => "fireCreate",
            Trigger::OpenConsole => "openConsole",
            Trigger::BlastFreeze => "blastFreeze",
            Trigger::ImpactPower => "impactPower",
            Trigger::BlastGenerator => "blastGenerator",
            Trigger::ShockwaveTowerUse => "shockwaveTowerUse",
            Trigger::ForceProjectorBreak => "forceProjectorBreak",
            Trigger::ThoriumReactorOverheat => "thoriumReactorOverheat",
            Trigger::NeoplasmReact => "neoplasmReact",
            Trigger::FireExtinguish => "fireExtinguish",
            Trigger::AcceleratorUse => "acceleratorUse",
            Trigger::NewGame => "newGame",
            Trigger::TutorialComplete => "tutorialComplete",
            Trigger::FlameAmmo => "flameAmmo",
            Trigger::ResupplyTurret => "resupplyTurret",
            Trigger::TurretCool => "turretCool",
            Trigger::EnablePixelation => "enablePixelation",
            Trigger::ExclusionDeath => "exclusionDeath",
            Trigger::SuicideBomb => "suicideBomb",
            Trigger::OpenWiki => "openWiki",
            Trigger::TeamCoreDamage => "teamCoreDamage",
            Trigger::SocketConfigChanged => "socketConfigChanged",
            Trigger::Update => "update",
            Trigger::BeforeGameUpdate => "beforeGameUpdate",
            Trigger::AfterGameUpdate => "afterGameUpdate",
            Trigger::UnitCommandChange => "unitCommandChange",
            Trigger::UnitCommandPosition => "unitCommandPosition",
            Trigger::UnitCommandAttack => "unitCommandAttack",
            Trigger::UnitCommandBoost => "unitCommandBoost",
            Trigger::ImportMod => "importMod",
            Trigger::Draw => "draw",
            Trigger::DrawOver => "drawOver",
            Trigger::PreDraw => "preDraw",
            Trigger::PostDraw => "postDraw",
            Trigger::UiDrawBegin => "uiDrawBegin",
            Trigger::UiDrawEnd => "uiDrawEnd",
            Trigger::UniverseDrawBegin => "universeDrawBegin",
            Trigger::UniverseDraw => "universeDraw",
            Trigger::UniverseDrawEnd => "universeDrawEnd",
        }
    }

    /// Whether this trigger is render-only (never fired by `Sim`).
    pub const fn is_view_only(self) -> bool {
        matches!(
            self,
            Trigger::Draw
                | Trigger::DrawOver
                | Trigger::PreDraw
                | Trigger::PostDraw
                | Trigger::UiDrawBegin
                | Trigger::UiDrawEnd
                | Trigger::UniverseDrawBegin
                | Trigger::UniverseDraw
                | Trigger::UniverseDrawEnd
        )
    }
}

/// Every trigger, in enum order (index == discriminant).
pub static ALL_TRIGGERS: [Trigger; Trigger::COUNT] = [
    Trigger::Shock,
    Trigger::CannotUpgrade,
    Trigger::FireCreate,
    Trigger::OpenConsole,
    Trigger::BlastFreeze,
    Trigger::ImpactPower,
    Trigger::BlastGenerator,
    Trigger::ShockwaveTowerUse,
    Trigger::ForceProjectorBreak,
    Trigger::ThoriumReactorOverheat,
    Trigger::NeoplasmReact,
    Trigger::FireExtinguish,
    Trigger::AcceleratorUse,
    Trigger::NewGame,
    Trigger::TutorialComplete,
    Trigger::FlameAmmo,
    Trigger::ResupplyTurret,
    Trigger::TurretCool,
    Trigger::EnablePixelation,
    Trigger::ExclusionDeath,
    Trigger::SuicideBomb,
    Trigger::OpenWiki,
    Trigger::TeamCoreDamage,
    Trigger::SocketConfigChanged,
    Trigger::Update,
    Trigger::BeforeGameUpdate,
    Trigger::AfterGameUpdate,
    Trigger::UnitCommandChange,
    Trigger::UnitCommandPosition,
    Trigger::UnitCommandAttack,
    Trigger::UnitCommandBoost,
    Trigger::ImportMod,
    Trigger::Draw,
    Trigger::DrawOver,
    Trigger::PreDraw,
    Trigger::PostDraw,
    Trigger::UiDrawBegin,
    Trigger::UiDrawEnd,
    Trigger::UniverseDrawBegin,
    Trigger::UniverseDraw,
    Trigger::UniverseDrawEnd,
];

type TriggerFn = fn(&mut World);

/// Registration-order trigger listener lists (Arc `Events.run`).
pub struct TriggerRegistry {
    listeners: [Vec<TriggerFn>; Trigger::COUNT],
    fire_counts: [u64; Trigger::COUNT],
}

impl std::fmt::Debug for TriggerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total: usize = self.listeners.iter().map(Vec::len).sum();
        f.debug_struct("TriggerRegistry")
            .field("listeners", &total)
            .finish()
    }
}

impl Default for TriggerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            listeners: std::array::from_fn(|_| Vec::new()),
            fire_counts: [0; Trigger::COUNT],
        }
    }

    /// Registers a listener (no-op in release if called mid-fire is impossible
    /// because listeners are `fn` pointers with no registry access).
    pub fn on(&mut self, trigger: Trigger, listener: TriggerFn) {
        self.listeners[trigger.index()].push(listener);
    }

    /// Fires a trigger to every listener in registration order.
    pub fn fire(&mut self, trigger: Trigger, world: &mut World) {
        self.fire_counts[trigger.index()] = self.fire_counts[trigger.index()].wrapping_add(1);
        for listener in &self.listeners[trigger.index()] {
            listener(world);
        }
    }

    /// How many times a trigger has fired (order-trace diagnostics).
    pub fn fire_count(&self, trigger: Trigger) -> u64 {
        self.fire_counts[trigger.index()]
    }

    /// Total registered listeners.
    pub fn listener_count(&self) -> usize {
        self.listeners.iter().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_listener(world: &mut World) {
        if let Some(mut counter) = world.get_resource_mut::<Counter>() {
            counter.0 += 1;
        } else {
            world.insert_resource(Counter(1));
        }
    }

    #[derive(Clone, Default, bevy_ecs::resource::Resource)]
    struct Counter(u32);

    #[test]
    fn all_triggers_are_addressable_and_named() {
        assert_eq!(ALL_TRIGGERS.len(), Trigger::COUNT);
        for (index, trigger) in ALL_TRIGGERS.into_iter().enumerate() {
            assert_eq!(trigger.index(), index);
            assert!(!trigger.name().is_empty());
        }
    }

    #[test]
    fn trigger_order_fires_registered_listeners() {
        let mut registry = TriggerRegistry::new();
        let mut world = World::new();
        registry.on(Trigger::Update, count_listener);
        registry.on(Trigger::Update, count_listener);
        registry.on(Trigger::AfterGameUpdate, count_listener);
        registry.fire(Trigger::Update, &mut world);
        registry.fire(Trigger::AfterGameUpdate, &mut world);
        registry.fire(Trigger::AfterGameUpdate, &mut world);
        let counter = world.resource::<Counter>();
        assert_eq!(counter.0, 4);
        assert_eq!(registry.fire_count(Trigger::Update), 1);
        assert_eq!(registry.listener_count(), 3);
    }

    #[test]
    fn view_triggers_are_flagged() {
        assert!(Trigger::Draw.is_view_only());
        assert!(!Trigger::Update.is_view_only());
    }
}
