// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla status effect registry.
//!
//! Ported from `core/src/mindustry/content/StatusEffects.java` (23 statuses,
//! exact `load()` order) and `core/src/mindustry/type/StatusEffect.java`
//! (metadata + transition tables; behavior in plans 05/11).
//!
//! Transition declaration is deferred data (plan 02 §2.4.5): `init(() -> {
//! opposite(...); affinity(...); })` blocks become `init_ops`, and the registry
//! `link()` pass performs the symmetric cross-content mutation in load order.

use super::super::bundle::BundleView;
use super::super::color::Rgba;
use super::super::ctype::{Content, Mappable, ModContentInfo, UnlockFields, Unlockable};
use super::super::id::StatusId;
use super::super::load::ContentRegistry;
use super::super::settings_store::UnlockStore;
use super::super::{ContentError, ContentType};
use super::fx_meta::EffectId;

/// Game trigger fired by a transition handler (`Trigger.shock`/`blastFreeze`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionTrigger {
    /// `Trigger.shock` (wet -> shocked).
    Shock,
    /// `Trigger.blastFreeze` (freezing -> blasted).
    BlastFreeze,
}

/// Data description of an `affinity(other, handler)` transition.
///
/// Subsumes the plan §3.8 `ExtendAffinity`/`Damage`/`SetEffect` shapes so
/// composite handlers (burning/melting -> tarred) lose no data.
#[derive(Debug, Clone, PartialEq)]
pub struct AffinityTransition {
    /// Immediate damage applied by the handler (`0` = none).
    pub damage: f32,
    /// Whether the damage is armor-piercing (`damagePierce`).
    pub damage_pierce: bool,
    /// Effect played at the unit (e.g. `Fx.burning`).
    pub effect: Option<EffectId>,
    /// `result.set(effect, time + result.time)` with an optional cap.
    pub extend: Option<(StatusId, Option<f32>)>,
    /// Game trigger fired by the handler.
    pub trigger: Option<TransitionTrigger>,
}

impl AffinityTransition {
    /// Empty handler.
    pub const fn none() -> Self {
        Self {
            damage: 0.0,
            damage_pierce: false,
            effect: None,
            extend: None,
            trigger: None,
        }
    }

    /// Handler that applies immediate damage.
    pub const fn damage(amount: f32, pierce: bool) -> Self {
        Self {
            damage: amount,
            damage_pierce: pierce,
            effect: None,
            extend: None,
            trigger: None,
        }
    }

    /// Handler that extends an effect without a cap.
    pub const fn extend(effect: StatusId) -> Self {
        Self {
            damage: 0.0,
            damage_pierce: false,
            effect: None,
            extend: Some((effect, None)),
            trigger: None,
        }
    }
}

/// One transition handler entry (`StatusEffect.transitions` value).
#[derive(Debug, Clone, PartialEq)]
pub enum TransitionSpec {
    /// `handleOpposite`: `result.time -= time * 0.5`, flips to the opposite at <= 0.
    Opposite,
    /// An affinity handler's data.
    Affinity(AffinityTransition),
}

/// Pre-link transition declaration (`init(() -> { ... })` body).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TransitionInit {
    /// `opposite(other)`.
    Opposite(StatusId),
    /// `affinity(other, handler)`.
    Affinity(StatusId, AffinityTransition),
}

