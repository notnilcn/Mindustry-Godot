// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: generated `mindustry.gen.Sounds` field names (one per file in
//         `core/assets/sounds/`, plus the `none`/`unset` dummies).

//! Seed sound-name table (names only).
//!
//! Plan 18 owns `Sounds` registration and playback; this table is a
//! pre-registration seed with identical names so content metadata can reference
//! sounds by a stable id (same reconciliation posture as `fx_meta`, plan 02 R6).
//! Sounds are not a `ContentType`, so [`SoundId`] indexes this table rather
//! than a content vector. Regenerate with `parity/tools/gen_units.py`.

/// Number of seed sounds (`none` + `unset` + one per `core/assets/sounds/*.ogg`).
pub const SOUND_COUNT: usize = 207;

/// Minimal sound metadata: the generated `Sounds` field name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundMeta {
    /// Upstream `Sounds` field name (camelCase file name).
    pub name: &'static str,
    /// Asset path relative to `core/assets/sounds/` (without extension).
    pub asset: &'static str,
}

/// Stable index into [`SOUNDS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct SoundId(pub u16);

impl SoundId {
    /// Raw index.
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Metadata for this sound.
    pub fn meta(self) -> &'static SoundMeta {
        &SOUNDS[(self.0 as usize).min(SOUNDS.len() - 1)]
    }

    /// `Sounds.none`.
    pub const NONE: SoundId = SoundId(0);

    /// `Sounds.unset`.
    pub const UNSET: SoundId = SoundId(1);

    /// `Sounds.acceleratorCharge`.
    pub const ACCELERATOR_CHARGE: SoundId = SoundId(2);

    /// `Sounds.acceleratorConstruct`.
    pub const ACCELERATOR_CONSTRUCT: SoundId = SoundId(3);

    /// `Sounds.acceleratorLaunch`.
    pub const ACCELERATOR_LAUNCH: SoundId = SoundId(4);

    /// `Sounds.acceleratorLightning1`.
    pub const ACCELERATOR_LIGHTNING1: SoundId = SoundId(5);

    /// `Sounds.acceleratorLightning2`.
    pub const ACCELERATOR_LIGHTNING2: SoundId = SoundId(6);

    /// `Sounds.beamHeal`.
    pub const BEAM_HEAL: SoundId = SoundId(7);

    /// `Sounds.beamLustre`.
    pub const BEAM_LUSTRE: SoundId = SoundId(8);

    /// `Sounds.beamMeltdown`.
    pub const BEAM_MELTDOWN: SoundId = SoundId(9);

    /// `Sounds.beamParallax`.
    pub const BEAM_PARALLAX: SoundId = SoundId(10);

    /// `Sounds.beamPlasma`.
    pub const BEAM_PLASMA: SoundId = SoundId(11);

    /// `Sounds.beamPlasmaSmall`.
    pub const BEAM_PLASMA_SMALL: SoundId = SoundId(12);

    /// `Sounds.blockBreak1`.
    pub const BLOCK_BREAK1: SoundId = SoundId(13);

    /// `Sounds.blockBreak2`.
    pub const BLOCK_BREAK2: SoundId = SoundId(14);

    /// `Sounds.blockBreak3`.
    pub const BLOCK_BREAK3: SoundId = SoundId(15);

    /// `Sounds.blockExplode1`.
    pub const BLOCK_EXPLODE1: SoundId = SoundId(16);

    /// `Sounds.blockExplode1Alt`.
    pub const BLOCK_EXPLODE1_ALT: SoundId = SoundId(17);

    /// `Sounds.blockExplode2`.
    pub const BLOCK_EXPLODE2: SoundId = SoundId(18);

    /// `Sounds.blockExplode2Alt`.
    pub const BLOCK_EXPLODE2_ALT: SoundId = SoundId(19);

    /// `Sounds.blockExplode3`.
    pub const BLOCK_EXPLODE3: SoundId = SoundId(20);

    /// `Sounds.blockExplodeElectric`.
    pub const BLOCK_EXPLODE_ELECTRIC: SoundId = SoundId(21);

    /// `Sounds.blockExplodeElectricBig`.
    pub const BLOCK_EXPLODE_ELECTRIC_BIG: SoundId = SoundId(22);

    /// `Sounds.blockExplodeExplosive`.
    pub const BLOCK_EXPLODE_EXPLOSIVE: SoundId = SoundId(23);

    /// `Sounds.blockExplodeExplosiveAlt`.
    pub const BLOCK_EXPLODE_EXPLOSIVE_ALT: SoundId = SoundId(24);

    /// `Sounds.blockExplodeFlammable`.
    pub const BLOCK_EXPLODE_FLAMMABLE: SoundId = SoundId(25);

    /// `Sounds.blockExplodeWall`.
    pub const BLOCK_EXPLODE_WALL: SoundId = SoundId(26);

    /// `Sounds.blockHeal`.
    pub const BLOCK_HEAL: SoundId = SoundId(27);

    /// `Sounds.blockPlace1`.
    pub const BLOCK_PLACE1: SoundId = SoundId(28);

    /// `Sounds.blockPlace2`.
    pub const BLOCK_PLACE2: SoundId = SoundId(29);

    /// `Sounds.blockPlace3`.
    pub const BLOCK_PLACE3: SoundId = SoundId(30);

    /// `Sounds.blockRepair`.
    pub const BLOCK_REPAIR: SoundId = SoundId(31);

    /// `Sounds.blockRotate`.
    pub const BLOCK_ROTATE: SoundId = SoundId(32);

    /// `Sounds.chargeCorvus`.
    pub const CHARGE_CORVUS: SoundId = SoundId(33);

    /// `Sounds.chargeLancer`.
    pub const CHARGE_LANCER: SoundId = SoundId(34);

    /// `Sounds.chargeVela`.
    pub const CHARGE_VELA: SoundId = SoundId(35);

    /// `Sounds.click`.
    pub const CLICK: SoundId = SoundId(36);

    /// `Sounds.coreLand`.
    pub const CORE_LAND: SoundId = SoundId(37);

    /// `Sounds.coreLaunch`.
    pub const CORE_LAUNCH: SoundId = SoundId(38);

    /// `Sounds.door`.
    pub const DOOR: SoundId = SoundId(39);

    /// `Sounds.drillCharge`.
    pub const DRILL_CHARGE: SoundId = SoundId(40);

    /// `Sounds.drillImpact`.
    pub const DRILL_IMPACT: SoundId = SoundId(41);

    /// `Sounds.explosion`.
    pub const EXPLOSION: SoundId = SoundId(42);

    /// `Sounds.explosionAfflict`.
    pub const EXPLOSION_AFFLICT: SoundId = SoundId(43);

    /// `Sounds.explosionArtillery`.
    pub const EXPLOSION_ARTILLERY: SoundId = SoundId(44);

    /// `Sounds.explosionArtilleryShock`.
    pub const EXPLOSION_ARTILLERY_SHOCK: SoundId = SoundId(45);

    /// `Sounds.explosionArtilleryShockBig`.
    pub const EXPLOSION_ARTILLERY_SHOCK_BIG: SoundId = SoundId(46);

    /// `Sounds.explosionCleroi`.
    pub const EXPLOSION_CLEROI: SoundId = SoundId(47);

    /// `Sounds.explosionCore`.
    pub const EXPLOSION_CORE: SoundId = SoundId(48);

    /// `Sounds.explosionCrawler`.
    pub const EXPLOSION_CRAWLER: SoundId = SoundId(49);

    /// `Sounds.explosionDull`.
    pub const EXPLOSION_DULL: SoundId = SoundId(50);

    /// `Sounds.explosionMissile`.
    pub const EXPLOSION_MISSILE: SoundId = SoundId(51);

    /// `Sounds.explosionNavanax`.
    pub const EXPLOSION_NAVANAX: SoundId = SoundId(52);

    /// `Sounds.explosionObviate`.
    pub const EXPLOSION_OBVIATE: SoundId = SoundId(53);

    /// `Sounds.explosionPlasmaSmall`.
    pub const EXPLOSION_PLASMA_SMALL: SoundId = SoundId(54);

    /// `Sounds.explosionQuad`.
    pub const EXPLOSION_QUAD: SoundId = SoundId(55);

    /// `Sounds.explosionReactor`.
    pub const EXPLOSION_REACTOR: SoundId = SoundId(56);

    /// `Sounds.explosionReactor2`.
    pub const EXPLOSION_REACTOR2: SoundId = SoundId(57);

    /// `Sounds.explosionReactorNeoplasm`.
    pub const EXPLOSION_REACTOR_NEOPLASM: SoundId = SoundId(58);

    /// `Sounds.explosionTitan`.
    pub const EXPLOSION_TITAN: SoundId = SoundId(59);

    /// `Sounds.healWave`.
    pub const HEAL_WAVE: SoundId = SoundId(60);

    /// `Sounds.loopBio`.
    pub const LOOP_BIO: SoundId = SoundId(61);

    /// `Sounds.loopBuild`.
    pub const LOOP_BUILD: SoundId = SoundId(62);

    /// `Sounds.loopCircuit`.
    pub const LOOP_CIRCUIT: SoundId = SoundId(63);

    /// `Sounds.loopCombustion`.
    pub const LOOP_COMBUSTION: SoundId = SoundId(64);

    /// `Sounds.loopConveyor`.
    pub const LOOP_CONVEYOR: SoundId = SoundId(65);

    /// `Sounds.loopCultivator`.
    pub const LOOP_CULTIVATOR: SoundId = SoundId(66);

    /// `Sounds.loopCutter`.
    pub const LOOP_CUTTER: SoundId = SoundId(67);

    /// `Sounds.loopDifferential`.
    pub const LOOP_DIFFERENTIAL: SoundId = SoundId(68);

    /// `Sounds.loopDrill`.
    pub const LOOP_DRILL: SoundId = SoundId(69);

    /// `Sounds.loopElectricHum`.
    pub const LOOP_ELECTRIC_HUM: SoundId = SoundId(70);

    /// `Sounds.loopExtract`.
    pub const LOOP_EXTRACT: SoundId = SoundId(71);

    /// `Sounds.loopFire`.
    pub const LOOP_FIRE: SoundId = SoundId(72);

    /// `Sounds.loopFlux`.
    pub const LOOP_FLUX: SoundId = SoundId(73);

    /// `Sounds.loopGlow`.
    pub const LOOP_GLOW: SoundId = SoundId(74);

    /// `Sounds.loopGrind`.
    pub const LOOP_GRIND: SoundId = SoundId(75);

    /// `Sounds.loopHover`.
    pub const LOOP_HOVER: SoundId = SoundId(76);

    /// `Sounds.loopHover2`.
    pub const LOOP_HOVER2: SoundId = SoundId(77);

    /// `Sounds.loopHum`.
    pub const LOOP_HUM: SoundId = SoundId(78);

    /// `Sounds.loopMachine`.
    pub const LOOP_MACHINE: SoundId = SoundId(79);

    /// `Sounds.loopMachine2`.
    pub const LOOP_MACHINE2: SoundId = SoundId(80);

    /// `Sounds.loopMachineSpin`.
    pub const LOOP_MACHINE_SPIN: SoundId = SoundId(81);

    /// `Sounds.loopMalign`.
    pub const LOOP_MALIGN: SoundId = SoundId(82);

    /// `Sounds.loopMineBeam`.
    pub const LOOP_MINE_BEAM: SoundId = SoundId(83);

    /// `Sounds.loopMissileTrail`.
    pub const LOOP_MISSILE_TRAIL: SoundId = SoundId(84);

    /// `Sounds.loopPulse`.
    pub const LOOP_PULSE: SoundId = SoundId(85);

    /// `Sounds.loopRegen`.
    pub const LOOP_REGEN: SoundId = SoundId(86);

    /// `Sounds.loopShield`.
    pub const LOOP_SHIELD: SoundId = SoundId(87);

    /// `Sounds.loopSmelter`.
    pub const LOOP_SMELTER: SoundId = SoundId(88);

    /// `Sounds.loopSpray`.
    pub const LOOP_SPRAY: SoundId = SoundId(89);

    /// `Sounds.loopSteam`.
    pub const LOOP_STEAM: SoundId = SoundId(90);

    /// `Sounds.loopTech`.
    pub const LOOP_TECH: SoundId = SoundId(91);

    /// `Sounds.loopThoriumReactor`.
    pub const LOOP_THORIUM_REACTOR: SoundId = SoundId(92);

    /// `Sounds.loopThruster`.
    pub const LOOP_THRUSTER: SoundId = SoundId(93);

    /// `Sounds.loopUnitBuilding`.
    pub const LOOP_UNIT_BUILDING: SoundId = SoundId(94);

    /// `Sounds.massdriver`.
    pub const MASSDRIVER: SoundId = SoundId(95);

    /// `Sounds.massdriverReceive`.
    pub const MASSDRIVER_RECEIVE: SoundId = SoundId(96);

    /// `Sounds.mechStep`.
    pub const MECH_STEP: SoundId = SoundId(97);

    /// `Sounds.mechStepHeavy`.
    pub const MECH_STEP_HEAVY: SoundId = SoundId(98);

    /// `Sounds.mechStepSmall`.
    pub const MECH_STEP_SMALL: SoundId = SoundId(99);

    /// `Sounds.padLand`.
    pub const PAD_LAND: SoundId = SoundId(100);

    /// `Sounds.padLaunch`.
    pub const PAD_LAUNCH: SoundId = SoundId(101);

    /// `Sounds.payloadDrop1`.
    pub const PAYLOAD_DROP1: SoundId = SoundId(102);

    /// `Sounds.payloadDrop2`.
    pub const PAYLOAD_DROP2: SoundId = SoundId(103);

    /// `Sounds.payloadDrop3`.
    pub const PAYLOAD_DROP3: SoundId = SoundId(104);

    /// `Sounds.payloadPickup`.
    pub const PAYLOAD_PICKUP: SoundId = SoundId(105);

    /// `Sounds.plantBreak`.
    pub const PLANT_BREAK: SoundId = SoundId(106);

    /// `Sounds.rain`.
    pub const RAIN: SoundId = SoundId(107);

    /// `Sounds.rockBreak`.
    pub const ROCK_BREAK: SoundId = SoundId(108);

    /// `Sounds.shieldBreak`.
    pub const SHIELD_BREAK: SoundId = SoundId(109);

    /// `Sounds.shieldBreakSmall`.
    pub const SHIELD_BREAK_SMALL: SoundId = SoundId(110);

    /// `Sounds.shieldHit`.
    pub const SHIELD_HIT: SoundId = SoundId(111);

    /// `Sounds.shieldWave`.
    pub const SHIELD_WAVE: SoundId = SoundId(112);

    /// `Sounds.shipMove`.
    pub const SHIP_MOVE: SoundId = SoundId(113);

    /// `Sounds.shipMoveBig`.
    pub const SHIP_MOVE_BIG: SoundId = SoundId(114);

    /// `Sounds.shockBullet`.
    pub const SHOCK_BULLET: SoundId = SoundId(115);

    /// `Sounds.shockwaveTower`.
    pub const SHOCKWAVE_TOWER: SoundId = SoundId(116);

    /// `Sounds.shoot`.
    pub const SHOOT: SoundId = SoundId(117);

    /// `Sounds.shootAfflict`.
    pub const SHOOT_AFFLICT: SoundId = SoundId(118);

    /// `Sounds.shootAlpha`.
    pub const SHOOT_ALPHA: SoundId = SoundId(119);

    /// `Sounds.shootArc`.
    pub const SHOOT_ARC: SoundId = SoundId(120);

    /// `Sounds.shootArtillery`.
    pub const SHOOT_ARTILLERY: SoundId = SoundId(121);

    /// `Sounds.shootArtillerySap`.
    pub const SHOOT_ARTILLERY_SAP: SoundId = SoundId(122);

    /// `Sounds.shootArtillerySapBig`.
    pub const SHOOT_ARTILLERY_SAP_BIG: SoundId = SoundId(123);

    /// `Sounds.shootArtillerySmall`.
    pub const SHOOT_ARTILLERY_SMALL: SoundId = SoundId(124);

    /// `Sounds.shootAtrax`.
    pub const SHOOT_ATRAX: SoundId = SoundId(125);

    /// `Sounds.shootAvert`.
    pub const SHOOT_AVERT: SoundId = SoundId(126);

    /// `Sounds.shootBeamPlasma`.
    pub const SHOOT_BEAM_PLASMA: SoundId = SoundId(127);

    /// `Sounds.shootBeamPlasmaSmall`.
    pub const SHOOT_BEAM_PLASMA_SMALL: SoundId = SoundId(128);

    /// `Sounds.shootBreach`.
    pub const SHOOT_BREACH: SoundId = SoundId(129);

    /// `Sounds.shootBreachCarbide`.
    pub const SHOOT_BREACH_CARBIDE: SoundId = SoundId(130);

    /// `Sounds.shootCleroi`.
    pub const SHOOT_CLEROI: SoundId = SoundId(131);

    /// `Sounds.shootCollaris`.
    pub const SHOOT_COLLARIS: SoundId = SoundId(132);

    /// `Sounds.shootConquer`.
    pub const SHOOT_CONQUER: SoundId = SoundId(133);

    /// `Sounds.shootCorvus`.
    pub const SHOOT_CORVUS: SoundId = SoundId(134);

    /// `Sounds.shootCyclone`.
    pub const SHOOT_CYCLONE: SoundId = SoundId(135);

    /// `Sounds.shootDiffuse`.
    pub const SHOOT_DIFFUSE: SoundId = SoundId(136);

    /// `Sounds.shootDisperse`.
    pub const SHOOT_DISPERSE: SoundId = SoundId(137);

    /// `Sounds.shootDuo`.
    pub const SHOOT_DUO: SoundId = SoundId(138);

    /// `Sounds.shootEclipse`.
    pub const SHOOT_ECLIPSE: SoundId = SoundId(139);

    /// `Sounds.shootElude`.
    pub const SHOOT_ELUDE: SoundId = SoundId(140);

    /// `Sounds.shootEnergyField`.
    pub const SHOOT_ENERGY_FIELD: SoundId = SoundId(141);

    /// `Sounds.shootFlame`.
    pub const SHOOT_FLAME: SoundId = SoundId(142);

    /// `Sounds.shootFlamePlasma`.
    pub const SHOOT_FLAME_PLASMA: SoundId = SoundId(143);

    /// `Sounds.shootForeshadow`.
    pub const SHOOT_FORESHADOW: SoundId = SoundId(144);

    /// `Sounds.shootFuse`.
    pub const SHOOT_FUSE: SoundId = SoundId(145);

    /// `Sounds.shootHorizon`.
    pub const SHOOT_HORIZON: SoundId = SoundId(146);

    /// `Sounds.shootLancer`.
    pub const SHOOT_LANCER: SoundId = SoundId(147);

    /// `Sounds.shootLaser`.
    pub const SHOOT_LASER: SoundId = SoundId(148);

    /// `Sounds.shootLocus`.
    pub const SHOOT_LOCUS: SoundId = SoundId(149);

    /// `Sounds.shootMalign`.
    pub const SHOOT_MALIGN: SoundId = SoundId(150);

    /// `Sounds.shootMeltdown`.
    pub const SHOOT_MELTDOWN: SoundId = SoundId(151);

    /// `Sounds.shootMerui`.
    pub const SHOOT_MERUI: SoundId = SoundId(152);

    /// `Sounds.shootMissile`.
    pub const SHOOT_MISSILE: SoundId = SoundId(153);

    /// `Sounds.shootMissileLarge`.
    pub const SHOOT_MISSILE_LARGE: SoundId = SoundId(154);

    /// `Sounds.shootMissileLong`.
    pub const SHOOT_MISSILE_LONG: SoundId = SoundId(155);

    /// `Sounds.shootMissilePlasma`.
    pub const SHOOT_MISSILE_PLASMA: SoundId = SoundId(156);

    /// `Sounds.shootMissilePlasmaShort`.
    pub const SHOOT_MISSILE_PLASMA_SHORT: SoundId = SoundId(157);

    /// `Sounds.shootMissileShort`.
    pub const SHOOT_MISSILE_SHORT: SoundId = SoundId(158);

    /// `Sounds.shootMissileSmall`.
    pub const SHOOT_MISSILE_SMALL: SoundId = SoundId(159);

    /// `Sounds.shootNavanax`.
    pub const SHOOT_NAVANAX: SoundId = SoundId(160);

    /// `Sounds.shootOmura`.
    pub const SHOOT_OMURA: SoundId = SoundId(161);

    /// `Sounds.shootPayload`.
    pub const SHOOT_PAYLOAD: SoundId = SoundId(162);

    /// `Sounds.shootPulsar`.
    pub const SHOOT_PULSAR: SoundId = SoundId(163);

    /// `Sounds.shootQuad`.
    pub const SHOOT_QUAD: SoundId = SoundId(164);

    /// `Sounds.shootReign`.
    pub const SHOOT_REIGN: SoundId = SoundId(165);

    /// `Sounds.shootRetusa`.
    pub const SHOOT_RETUSA: SoundId = SoundId(166);

    /// `Sounds.shootRipple`.
    pub const SHOOT_RIPPLE: SoundId = SoundId(167);

    /// `Sounds.shootSalvo`.
    pub const SHOOT_SALVO: SoundId = SoundId(168);

    /// `Sounds.shootSap`.
    pub const SHOOT_SAP: SoundId = SoundId(169);

    /// `Sounds.shootScathe`.
    pub const SHOOT_SCATHE: SoundId = SoundId(170);

    /// `Sounds.shootScatter`.
    pub const SHOOT_SCATTER: SoundId = SoundId(171);

    /// `Sounds.shootScepter`.
    pub const SHOOT_SCEPTER: SoundId = SoundId(172);

    /// `Sounds.shootScepterSecondary`.
    pub const SHOOT_SCEPTER_SECONDARY: SoundId = SoundId(173);

    /// `Sounds.shootSegment`.
    pub const SHOOT_SEGMENT: SoundId = SoundId(174);

    /// `Sounds.shootSmite`.
    pub const SHOOT_SMITE: SoundId = SoundId(175);

    /// `Sounds.shootSpectre`.
    pub const SHOOT_SPECTRE: SoundId = SoundId(176);

    /// `Sounds.shootStell`.
    pub const SHOOT_STELL: SoundId = SoundId(177);

    /// `Sounds.shootSublimate`.
    pub const SHOOT_SUBLIMATE: SoundId = SoundId(178);

    /// `Sounds.shootTank`.
    pub const SHOOT_TANK: SoundId = SoundId(179);

    /// `Sounds.shootToxopidShotgun`.
    pub const SHOOT_TOXOPID_SHOTGUN: SoundId = SoundId(180);

    /// `Sounds.stepMud`.
    pub const STEP_MUD: SoundId = SoundId(181);

    /// `Sounds.stepWater`.
    pub const STEP_WATER: SoundId = SoundId(182);

    /// `Sounds.tankMove`.
    pub const TANK_MOVE: SoundId = SoundId(183);

    /// `Sounds.tankMoveHeavy`.
    pub const TANK_MOVE_HEAVY: SoundId = SoundId(184);

    /// `Sounds.tankMoveSmall`.
    pub const TANK_MOVE_SMALL: SoundId = SoundId(185);

    /// `Sounds.uiBack`.
    pub const UI_BACK: SoundId = SoundId(186);

    /// `Sounds.uiButton`.
    pub const UI_BUTTON: SoundId = SoundId(187);

    /// `Sounds.uiChat`.
    pub const UI_CHAT: SoundId = SoundId(188);

    /// `Sounds.uiFavorite`.
    pub const UI_FAVORITE: SoundId = SoundId(189);

    /// `Sounds.uiNotify`.
    pub const UI_NOTIFY: SoundId = SoundId(190);

    /// `Sounds.uiUnlock`.
    pub const UI_UNLOCK: SoundId = SoundId(191);

    /// `Sounds.unitCreate`.
    pub const UNIT_CREATE: SoundId = SoundId(192);

    /// `Sounds.unitCreateBig`.
    pub const UNIT_CREATE_BIG: SoundId = SoundId(193);

    /// `Sounds.unitExplode1`.
    pub const UNIT_EXPLODE1: SoundId = SoundId(194);

    /// `Sounds.unitExplode2`.
    pub const UNIT_EXPLODE2: SoundId = SoundId(195);

    /// `Sounds.unitExplode3`.
    pub const UNIT_EXPLODE3: SoundId = SoundId(196);

    /// `Sounds.walkerStep`.
    pub const WALKER_STEP: SoundId = SoundId(197);

    /// `Sounds.walkerStepSmall`.
    pub const WALKER_STEP_SMALL: SoundId = SoundId(198);

    /// `Sounds.walkerStepTiny`.
    pub const WALKER_STEP_TINY: SoundId = SoundId(199);

    /// `Sounds.waveSpawn`.
    pub const WAVE_SPAWN: SoundId = SoundId(200);

    /// `Sounds.wind`.
    pub const WIND: SoundId = SoundId(201);

    /// `Sounds.wind2`.
    pub const WIND2: SoundId = SoundId(202);

    /// `Sounds.wind3`.
    pub const WIND3: SoundId = SoundId(203);

    /// `Sounds.windHowl`.
    pub const WIND_HOWL: SoundId = SoundId(204);

    /// `Sounds.wreckFall`.
    pub const WRECK_FALL: SoundId = SoundId(205);

    /// `Sounds.wreckFallBig`.
    pub const WRECK_FALL_BIG: SoundId = SoundId(206);

    /// Looks up a sound by its upstream field name.
    pub fn by_name(name: &str) -> Option<SoundId> {
        SOUNDS
            .iter()
            .position(|meta| meta.name == name)
            .map(|index| SoundId(index as u16))
    }
}