/// Status effect content record (`mindustry.type.StatusEffect`).
#[derive(Debug, Clone, PartialEq)]
pub struct StatusEffect {
    /// Dense id in the status content space.
    pub id: StatusId,
    /// Content name (parity ABI).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Unlock/database fields.
    pub unlock: UnlockFields,
    /// Damage multiplier dealt by the affected unit.
    pub damage_multiplier: f32,
    /// Unit health multiplier.
    pub health_multiplier: f32,
    /// Unit speed multiplier.
    pub speed_multiplier: f32,
    /// Unit reload multiplier.
    pub reload_multiplier: f32,
    /// Unit build speed multiplier.
    pub build_speed_multiplier: f32,
    /// Unit drag multiplier.
    pub drag_multiplier: f32,
    /// Damage on transition to an affinity.
    pub transition_damage: f32,
    /// Unit weapon(s) disabled.
    pub disarm: bool,
    /// Damage per frame (negative heals).
    pub damage: f32,
    /// Spacing in ticks between interval damage (`<=0` disables).
    pub interval_damage_time: f32,
    /// Damage dealt by interval damage.
    pub interval_damage: f32,
    /// Whether interval damage is armor piercing.
    pub interval_damage_pierce: bool,
    /// Chance of visual effects appearing.
    pub effect_chance: f32,
    /// Whether the effect is given a parent.
    pub parentize_effect: bool,
    /// If true, the effect never disappears.
    pub permanent: bool,
    /// If true, this effect only reacts and cannot be applied.
    pub reactive: bool,
    /// Special flag for the `dynamic` effect type.
    pub dynamic: bool,
    /// Whether to show this effect in the database.
    pub show: bool,
    /// Tint color.
    pub color: Rgba,
    /// Effect that happens randomly on the affected unit.
    pub effect: EffectId,
    /// Effect displayed once when applied.
    pub apply_effect: EffectId,
    /// Whether the apply effect displays when already applied.
    pub apply_extend: bool,
    /// Tint color of the apply effect.
    pub apply_color: Rgba,
    /// Whether the apply effect is given a parent.
    pub parentize_apply_effect: bool,
    /// Affinities for stat displays (insertion-ordered set).
    pub affinities: Vec<StatusId>,
    /// Opposites for stat displays (insertion-ordered set).
    pub opposites: Vec<StatusId>,
    /// Transition handler map (insertion-ordered pairs).
    pub transitions: Vec<(StatusId, TransitionSpec)>,
    /// Whether outline generation is enabled.
    pub outline: bool,
    /// Deferred `init()` transition declarations.
    pub(crate) init_ops: Vec<TransitionInit>,
}

impl StatusEffect {
    /// Creates a status with upstream defaults (`StatusEffect(String)`).
    pub fn new(name: &str, bundle: &dyn BundleView, store: &dyn UnlockStore) -> Self {
        let mut unlock = UnlockFields::new(ContentType::Status, name, bundle, store);
        unlock.all_database_tabs = true;
        Self {
            id: StatusId::new(0),
            name: name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            unlock,
            damage_multiplier: 1.0,
            health_multiplier: 1.0,
            speed_multiplier: 1.0,
            reload_multiplier: 1.0,
            build_speed_multiplier: 1.0,
            drag_multiplier: 1.0,
            transition_damage: 0.0,
            disarm: false,
            damage: 0.0,
            interval_damage_time: 0.0,
            interval_damage: 0.0,
            interval_damage_pierce: false,
            effect_chance: 0.15,
            parentize_effect: false,
            permanent: false,
            reactive: false,
            dynamic: false,
            show: true,
            color: Rgba::WHITE,
            effect: EffectId::NONE,
            apply_effect: EffectId::NONE,
            apply_extend: false,
            apply_color: Rgba::WHITE,
            parentize_apply_effect: false,
            affinities: Vec::new(),
            opposites: Vec::new(),
            transitions: Vec::new(),
            outline: true,
            init_ops: Vec::new(),
        }
    }

    /// `StatusEffect.isHidden()`: internal name unchanged or `show == false`.
    pub fn is_hidden(&self) -> bool {
        self.unlock.localized_name == self.name || !self.show
    }

    /// Transition handler for `other`, if declared.
    pub fn transition(&self, other: StatusId) -> Option<&TransitionSpec> {
        self.transitions
            .iter()
            .find(|(target, _)| *target == other)
            .map(|(_, spec)| spec)
    }

    /// Whether a transition handler exists (`StatusEffect.reactsWith`).
    pub fn reacts_with(&self, other: StatusId) -> bool {
        self.transition(other).is_some()
    }

    /// Declares `opposite(other)` (applied symmetrically in `link()`).
    pub fn declare_opposite(&mut self, other: StatusId) {
        self.init_ops.push(TransitionInit::Opposite(other));
    }

    /// Declares `affinity(other, handler)`.
    pub fn declare_affinity(&mut self, other: StatusId, handler: AffinityTransition) {
        self.init_ops.push(TransitionInit::Affinity(other, handler));
    }

    /// Sets/replaces a transition handler (insertion order preserved for new pairs).
    pub fn set_transition(&mut self, other: StatusId, spec: TransitionSpec) {
        if let Some((_, existing)) = self
            .transitions
            .iter_mut()
            .find(|(target, _)| *target == other)
        {
            *existing = spec;
        } else {
            self.transitions.push((other, spec));
        }
    }

    /// Adds an affinity if absent (insertion-ordered set).
    pub fn add_affinity(&mut self, other: StatusId) {
        push_unique(&mut self.affinities, other);
    }

    /// Adds an opposite if absent (insertion-ordered set).
    pub fn add_opposite(&mut self, other: StatusId) {
        push_unique(&mut self.opposites, other);
    }
}

impl Content for StatusEffect {
    const TYPE: ContentType = ContentType::Status;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = StatusId::new(id);
    }

    fn minfo(&self) -> &ModContentInfo {
        &self.minfo
    }

    fn minfo_mut(&mut self) -> &mut ModContentInfo {
        &mut self.minfo
    }

    fn removed(&self) -> bool {
        self.removed
    }

    fn set_removed(&mut self, removed: bool) {
        self.removed = removed;
    }

    fn kind_name(&self) -> &'static str {
        "StatusEffect"
    }

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn unlock_fields(&self) -> Option<&UnlockFields> {
        Some(&self.unlock)
    }

    fn post_init(&mut self) -> Result<(), ContentError> {
        self.unlock.post_init();
        Ok(())
    }
}

impl Mappable for StatusEffect {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Unlockable for StatusEffect {
    fn unlock(&self) -> &UnlockFields {
        &self.unlock
    }

    fn unlock_mut(&mut self) -> &mut UnlockFields {
        &mut self.unlock
    }
}

fn push_unique(list: &mut Vec<StatusId>, value: StatusId) {
    if !list.contains(&value) {
        list.push(value);
    }
}

/// Applies all deferred transition declarations symmetrically, in status id order
/// (`ContentLoader.init()` sweep order).
pub(crate) fn link(registry: &mut ContentRegistry) -> Result<(), ContentError> {
    let statuses = registry.statuses_mut();
    let len = statuses.len();
    for index in 0..len {
        let ops = std::mem::take(&mut statuses[index].init_ops);
        let self_id = StatusId::new(index as u16);
        for op in ops {
            match op {
                TransitionInit::Opposite(other) => {
                    let other_index = other.index();
                    if other_index >= len || other_index == index {
                        continue;
                    }
                    statuses[index].add_opposite(other);
                    statuses[other_index].add_opposite(self_id);
                    statuses[index].set_transition(other, TransitionSpec::Opposite);
                    statuses[other_index].set_transition(self_id, TransitionSpec::Opposite);
                }
                TransitionInit::Affinity(other, handler) => {
                    let other_index = other.index();
                    if other_index >= len || other_index == index {
                        continue;
                    }
                    statuses[index].add_affinity(other);
                    statuses[other_index].add_affinity(self_id);
                    statuses[index].set_transition(other, TransitionSpec::Affinity(handler));
                }
            }
        }
    }
    Ok(())
}

/// All status ids, captured during [`load`] for the deferred declarations.
/// Fields not referenced by the declaration table are kept so the load order is
/// auditable against `StatusEffects.load()`.
#[allow(dead_code)]
struct StatusIds {
    none: StatusId,
    burning: StatusId,
    freezing: StatusId,
    unmoving: StatusId,
    slow: StatusId,
    fast: StatusId,
    wet: StatusId,
    muddy: StatusId,
    melting: StatusId,
    sapped: StatusId,
    electrified: StatusId,
    spore_slowed: StatusId,
    tarred: StatusId,
    overdrive: StatusId,
    overclock: StatusId,
    shielded: StatusId,
    boss: StatusId,
    shocked: StatusId,
    blasted: StatusId,
    corroded: StatusId,
    disarmed: StatusId,
    invincible: StatusId,
    dynamic: StatusId,
}

/// Loads all 23 vanilla statuses in `StatusEffects.load()` order.
pub fn load(
    registry: &mut ContentRegistry,
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
) -> Result<(), ContentError> {
    let none = registry.add_status(StatusEffect::new("none", bundle, store))?;

    let burning = registry.add_status({
        let mut status = StatusEffect::new("burning", bundle, store);
        status.color = hex("ffc455");
        status.damage = 0.167;
        status.effect = EffectId::BURNING;
        status.transition_damage = 8.0;
        status
    })?;

    let freezing = registry.add_status({
        let mut status = StatusEffect::new("freezing", bundle, store);
        status.color = hex("6ecdec");
        status.speed_multiplier = 0.6;
        status.health_multiplier = 0.8;
        status.effect = EffectId::FREEZING;
        status.transition_damage = 18.0;
        status
    })?;

    let unmoving = registry.add_status({
        let mut status = StatusEffect::new("unmoving", bundle, store);
        status.color = hex("454545");
        status.speed_multiplier = 0.0;
        status
    })?;

    let slow = registry.add_status({
        let mut status = StatusEffect::new("slow", bundle, store);
        status.color = hex("a2a2a2");
        status.speed_multiplier = 0.4;
        status.show = false;
        status
    })?;

    let fast = registry.add_status({
        let mut status = StatusEffect::new("fast", bundle, store);
        status.color = hex("ffad4d");
        status.speed_multiplier = 1.6;
        status
    })?;

    let wet = registry.add_status({
        let mut status = StatusEffect::new("wet", bundle, store);
        status.color = Rgba::ROYAL;
        status.speed_multiplier = 0.94;
        status.effect = EffectId::WET;
        status.effect_chance = 0.09;
        status.transition_damage = 14.0;
        status
    })?;

    let muddy = registry.add_status({
        let mut status = StatusEffect::new("muddy", bundle, store);
        status.color = hex("46382a");
        status.speed_multiplier = 0.94;
        status.effect = EffectId::MUDDY;
        status.effect_chance = 0.09;
        status.show = false;
        status
    })?;

    let melting = registry.add_status({
        let mut status = StatusEffect::new("melting", bundle, store);
        status.color = hex("ffa166");
        status.speed_multiplier = 0.8;
        status.health_multiplier = 0.8;
        status.damage = 0.3;
        status.effect = EffectId::MELTING;
        status
    })?;

    let sapped = registry.add_status({
        let mut status = StatusEffect::new("sapped", bundle, store);
        status.color = hex("665c9f");
        status.speed_multiplier = 0.7;
        status.health_multiplier = 0.8;
        status.effect = EffectId::SAPPED;
        status.effect_chance = 0.1;
        status
    })?;

    let electrified = registry.add_status({
        let mut status = StatusEffect::new("electrified", bundle, store);
        status.color = hex("98ffa9");
        status.speed_multiplier = 0.7;
        status.reload_multiplier = 0.6;
        status.effect = EffectId::ELECTRIFIED;
        status.effect_chance = 0.1;
        status
    })?;

    let spore_slowed = registry.add_status({
        let mut status = StatusEffect::new("spore-slowed", bundle, store);
        status.color = hex("7457ce");
        status.speed_multiplier = 0.8;
        status.effect = EffectId::SAPPED;
        status.effect_chance = 0.04;
        status
    })?;

    let tarred = registry.add_status({
        let mut status = StatusEffect::new("tarred", bundle, store);
        status.color = hex("313131");
        status.speed_multiplier = 0.6;
        status.effect = EffectId::OILY;
        status
    })?;

    let overdrive = registry.add_status({
        let mut status = StatusEffect::new("overdrive", bundle, store);
        status.color = hex("ffd37f");
        status.health_multiplier = 0.95;
        status.speed_multiplier = 1.15;
        status.damage_multiplier = 1.4;
        status.damage = -0.01;
        status.effect = EffectId::OVERDRIVEN;
        status.permanent = true;
        status
    })?;

    let overclock = registry.add_status({
        let mut status = StatusEffect::new("overclock", bundle, store);
        status.color = hex("ffd37f");
        status.speed_multiplier = 1.15;
        status.damage_multiplier = 1.15;
        status.reload_multiplier = 1.25;
        status.effect_chance = 0.07;
        status.effect = EffectId::OVERCLOCKED;
        status
    })?;

    let shielded = registry.add_status({
        let mut status = StatusEffect::new("shielded", bundle, store);
        status.color = hex("ffd37f");
        status.health_multiplier = 3.0;
        status.show = false;
        status
    })?;

    let boss = registry.add_status({
        let mut status = StatusEffect::new("boss", bundle, store);
        status.color = hex("f25555");
        status.permanent = true;
        status.damage_multiplier = 1.3;
        status.health_multiplier = 1.5;
        status
    })?;

    let shocked = registry.add_status({
        let mut status = StatusEffect::new("shocked", bundle, store);
        status.color = hex("a9d8ff");
        status.reactive = true;
        status
    })?;

    let blasted = registry.add_status({
        let mut status = StatusEffect::new("blasted", bundle, store);
        status.color = hex("ff795e");
        status.reactive = true;
        status
    })?;

    let corroded = registry.add_status({
        let mut status = StatusEffect::new("corroded", bundle, store);
        status.color = hex("e4ffd6");
        status.interval_damage = 20.0;
        status.interval_damage_time = 15.0;
        status.effect_chance = 0.1;
        status.effect = EffectId::CORROSION_VAPOR;
        status
    })?;

    let disarmed = registry.add_status({
        let mut status = StatusEffect::new("disarmed", bundle, store);
        status.color = hex("e9ead3");
        status.disarm = true;
        status.show = false;
        status
    })?;

    let invincible = registry.add_status({
        let mut status = StatusEffect::new("invincible", bundle, store);
        status.health_multiplier = f32::INFINITY;
        status.show = false;
        status
    })?;

    let dynamic = registry.add_status({
        let mut status = StatusEffect::new("dynamic", bundle, store);
        status.show = false;
        status.dynamic = true;
        status.permanent = true;
        status
    })?;

    let ids = StatusIds {
        none,
        burning,
        freezing,
        unmoving,
        slow,
        fast,
        wet,
        muddy,
        melting,
        sapped,
        electrified,
        spore_slowed,
        tarred,
        overdrive,
        overclock,
        shielded,
        boss,
        shocked,
        blasted,
        corroded,
        disarmed,
        invincible,
        dynamic,
    };
    declare_transitions(registry, &ids);
    Ok(())
}

/// Ports the `init(() -> { ... })` blocks from `StatusEffects.load()`.
fn declare_transitions(registry: &mut ContentRegistry, ids: &StatusIds) {
    let mut declare = |from: StatusId, op: TransitionInit| {
        if let Some(status) = registry.status_mut(from) {
            status.init_ops.push(op);
        }
    };

    // burning.init
    declare(ids.burning, TransitionInit::Opposite(ids.wet));
    declare(ids.burning, TransitionInit::Opposite(ids.freezing));
    declare(
        ids.burning,
        TransitionInit::Affinity(
            ids.tarred,
            AffinityTransition {
                damage: 8.0,
                damage_pierce: true,
                effect: Some(EffectId::BURNING),
                extend: Some((ids.burning, Some(300.0))),
                trigger: None,
            },
        ),
    );

    // freezing.init
    declare(ids.freezing, TransitionInit::Opposite(ids.melting));
    declare(ids.freezing, TransitionInit::Opposite(ids.burning));
    declare(
        ids.freezing,
        TransitionInit::Affinity(
            ids.blasted,
            AffinityTransition {
                damage: 18.0,
                damage_pierce: true,
                trigger: Some(TransitionTrigger::BlastFreeze),
                ..AffinityTransition::none()
            },
        ),
    );

    // slow.init / fast.init
    declare(ids.slow, TransitionInit::Opposite(ids.fast));
    declare(ids.fast, TransitionInit::Opposite(ids.slow));

    // wet.init
    declare(
        ids.wet,
        TransitionInit::Affinity(
            ids.shocked,
            AffinityTransition {
                damage: 14.0,
                damage_pierce: false,
                trigger: Some(TransitionTrigger::Shock),
                ..AffinityTransition::none()
            },
        ),
    );
    declare(ids.wet, TransitionInit::Opposite(ids.burning));
    declare(ids.wet, TransitionInit::Opposite(ids.melting));

    // melting.init
    declare(ids.melting, TransitionInit::Opposite(ids.wet));
    declare(ids.melting, TransitionInit::Opposite(ids.freezing));
    declare(
        ids.melting,
        TransitionInit::Affinity(
            ids.tarred,
            AffinityTransition {
                damage: 8.0,
                damage_pierce: true,
                effect: Some(EffectId::BURNING),
                extend: Some((ids.melting, Some(200.0))),
                trigger: None,
            },
        ),
    );

    // tarred.init
    declare(
        ids.tarred,
        TransitionInit::Affinity(ids.melting, AffinityTransition::extend(ids.melting)),
    );
    declare(
        ids.tarred,
        TransitionInit::Affinity(ids.burning, AffinityTransition::extend(ids.burning)),
    );
}

/// Known-good color literal helper (fallback white; colors are static content data).
fn hex(value: &str) -> Rgba {
    Rgba::from_hex(value).unwrap_or(Rgba::WHITE)
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;
    use super::*;

    /// `statuses::opposites_symmetric` (plan 02 §5 M1).
    #[test]
    fn opposites_symmetric() {
        let registry = test_registry();
        assert_eq!(registry.statuses().len(), 23, "status count");
        let expected_pairs: &[(&str, &str)] = &[
            ("burning", "wet"),
            ("burning", "freezing"),
            ("freezing", "melting"),
            ("slow", "fast"),
            ("wet", "melting"),
        ];
        for (a, b) in expected_pairs {
            let a_id = registry.status_id(a).unwrap();
            let b_id = registry.status_id(b).unwrap();
            let status_a = registry.status(a_id).unwrap();
            let status_b = registry.status(b_id).unwrap();
            assert!(
                status_a.opposites.contains(&b_id),
                "{a} must list {b} as opposite"
            );
            assert!(
                status_b.opposites.contains(&a_id),
                "{b} must list {a} as opposite"
            );
            assert!(matches!(
                status_a.transition(b_id),
                Some(TransitionSpec::Opposite)
            ));
            assert!(matches!(
                status_b.transition(a_id),
                Some(TransitionSpec::Opposite)
            ));
        }
        // Opposites only exist as declared pairs.
        let slow = registry
            .status(registry.status_id("slow").unwrap())
            .unwrap();
        assert_eq!(slow.opposites, vec![registry.status_id("fast").unwrap()]);
    }

    /// `statuses::affinity_transition_tables` (plan 02 §5 M1): exact handler data
    /// and symmetric affinity sets.
    #[test]
    fn affinity_transition_tables() {
        let registry = test_registry();
        let id = |name: &str| registry.status_id(name).unwrap();
        let spec = |a: &str, b: &str| {
            registry
                .status(id(a))
                .unwrap()
                .transition(id(b))
                .cloned()
                .unwrap_or_else(|| panic!("{a} -> {b} transition missing"))
        };

        assert_eq!(
            spec("burning", "tarred"),
            TransitionSpec::Affinity(AffinityTransition {
                damage: 8.0,
                damage_pierce: true,
                effect: Some(EffectId::BURNING),
                extend: Some((id("burning"), Some(300.0))),
                trigger: None,
            })
        );
        assert_eq!(
            spec("freezing", "blasted"),
            TransitionSpec::Affinity(AffinityTransition {
                damage: 18.0,
                damage_pierce: true,
                trigger: Some(TransitionTrigger::BlastFreeze),
                ..AffinityTransition::none()
            })
        );
        assert_eq!(
            spec("wet", "shocked"),
            TransitionSpec::Affinity(AffinityTransition {
                damage: 14.0,
                damage_pierce: false,
                trigger: Some(TransitionTrigger::Shock),
                ..AffinityTransition::none()
            })
        );
        assert_eq!(
            spec("melting", "tarred"),
            TransitionSpec::Affinity(AffinityTransition {
                damage: 8.0,
                damage_pierce: true,
                effect: Some(EffectId::BURNING),
                extend: Some((id("melting"), Some(200.0))),
                trigger: None,
            })
        );
        assert_eq!(
            spec("tarred", "melting"),
            TransitionSpec::Affinity(AffinityTransition::extend(id("melting")))
        );
        assert_eq!(
            spec("tarred", "burning"),
            TransitionSpec::Affinity(AffinityTransition::extend(id("burning")))
        );

        // Affinity sets are symmetric and absent where not declared.
        let expected: &[(&str, &str)] = &[
            ("burning", "tarred"),
            ("freezing", "blasted"),
            ("wet", "shocked"),
            ("melting", "tarred"),
        ];
        for (a, b) in expected {
            assert!(registry.status(id(a)).unwrap().affinities.contains(&id(b)));
            assert!(registry.status(id(b)).unwrap().affinities.contains(&id(a)));
        }
        assert!(
            registry
                .status(id("shocked"))
                .unwrap()
                .affinities
                .contains(&id("wet"))
        );
        assert!(registry.status(id("wet")).unwrap().affinities.len() == 1);
    }

    /// Statuses load in `StatusEffects.load()` order with those exact names.
    #[test]
    fn load_order_and_names() {
        let registry = test_registry();
        let names: Vec<&str> = registry
            .statuses()
            .iter()
            .map(|status| status.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "none",
                "burning",
                "freezing",
                "unmoving",
                "slow",
                "fast",
                "wet",
                "muddy",
                "melting",
                "sapped",
                "electrified",
                "spore-slowed",
                "tarred",
                "overdrive",
                "overclock",
                "shielded",
                "boss",
                "shocked",
                "blasted",
                "corroded",
                "disarmed",
                "invincible",
                "dynamic",
            ]
        );
        assert!(registry.statuses()[0].unlock.all_database_tabs);
    }
}