/// Seed sound table (`none`, `unset`, then `core/assets/sounds/**/*.ogg` in
/// sorted name order). Regenerate with `parity/tools/gen_units.py`.
pub static SOUNDS: &[SoundMeta] = &[
    SoundMeta {
        name: "none",
        asset: "",
    },
    SoundMeta {
        name: "unset",
        asset: "",
    },
    SoundMeta {
        name: "acceleratorCharge",
        asset: "block/acceleratorCharge",
    },
    SoundMeta {
        name: "acceleratorConstruct",
        asset: "block/acceleratorConstruct",
    },
    SoundMeta {
        name: "acceleratorLaunch",
        asset: "block/acceleratorLaunch",
    },
    SoundMeta {
        name: "acceleratorLightning1",
        asset: "block/acceleratorLightning1",
    },
    SoundMeta {
        name: "acceleratorLightning2",
        asset: "block/acceleratorLightning2",
    },
    SoundMeta {
        name: "beamHeal",
        asset: "beams/beamHeal",
    },
    SoundMeta {
        name: "beamLustre",
        asset: "beams/beamLustre",
    },
    SoundMeta {
        name: "beamMeltdown",
        asset: "beams/beamMeltdown",
    },
    SoundMeta {
        name: "beamParallax",
        asset: "beams/beamParallax",
    },
    SoundMeta {
        name: "beamPlasma",
        asset: "beams/beamPlasma",
    },
    SoundMeta {
        name: "beamPlasmaSmall",
        asset: "beams/beamPlasmaSmall",
    },
    SoundMeta {
        name: "blockBreak1",
        asset: "block/blockBreak1",
    },
    SoundMeta {
        name: "blockBreak2",
        asset: "block/blockBreak2",
    },
    SoundMeta {
        name: "blockBreak3",
        asset: "block/blockBreak3",
    },
    SoundMeta {
        name: "blockExplode1",
        asset: "explosions/blockExplode1",
    },
    SoundMeta {
        name: "blockExplode1Alt",
        asset: "explosions/blockExplode1Alt",
    },
    SoundMeta {
        name: "blockExplode2",
        asset: "explosions/blockExplode2",
    },
    SoundMeta {
        name: "blockExplode2Alt",
        asset: "explosions/blockExplode2Alt",
    },
    SoundMeta {
        name: "blockExplode3",
        asset: "explosions/blockExplode3",
    },
    SoundMeta {
        name: "blockExplodeElectric",
        asset: "explosions/blockExplodeElectric",
    },
    SoundMeta {
        name: "blockExplodeElectricBig",
        asset: "explosions/blockExplodeElectricBig",
    },
    SoundMeta {
        name: "blockExplodeExplosive",
        asset: "explosions/blockExplodeExplosive",
    },
    SoundMeta {
        name: "blockExplodeExplosiveAlt",
        asset: "explosions/blockExplodeExplosiveAlt",
    },
    SoundMeta {
        name: "blockExplodeFlammable",
        asset: "explosions/blockExplodeFlammable",
    },
    SoundMeta {
        name: "blockExplodeWall",
        asset: "explosions/blockExplodeWall",
    },
    SoundMeta {
        name: "blockHeal",
        asset: "block/blockHeal",
    },
    SoundMeta {
        name: "blockPlace1",
        asset: "block/blockPlace1",
    },
    SoundMeta {
        name: "blockPlace2",
        asset: "block/blockPlace2",
    },
    SoundMeta {
        name: "blockPlace3",
        asset: "block/blockPlace3",
    },
    SoundMeta {
        name: "blockRepair",
        asset: "block/blockRepair",
    },
    SoundMeta {
        name: "blockRotate",
        asset: "block/blockRotate",
    },
    SoundMeta {
        name: "chargeCorvus",
        asset: "charge/chargeCorvus",
    },
    SoundMeta {
        name: "chargeLancer",
        asset: "charge/chargeLancer",
    },
    SoundMeta {
        name: "chargeVela",
        asset: "charge/chargeVela",
    },
    SoundMeta {
        name: "click",
        asset: "block/click",
    },
    SoundMeta {
        name: "coreLand",
        asset: "block/coreLand",
    },
    SoundMeta {
        name: "coreLaunch",
        asset: "block/coreLaunch",
    },
    SoundMeta {
        name: "door",
        asset: "block/door",
    },
    SoundMeta {
        name: "drillCharge",
        asset: "block/drillCharge",
    },
    SoundMeta {
        name: "drillImpact",
        asset: "block/drillImpact",
    },
    SoundMeta {
        name: "explosion",
        asset: "explosions/explosion",
    },
    SoundMeta {
        name: "explosionAfflict",
        asset: "explosions/explosionAfflict",
    },
    SoundMeta {
        name: "explosionArtillery",
        asset: "explosions/explosionArtillery",
    },
    SoundMeta {
        name: "explosionArtilleryShock",
        asset: "explosions/explosionArtilleryShock",
    },
    SoundMeta {
        name: "explosionArtilleryShockBig",
        asset: "explosions/explosionArtilleryShockBig",
    },
    SoundMeta {
        name: "explosionCleroi",
        asset: "explosions/explosionCleroi",
    },
    SoundMeta {
        name: "explosionCore",
        asset: "explosions/explosionCore",
    },
    SoundMeta {
        name: "explosionCrawler",
        asset: "explosions/explosionCrawler",
    },
    SoundMeta {
        name: "explosionDull",
        asset: "explosions/explosionDull",
    },
    SoundMeta {
        name: "explosionMissile",
        asset: "explosions/explosionMissile",
    },
    SoundMeta {
        name: "explosionNavanax",
        asset: "explosions/explosionNavanax",
    },
    SoundMeta {
        name: "explosionObviate",
        asset: "explosions/explosionObviate",
    },
    SoundMeta {
        name: "explosionPlasmaSmall",
        asset: "explosions/explosionPlasmaSmall",
    },
    SoundMeta {
        name: "explosionQuad",
        asset: "explosions/explosionQuad",
    },
    SoundMeta {
        name: "explosionReactor",
        asset: "explosions/explosionReactor",
    },
    SoundMeta {
        name: "explosionReactor2",
        asset: "explosions/explosionReactor2",
    },
    SoundMeta {
        name: "explosionReactorNeoplasm",
        asset: "explosions/explosionReactorNeoplasm",
    },
    SoundMeta {
        name: "explosionTitan",
        asset: "explosions/explosionTitan",
    },
    SoundMeta {
        name: "healWave",
        asset: "block/healWave",
    },
    SoundMeta {
        name: "loopBio",
        asset: "loops/loopBio",
    },
    SoundMeta {
        name: "loopBuild",
        asset: "loops/loopBuild",
    },
    SoundMeta {
        name: "loopCircuit",
        asset: "loops/loopCircuit",
    },
    SoundMeta {
        name: "loopCombustion",
        asset: "loops/loopCombustion",
    },
    SoundMeta {
        name: "loopConveyor",
        asset: "loops/loopConveyor",
    },
    SoundMeta {
        name: "loopCultivator",
        asset: "loops/loopCultivator",
    },
    SoundMeta {
        name: "loopCutter",
        asset: "loops/loopCutter",
    },
    SoundMeta {
        name: "loopDifferential",
        asset: "loops/loopDifferential",
    },
    SoundMeta {
        name: "loopDrill",
        asset: "loops/loopDrill",
    },
    SoundMeta {
        name: "loopElectricHum",
        asset: "loops/loopElectricHum",
    },
    SoundMeta {
        name: "loopExtract",
        asset: "loops/loopExtract",
    },
    SoundMeta {
        name: "loopFire",
        asset: "loops/loopFire",
    },
    SoundMeta {
        name: "loopFlux",
        asset: "loops/loopFlux",
    },
    SoundMeta {
        name: "loopGlow",
        asset: "loops/loopGlow",
    },
    SoundMeta {
        name: "loopGrind",
        asset: "loops/loopGrind",
    },
    SoundMeta {
        name: "loopHover",
        asset: "loops/loopHover",
    },
    SoundMeta {
        name: "loopHover2",
        asset: "loops/loopHover2",
    },
    SoundMeta {
        name: "loopHum",
        asset: "loops/loopHum",
    },
    SoundMeta {
        name: "loopMachine",
        asset: "loops/loopMachine",
    },
    SoundMeta {
        name: "loopMachine2",
        asset: "loops/loopMachine2",
    },
    SoundMeta {
        name: "loopMachineSpin",
        asset: "loops/loopMachineSpin",
    },
    SoundMeta {
        name: "loopMalign",
        asset: "loops/loopMalign",
    },
    SoundMeta {
        name: "loopMineBeam",
        asset: "loops/loopMineBeam",
    },
    SoundMeta {
        name: "loopMissileTrail",
        asset: "loops/loopMissileTrail",
    },
    SoundMeta {
        name: "loopPulse",
        asset: "loops/loopPulse",
    },
    SoundMeta {
        name: "loopRegen",
        asset: "loops/loopRegen",
    },
    SoundMeta {
        name: "loopShield",
        asset: "loops/loopShield",
    },
    SoundMeta {
        name: "loopSmelter",
        asset: "loops/loopSmelter",
    },
    SoundMeta {
        name: "loopSpray",
        asset: "loops/loopSpray",
    },
    SoundMeta {
        name: "loopSteam",
        asset: "loops/loopSteam",
    },
    SoundMeta {
        name: "loopTech",
        asset: "loops/loopTech",
    },
    SoundMeta {
        name: "loopThoriumReactor",
        asset: "loops/loopThoriumReactor",
    },
    SoundMeta {
        name: "loopThruster",
        asset: "loops/loopThruster",
    },
    SoundMeta {
        name: "loopUnitBuilding",
        asset: "loops/loopUnitBuilding",
    },
    SoundMeta {
        name: "massdriver",
        asset: "block/massdriver",
    },
    SoundMeta {
        name: "massdriverReceive",
        asset: "block/massdriverReceive",
    },
    SoundMeta {
        name: "mechStep",
        asset: "movement/mechStep",
    },
    SoundMeta {
        name: "mechStepHeavy",
        asset: "movement/mechStepHeavy",
    },
    SoundMeta {
        name: "mechStepSmall",
        asset: "movement/mechStepSmall",
    },
    SoundMeta {
        name: "padLand",
        asset: "block/padLand",
    },
    SoundMeta {
        name: "padLaunch",
        asset: "block/padLaunch",
    },
    SoundMeta {
        name: "payloadDrop1",
        asset: "block/payloadDrop1",
    },
    SoundMeta {
        name: "payloadDrop2",
        asset: "block/payloadDrop2",
    },
    SoundMeta {
        name: "payloadDrop3",
        asset: "block/payloadDrop3",
    },
    SoundMeta {
        name: "payloadPickup",
        asset: "block/payloadPickup",
    },
    SoundMeta {
        name: "plantBreak",
        asset: "block/plantBreak",
    },
    SoundMeta {
        name: "rain",
        asset: "environment/rain",
    },
    SoundMeta {
        name: "rockBreak",
        asset: "block/rockBreak",
    },
    SoundMeta {
        name: "shieldBreak",
        asset: "block/shieldBreak",
    },
    SoundMeta {
        name: "shieldBreakSmall",
        asset: "block/shieldBreakSmall",
    },
    SoundMeta {
        name: "shieldHit",
        asset: "block/shieldHit",
    },
    SoundMeta {
        name: "shieldWave",
        asset: "block/shieldWave",
    },
    SoundMeta {
        name: "shipMove",
        asset: "movement/shipMove",
    },
    SoundMeta {
        name: "shipMoveBig",
        asset: "movement/shipMoveBig",
    },
    SoundMeta {
        name: "shockBullet",
        asset: "explosions/shockBullet",
    },
    SoundMeta {
        name: "shockwaveTower",
        asset: "block/shockwaveTower",
    },
    SoundMeta {
        name: "shoot",
        asset: "shoot/shoot",
    },
    SoundMeta {
        name: "shootAfflict",
        asset: "shoot/shootAfflict",
    },
    SoundMeta {
        name: "shootAlpha",
        asset: "shoot/shootAlpha",
    },
    SoundMeta {
        name: "shootArc",
        asset: "shoot/shootArc",
    },
    SoundMeta {
        name: "shootArtillery",
        asset: "shoot/shootArtillery",
    },
    SoundMeta {
        name: "shootArtillerySap",
        asset: "shoot/shootArtillerySap",
    },
    SoundMeta {
        name: "shootArtillerySapBig",
        asset: "shoot/shootArtillerySapBig",
    },
    SoundMeta {
        name: "shootArtillerySmall",
        asset: "shoot/shootArtillerySmall",
    },
    SoundMeta {
        name: "shootAtrax",
        asset: "shoot/shootAtrax",
    },
    SoundMeta {
        name: "shootAvert",
        asset: "shoot/shootAvert",
    },
    SoundMeta {
        name: "shootBeamPlasma",
        asset: "shoot/shootBeamPlasma",
    },
    SoundMeta {
        name: "shootBeamPlasmaSmall",
        asset: "shoot/shootBeamPlasmaSmall",
    },
    SoundMeta {
        name: "shootBreach",
        asset: "shoot/shootBreach",
    },
    SoundMeta {
        name: "shootBreachCarbide",
        asset: "shoot/shootBreachCarbide",
    },
    SoundMeta {
        name: "shootCleroi",
        asset: "shoot/shootCleroi",
    },
    SoundMeta {
        name: "shootCollaris",
        asset: "shoot/shootCollaris",
    },
    SoundMeta {
        name: "shootConquer",
        asset: "shoot/shootConquer",
    },
    SoundMeta {
        name: "shootCorvus",
        asset: "shoot/shootCorvus",
    },
    SoundMeta {
        name: "shootCyclone",
        asset: "shoot/shootCyclone",
    },
    SoundMeta {
        name: "shootDiffuse",
        asset: "shoot/shootDiffuse",
    },
    SoundMeta {
        name: "shootDisperse",
        asset: "shoot/shootDisperse",
    },
    SoundMeta {
        name: "shootDuo",
        asset: "shoot/shootDuo",
    },
    SoundMeta {
        name: "shootEclipse",
        asset: "shoot/shootEclipse",
    },
    SoundMeta {
        name: "shootElude",
        asset: "shoot/shootElude",
    },
    SoundMeta {
        name: "shootEnergyField",
        asset: "shoot/shootEnergyField",
    },
    SoundMeta {
        name: "shootFlame",
        asset: "shoot/shootFlame",
    },
    SoundMeta {
        name: "shootFlamePlasma",
        asset: "shoot/shootFlamePlasma",
    },
    SoundMeta {
        name: "shootForeshadow",
        asset: "shoot/shootForeshadow",
    },
    SoundMeta {
        name: "shootFuse",
        asset: "shoot/shootFuse",
    },
    SoundMeta {
        name: "shootHorizon",
        asset: "shoot/shootHorizon",
    },
    SoundMeta {
        name: "shootLancer",
        asset: "shoot/shootLancer",
    },
    SoundMeta {
        name: "shootLaser",
        asset: "shoot/shootLaser",
    },
    SoundMeta {
        name: "shootLocus",
        asset: "shoot/shootLocus",
    },
    SoundMeta {
        name: "shootMalign",
        asset: "shoot/shootMalign",
    },
    SoundMeta {
        name: "shootMeltdown",
        asset: "shoot/shootMeltdown",
    },
    SoundMeta {
        name: "shootMerui",
        asset: "shoot/shootMerui",
    },
    SoundMeta {
        name: "shootMissile",
        asset: "shoot/shootMissile",
    },
    SoundMeta {
        name: "shootMissileLarge",
        asset: "shoot/shootMissileLarge",
    },
    SoundMeta {
        name: "shootMissileLong",
        asset: "shoot/shootMissileLong",
    },
    SoundMeta {
        name: "shootMissilePlasma",
        asset: "shoot/shootMissilePlasma",
    },
    SoundMeta {
        name: "shootMissilePlasmaShort",
        asset: "shoot/shootMissilePlasmaShort",
    },
    SoundMeta {
        name: "shootMissileShort",
        asset: "shoot/shootMissileShort",
    },
    SoundMeta {
        name: "shootMissileSmall",
        asset: "shoot/shootMissileSmall",
    },
    SoundMeta {
        name: "shootNavanax",
        asset: "shoot/shootNavanax",
    },
    SoundMeta {
        name: "shootOmura",
        asset: "shoot/shootOmura",
    },
    SoundMeta {
        name: "shootPayload",
        asset: "shoot/shootPayload",
    },
    SoundMeta {
        name: "shootPulsar",
        asset: "shoot/shootPulsar",
    },
    SoundMeta {
        name: "shootQuad",
        asset: "shoot/shootQuad",
    },
    SoundMeta {
        name: "shootReign",
        asset: "shoot/shootReign",
    },
    SoundMeta {
        name: "shootRetusa",
        asset: "shoot/shootRetusa",
    },
    SoundMeta {
        name: "shootRipple",
        asset: "shoot/shootRipple",
    },
    SoundMeta {
        name: "shootSalvo",
        asset: "shoot/shootSalvo",
    },
    SoundMeta {
        name: "shootSap",
        asset: "shoot/shootSap",
    },
    SoundMeta {
        name: "shootScathe",
        asset: "shoot/shootScathe",
    },
    SoundMeta {
        name: "shootScatter",
        asset: "shoot/shootScatter",
    },
    SoundMeta {
        name: "shootScepter",
        asset: "shoot/shootScepter",
    },
    SoundMeta {
        name: "shootScepterSecondary",
        asset: "shoot/shootScepterSecondary",
    },
    SoundMeta {
        name: "shootSegment",
        asset: "shoot/shootSegment",
    },
    SoundMeta {
        name: "shootSmite",
        asset: "shoot/shootSmite",
    },
    SoundMeta {
        name: "shootSpectre",
        asset: "shoot/shootSpectre",
    },
    SoundMeta {
        name: "shootStell",
        asset: "shoot/shootStell",
    },
    SoundMeta {
        name: "shootSublimate",
        asset: "shoot/shootSublimate",
    },
    SoundMeta {
        name: "shootTank",
        asset: "shoot/shootTank",
    },
    SoundMeta {
        name: "shootToxopidShotgun",
        asset: "shoot/shootToxopidShotgun",
    },
    SoundMeta {
        name: "stepMud",
        asset: "movement/stepMud",
    },
    SoundMeta {
        name: "stepWater",
        asset: "movement/stepWater",
    },
    SoundMeta {
        name: "tankMove",
        asset: "movement/tankMove",
    },
    SoundMeta {
        name: "tankMoveHeavy",
        asset: "movement/tankMoveHeavy",
    },
    SoundMeta {
        name: "tankMoveSmall",
        asset: "movement/tankMoveSmall",
    },
    SoundMeta {
        name: "uiBack",
        asset: "ui/uiBack",
    },
    SoundMeta {
        name: "uiButton",
        asset: "ui/uiButton",
    },
    SoundMeta {
        name: "uiChat",
        asset: "ui/uiChat",
    },
    SoundMeta {
        name: "uiFavorite",
        asset: "ui/uiFavorite",
    },
    SoundMeta {
        name: "uiNotify",
        asset: "ui/uiNotify",
    },
    SoundMeta {
        name: "uiUnlock",
        asset: "ui/uiUnlock",
    },
    SoundMeta {
        name: "unitCreate",
        asset: "block/unitCreate",
    },
    SoundMeta {
        name: "unitCreateBig",
        asset: "block/unitCreateBig",
    },
    SoundMeta {
        name: "unitExplode1",
        asset: "explosions/unitExplode1",
    },
    SoundMeta {
        name: "unitExplode2",
        asset: "explosions/unitExplode2",
    },
    SoundMeta {
        name: "unitExplode3",
        asset: "explosions/unitExplode3",
    },
    SoundMeta {
        name: "walkerStep",
        asset: "movement/walkerStep",
    },
    SoundMeta {
        name: "walkerStepSmall",
        asset: "movement/walkerStepSmall",
    },
    SoundMeta {
        name: "walkerStepTiny",
        asset: "movement/walkerStepTiny",
    },
    SoundMeta {
        name: "waveSpawn",
        asset: "ui/waveSpawn",
    },
    SoundMeta {
        name: "wind",
        asset: "environment/wind",
    },
    SoundMeta {
        name: "wind2",
        asset: "environment/wind2",
    },
    SoundMeta {
        name: "wind3",
        asset: "environment/wind3",
    },
    SoundMeta {
        name: "windHowl",
        asset: "environment/windHowl",
    },
    SoundMeta {
        name: "wreckFall",
        asset: "explosions/wreckFall",
    },
    SoundMeta {
        name: "wreckFallBig",
        asset: "explosions/wreckFallBig",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_consistent() {
        assert_eq!(SOUNDS.len(), SOUND_COUNT);
        assert_eq!(SOUNDS[SoundId::NONE.raw() as usize].name, "none");
        assert_eq!(SOUNDS[SoundId::UNSET.raw() as usize].name, "unset");
    }
}
