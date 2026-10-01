// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/Fx.java
//
//! Seed effect-name table (names + lifetimes only).
//!
//! Plan 17 owns the full `Fx` catalogue and effect bodies; this table is a
//! pre-registration seed with identical names, generated from the source once.
//! Effects are not a `ContentType` (the `effect_UNUSED` slot was never a live
//! ID space), so `EffectId` indexes this table rather than a content vector.

/// Number of seed effects (upstream `Fx` declarations): 267.
pub const EFFECT_COUNT: usize = 267;

/// Minimal effect metadata: name, lifetime (ticks) and clip radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectMeta {
    /// Upstream field name.
    pub name: &'static str,
    /// `Effect.lifetime` in ticks.
    pub lifetime: f32,
    /// `Effect.clip`, `0.0` when unset.
    pub clip: f32,
}

/// Stable index into [`EFFECTS`] (declaration order in `Fx.java`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EffectId(pub u16);

impl EffectId {
    /// Raw index.
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Metadata for this effect.
    pub fn meta(self) -> &'static EffectMeta {
        &EFFECTS[(self.0 as usize).min(EFFECTS.len() - 1)]
    }

    /// `Fx.none`.
    pub const NONE: EffectId = EffectId(0);

    /// `Fx.blockCrash`.
    pub const BLOCK_CRASH: EffectId = EffectId(1);

    /// `Fx.trailFade`.
    pub const TRAIL_FADE: EffectId = EffectId(2);

    /// `Fx.unitSpawn`.
    pub const UNIT_SPAWN: EffectId = EffectId(3);

    /// `Fx.unitCapKill`.
    pub const UNIT_CAP_KILL: EffectId = EffectId(4);

    /// `Fx.unitEnvKill`.
    pub const UNIT_ENV_KILL: EffectId = EffectId(5);

    /// `Fx.unitControl`.
    pub const UNIT_CONTROL: EffectId = EffectId(6);

    /// `Fx.unitDespawn`.
    pub const UNIT_DESPAWN: EffectId = EffectId(7);

    /// `Fx.unitSpirit`.
    pub const UNIT_SPIRIT: EffectId = EffectId(8);

    /// `Fx.itemTransfer`.
    pub const ITEM_TRANSFER: EffectId = EffectId(9);

    /// `Fx.pointBeam`.
    pub const POINT_BEAM: EffectId = EffectId(10);

    /// `Fx.pointHit`.
    pub const POINT_HIT: EffectId = EffectId(11);

    /// `Fx.hitScepterSecondary`.
    pub const HIT_SCEPTER_SECONDARY: EffectId = EffectId(12);

    /// `Fx.lightning`.
    pub const LIGHTNING: EffectId = EffectId(13);

    /// `Fx.coreBuildShockwave`.
    pub const CORE_BUILD_SHOCKWAVE: EffectId = EffectId(14);

    /// `Fx.coreBuildBlock`.
    pub const CORE_BUILD_BLOCK: EffectId = EffectId(15);

    /// `Fx.pointShockwave`.
    pub const POINT_SHOCKWAVE: EffectId = EffectId(16);

    /// `Fx.moveCommand`.
    pub const MOVE_COMMAND: EffectId = EffectId(17);

    /// `Fx.attackCommand`.
    pub const ATTACK_COMMAND: EffectId = EffectId(18);

    /// `Fx.commandSend`.
    pub const COMMAND_SEND: EffectId = EffectId(19);

    /// `Fx.upgradeCore`.
    pub const UPGRADE_CORE: EffectId = EffectId(20);

    /// `Fx.upgradeCoreBloom`.
    pub const UPGRADE_CORE_BLOOM: EffectId = EffectId(21);

    /// `Fx.placeBlock`.
    pub const PLACE_BLOCK: EffectId = EffectId(22);

    /// `Fx.coreLaunchConstruct`.
    pub const CORE_LAUNCH_CONSTRUCT: EffectId = EffectId(23);

    /// `Fx.tapBlock`.
    pub const TAP_BLOCK: EffectId = EffectId(24);

    /// `Fx.breakBlock`.
    pub const BREAK_BLOCK: EffectId = EffectId(25);

    /// `Fx.payloadDeposit`.
    pub const PAYLOAD_DEPOSIT: EffectId = EffectId(26);

    /// `Fx.select`.
    pub const SELECT: EffectId = EffectId(27);

    /// `Fx.smoke`.
    pub const SMOKE: EffectId = EffectId(28);

    /// `Fx.fallSmoke`.
    pub const FALL_SMOKE: EffectId = EffectId(29);

    /// `Fx.unitWreck`.
    pub const UNIT_WRECK: EffectId = EffectId(30);

    /// `Fx.rocketSmoke`.
    pub const ROCKET_SMOKE: EffectId = EffectId(31);

    /// `Fx.rocketSmokeLarge`.
    pub const ROCKET_SMOKE_LARGE: EffectId = EffectId(32);

    /// `Fx.magmasmoke`.
    pub const MAGMASMOKE: EffectId = EffectId(33);

    /// `Fx.spawn`.
    pub const SPAWN: EffectId = EffectId(34);

    /// `Fx.unitAssemble`.
    pub const UNIT_ASSEMBLE: EffectId = EffectId(35);

    /// `Fx.padlaunch`.
    pub const PADLAUNCH: EffectId = EffectId(36);

    /// `Fx.breakProp`.
    pub const BREAK_PROP: EffectId = EffectId(37);

    /// `Fx.unitDrop`.
    pub const UNIT_DROP: EffectId = EffectId(38);

    /// `Fx.unitLand`.
    pub const UNIT_LAND: EffectId = EffectId(39);

    /// `Fx.unitDust`.
    pub const UNIT_DUST: EffectId = EffectId(40);

    /// `Fx.unitLandSmall`.
    pub const UNIT_LAND_SMALL: EffectId = EffectId(41);

    /// `Fx.unitPickup`.
    pub const UNIT_PICKUP: EffectId = EffectId(42);

    /// `Fx.crawlDust`.
    pub const CRAWL_DUST: EffectId = EffectId(43);

    /// `Fx.landShock`.
    pub const LAND_SHOCK: EffectId = EffectId(44);

    /// `Fx.pickup`.
    pub const PICKUP: EffectId = EffectId(45);

    /// `Fx.sparkExplosion`.
    pub const SPARK_EXPLOSION: EffectId = EffectId(46);

    /// `Fx.titanExplosion`.
    pub const TITAN_EXPLOSION: EffectId = EffectId(47);

    /// `Fx.titanExplosionLarge`.
    pub const TITAN_EXPLOSION_LARGE: EffectId = EffectId(48);

    /// `Fx.titanExplosionSmall`.
    pub const TITAN_EXPLOSION_SMALL: EffectId = EffectId(49);

    /// `Fx.titanExplosionFrag`.
    pub const TITAN_EXPLOSION_FRAG: EffectId = EffectId(50);

    /// `Fx.titanSmoke`.
    pub const TITAN_SMOKE: EffectId = EffectId(51);

    /// `Fx.titanSmokeLarge`.
    pub const TITAN_SMOKE_LARGE: EffectId = EffectId(52);

    /// `Fx.titanSmokeSmall`.
    pub const TITAN_SMOKE_SMALL: EffectId = EffectId(53);

    /// `Fx.coreExplosion`.
    pub const CORE_EXPLOSION: EffectId = EffectId(54);

    /// `Fx.smokeAoeCloud`.
    pub const SMOKE_AOE_CLOUD: EffectId = EffectId(55);

    /// `Fx.missileTrailSmoke`.
    pub const MISSILE_TRAIL_SMOKE: EffectId = EffectId(56);

    /// `Fx.missileTrailSmokeSmall`.
    pub const MISSILE_TRAIL_SMOKE_SMALL: EffectId = EffectId(57);

    /// `Fx.neoplasmSplat`.
    pub const NEOPLASM_SPLAT: EffectId = EffectId(58);

    /// `Fx.scatheExplosion`.
    pub const SCATHE_EXPLOSION: EffectId = EffectId(59);

    /// `Fx.scatheExplosionSmall`.
    pub const SCATHE_EXPLOSION_SMALL: EffectId = EffectId(60);

    /// `Fx.scatheLight`.
    pub const SCATHE_LIGHT: EffectId = EffectId(61);

    /// `Fx.scatheLightSmall`.
    pub const SCATHE_LIGHT_SMALL: EffectId = EffectId(62);

    /// `Fx.titanLightSmall`.
    pub const TITAN_LIGHT_SMALL: EffectId = EffectId(63);

    /// `Fx.scatheSlash`.
    pub const SCATHE_SLASH: EffectId = EffectId(64);

    /// `Fx.dynamicSpikes`.
    pub const DYNAMIC_SPIKES: EffectId = EffectId(65);

    /// `Fx.greenBomb`.
    pub const GREEN_BOMB: EffectId = EffectId(66);

    /// `Fx.greenLaserCharge`.
    pub const GREEN_LASER_CHARGE: EffectId = EffectId(67);

    /// `Fx.greenLaserChargeSmall`.
    pub const GREEN_LASER_CHARGE_SMALL: EffectId = EffectId(68);

    /// `Fx.greenCloud`.
    pub const GREEN_CLOUD: EffectId = EffectId(69);

    /// `Fx.healWaveDynamic`.
    pub const HEAL_WAVE_DYNAMIC: EffectId = EffectId(70);

    /// `Fx.healWave`.
    pub const HEAL_WAVE: EffectId = EffectId(71);

    /// `Fx.heal`.
    pub const HEAL: EffectId = EffectId(72);

    /// `Fx.dynamicWave`.
    pub const DYNAMIC_WAVE: EffectId = EffectId(73);

    /// `Fx.shieldWave`.
    pub const SHIELD_WAVE: EffectId = EffectId(74);

    /// `Fx.shieldApply`.
    pub const SHIELD_APPLY: EffectId = EffectId(75);

    /// `Fx.disperseTrail`.
    pub const DISPERSE_TRAIL: EffectId = EffectId(76);

    /// `Fx.hitBulletSmall`.
    pub const HIT_BULLET_SMALL: EffectId = EffectId(77);

    /// `Fx.hitBulletColor`.
    pub const HIT_BULLET_COLOR: EffectId = EffectId(78);

    /// `Fx.hitSquaresColor`.
    pub const HIT_SQUARES_COLOR: EffectId = EffectId(79);

    /// `Fx.squareWaveEffect`.
    pub const SQUARE_WAVE_EFFECT: EffectId = EffectId(80);

    /// `Fx.hitFuse`.
    pub const HIT_FUSE: EffectId = EffectId(81);

    /// `Fx.hitBulletBig`.
    pub const HIT_BULLET_BIG: EffectId = EffectId(82);

    /// `Fx.hitFlameSmall`.
    pub const HIT_FLAME_SMALL: EffectId = EffectId(83);

    /// `Fx.hitFlamePlasma`.
    pub const HIT_FLAME_PLASMA: EffectId = EffectId(84);

    /// `Fx.hitLiquid`.
    pub const HIT_LIQUID: EffectId = EffectId(85);

    /// `Fx.hitLaserBlast`.
    pub const HIT_LASER_BLAST: EffectId = EffectId(86);

    /// `Fx.hitEmpSpark`.
    pub const HIT_EMP_SPARK: EffectId = EffectId(87);

    /// `Fx.hitLancer`.
    pub const HIT_LANCER: EffectId = EffectId(88);

    /// `Fx.hitLancerLow`.
    pub const HIT_LANCER_LOW: EffectId = EffectId(89);

    /// `Fx.hitBeam`.
    pub const HIT_BEAM: EffectId = EffectId(90);

    /// `Fx.hitFlameBeam`.
    pub const HIT_FLAME_BEAM: EffectId = EffectId(91);

    /// `Fx.hitMeltdown`.
    pub const HIT_MELTDOWN: EffectId = EffectId(92);

    /// `Fx.hitMeltHeal`.
    pub const HIT_MELT_HEAL: EffectId = EffectId(93);

    /// `Fx.instBomb`.
    pub const INST_BOMB: EffectId = EffectId(94);

    /// `Fx.instTrail`.
    pub const INST_TRAIL: EffectId = EffectId(95);

    /// `Fx.instShoot`.
    pub const INST_SHOOT: EffectId = EffectId(96);

    /// `Fx.instHit`.
    pub const INST_HIT: EffectId = EffectId(97);

    /// `Fx.hitLaser`.
    pub const HIT_LASER: EffectId = EffectId(98);

    /// `Fx.hitLaserColor`.
    pub const HIT_LASER_COLOR: EffectId = EffectId(99);

    /// `Fx.despawn`.
    pub const DESPAWN: EffectId = EffectId(100);

    /// `Fx.airBubble`.
    pub const AIR_BUBBLE: EffectId = EffectId(101);

    /// `Fx.flakExplosion`.
    pub const FLAK_EXPLOSION: EffectId = EffectId(102);

    /// `Fx.plasticExplosion`.
    pub const PLASTIC_EXPLOSION: EffectId = EffectId(103);

    /// `Fx.plasticExplosionFlak`.
    pub const PLASTIC_EXPLOSION_FLAK: EffectId = EffectId(104);

    /// `Fx.blastExplosion`.
    pub const BLAST_EXPLOSION: EffectId = EffectId(105);

    /// `Fx.sapExplosion`.
    pub const SAP_EXPLOSION: EffectId = EffectId(106);

    /// `Fx.massiveExplosion`.
    pub const MASSIVE_EXPLOSION: EffectId = EffectId(107);

    /// `Fx.artilleryTrail`.
    pub const ARTILLERY_TRAIL: EffectId = EffectId(108);

    /// `Fx.incendTrail`.
    pub const INCEND_TRAIL: EffectId = EffectId(109);

    /// `Fx.missileTrail`.
    pub const MISSILE_TRAIL: EffectId = EffectId(110);

    /// `Fx.missileTrailShort`.
    pub const MISSILE_TRAIL_SHORT: EffectId = EffectId(111);

    /// `Fx.bulletSparkSmokeTrailSmall`.
    pub const BULLET_SPARK_SMOKE_TRAIL_SMALL: EffectId = EffectId(112);

    /// `Fx.colorTrail`.
    pub const COLOR_TRAIL: EffectId = EffectId(113);

    /// `Fx.absorb`.
    pub const ABSORB: EffectId = EffectId(114);

    /// `Fx.forceShrink`.
    pub const FORCE_SHRINK: EffectId = EffectId(115);

    /// `Fx.flakExplosionBig`.
    pub const FLAK_EXPLOSION_BIG: EffectId = EffectId(116);

    /// `Fx.burning`.
    pub const BURNING: EffectId = EffectId(117);

    /// `Fx.fireRemove`.
    pub const FIRE_REMOVE: EffectId = EffectId(118);

    /// `Fx.fire`.
    pub const FIRE: EffectId = EffectId(119);

    /// `Fx.fireHit`.
    pub const FIRE_HIT: EffectId = EffectId(120);

    /// `Fx.fireSmoke`.
    pub const FIRE_SMOKE: EffectId = EffectId(121);

    /// `Fx.neoplasmHeal`.
    pub const NEOPLASM_HEAL: EffectId = EffectId(122);

    /// `Fx.steam`.
    pub const STEAM: EffectId = EffectId(123);

    /// `Fx.ventSteam`.
    pub const VENT_STEAM: EffectId = EffectId(124);

    /// `Fx.drillSteam`.
    pub const DRILL_STEAM: EffectId = EffectId(125);

    /// `Fx.fluxVapor`.
    pub const FLUX_VAPOR: EffectId = EffectId(126);

    /// `Fx.corrosionVapor`.
    pub const CORROSION_VAPOR: EffectId = EffectId(127);

    /// `Fx.vapor`.
    pub const VAPOR: EffectId = EffectId(128);

    /// `Fx.vaporSmall`.
    pub const VAPOR_SMALL: EffectId = EffectId(129);

    /// `Fx.fireballsmoke`.
    pub const FIREBALLSMOKE: EffectId = EffectId(130);

    /// `Fx.ballfire`.
    pub const BALLFIRE: EffectId = EffectId(131);

    /// `Fx.freezing`.
    pub const FREEZING: EffectId = EffectId(132);

    /// `Fx.melting`.
    pub const MELTING: EffectId = EffectId(133);

    /// `Fx.wet`.
    pub const WET: EffectId = EffectId(134);

    /// `Fx.muddy`.
    pub const MUDDY: EffectId = EffectId(135);

    /// `Fx.sapped`.
    pub const SAPPED: EffectId = EffectId(136);

    /// `Fx.electrified`.
    pub const ELECTRIFIED: EffectId = EffectId(137);

    /// `Fx.sporeSlowed`.
    pub const SPORE_SLOWED: EffectId = EffectId(138);

    /// `Fx.oily`.
    pub const OILY: EffectId = EffectId(139);

    /// `Fx.overdriven`.
    pub const OVERDRIVEN: EffectId = EffectId(140);

    /// `Fx.overclocked`.
    pub const OVERCLOCKED: EffectId = EffectId(141);

    /// `Fx.dropItem`.
    pub const DROP_ITEM: EffectId = EffectId(142);

    /// `Fx.shockwave`.
    pub const SHOCKWAVE: EffectId = EffectId(143);

    /// `Fx.shockwaveSmaller`.
    pub const SHOCKWAVE_SMALLER: EffectId = EffectId(144);

    /// `Fx.bigShockwave`.
    pub const BIG_SHOCKWAVE: EffectId = EffectId(145);

    /// `Fx.spawnShockwave`.
    pub const SPAWN_SHOCKWAVE: EffectId = EffectId(146);

    /// `Fx.podLandShockwave`.
    pub const POD_LAND_SHOCKWAVE: EffectId = EffectId(147);

    /// `Fx.explosion`.
    pub const EXPLOSION: EffectId = EffectId(148);

    /// `Fx.dynamicExplosion`.
    pub const DYNAMIC_EXPLOSION: EffectId = EffectId(149);

    /// `Fx.reactorExplosion`.
    pub const REACTOR_EXPLOSION: EffectId = EffectId(150);

    /// `Fx.impactReactorExplosion`.
    pub const IMPACT_REACTOR_EXPLOSION: EffectId = EffectId(151);

    /// `Fx.blockExplosionSmoke`.
    pub const BLOCK_EXPLOSION_SMOKE: EffectId = EffectId(152);

    /// `Fx.steamCoolSmoke`.
    pub const STEAM_COOL_SMOKE: EffectId = EffectId(153);

    /// `Fx.smokePuff`.
    pub const SMOKE_PUFF: EffectId = EffectId(154);

    /// `Fx.shootSmall`.
    pub const SHOOT_SMALL: EffectId = EffectId(155);

    /// `Fx.shootSmallColor`.
    pub const SHOOT_SMALL_COLOR: EffectId = EffectId(156);

    /// `Fx.shootHeal`.
    pub const SHOOT_HEAL: EffectId = EffectId(157);

    /// `Fx.shootHealYellow`.
    pub const SHOOT_HEAL_YELLOW: EffectId = EffectId(158);

    /// `Fx.shootSmallSmoke`.
    pub const SHOOT_SMALL_SMOKE: EffectId = EffectId(159);

    /// `Fx.shootBig`.
    pub const SHOOT_BIG: EffectId = EffectId(160);

    /// `Fx.shootBig2`.
    pub const SHOOT_BIG2: EffectId = EffectId(161);

    /// `Fx.shootBigColor`.
    pub const SHOOT_BIG_COLOR: EffectId = EffectId(162);

    /// `Fx.shootScepterSecondary`.
    pub const SHOOT_SCEPTER_SECONDARY: EffectId = EffectId(163);

    /// `Fx.shootQuellPulse`.
    pub const SHOOT_QUELL_PULSE: EffectId = EffectId(164);

    /// `Fx.shootTitan`.
    pub const SHOOT_TITAN: EffectId = EffectId(165);

    /// `Fx.shootBigSmoke`.
    pub const SHOOT_BIG_SMOKE: EffectId = EffectId(166);

    /// `Fx.shootBigSmoke2`.
    pub const SHOOT_BIG_SMOKE2: EffectId = EffectId(167);

    /// `Fx.shootSmokeDisperse`.
    pub const SHOOT_SMOKE_DISPERSE: EffectId = EffectId(168);

    /// `Fx.shootSmokeSquare`.
    pub const SHOOT_SMOKE_SQUARE: EffectId = EffectId(169);

    /// `Fx.shootSmokeSquareSparse`.
    pub const SHOOT_SMOKE_SQUARE_SPARSE: EffectId = EffectId(170);

    /// `Fx.shootSmokeSquareBig`.
    pub const SHOOT_SMOKE_SQUARE_BIG: EffectId = EffectId(171);

    /// `Fx.shootSmokeTitan`.
    pub const SHOOT_SMOKE_TITAN: EffectId = EffectId(172);

    /// `Fx.shootSmokeSmite`.
    pub const SHOOT_SMOKE_SMITE: EffectId = EffectId(173);

    /// `Fx.shootSmokeMissile`.
    pub const SHOOT_SMOKE_MISSILE: EffectId = EffectId(174);

    /// `Fx.shootSmokeMissileColor`.
    pub const SHOOT_SMOKE_MISSILE_COLOR: EffectId = EffectId(175);

    /// `Fx.regenParticle`.
    pub const REGEN_PARTICLE: EffectId = EffectId(176);

    /// `Fx.regenSuppressParticle`.
    pub const REGEN_SUPPRESS_PARTICLE: EffectId = EffectId(177);

    /// `Fx.regenSuppressSeek`.
    pub const REGEN_SUPPRESS_SEEK: EffectId = EffectId(178);

    /// `Fx.surgeCruciSmoke`.
    pub const SURGE_CRUCI_SMOKE: EffectId = EffectId(179);

    /// `Fx.neoplasiaSmoke`.
    pub const NEOPLASIA_SMOKE: EffectId = EffectId(180);

    /// `Fx.heatReactorSmoke`.
    pub const HEAT_REACTOR_SMOKE: EffectId = EffectId(181);

    /// `Fx.circleColorSpark`.
    pub const CIRCLE_COLOR_SPARK: EffectId = EffectId(182);

    /// `Fx.colorSpark`.
    pub const COLOR_SPARK: EffectId = EffectId(183);

    /// `Fx.colorSparkBig`.
    pub const COLOR_SPARK_BIG: EffectId = EffectId(184);

    /// `Fx.randLifeSpark`.
    pub const RAND_LIFE_SPARK: EffectId = EffectId(185);

    /// `Fx.shootPayloadDriver`.
    pub const SHOOT_PAYLOAD_DRIVER: EffectId = EffectId(186);

    /// `Fx.shootSmallFlame`.
    pub const SHOOT_SMALL_FLAME: EffectId = EffectId(187);

    /// `Fx.shootPyraFlame`.
    pub const SHOOT_PYRA_FLAME: EffectId = EffectId(188);

    /// `Fx.shootLiquid`.
    pub const SHOOT_LIQUID: EffectId = EffectId(189);

    /// `Fx.casing1`.
    pub const CASING1: EffectId = EffectId(190);

    /// `Fx.casing2`.
    pub const CASING2: EffectId = EffectId(191);

    /// `Fx.casing3`.
    pub const CASING3: EffectId = EffectId(192);

    /// `Fx.casing4`.
    pub const CASING4: EffectId = EffectId(193);

    /// `Fx.casing2Double`.
    pub const CASING2_DOUBLE: EffectId = EffectId(194);

    /// `Fx.casing3Double`.
    pub const CASING3_DOUBLE: EffectId = EffectId(195);

    /// `Fx.railShoot`.
    pub const RAIL_SHOOT: EffectId = EffectId(196);

    /// `Fx.railTrail`.
    pub const RAIL_TRAIL: EffectId = EffectId(197);

    /// `Fx.railHit`.
    pub const RAIL_HIT: EffectId = EffectId(198);

    /// `Fx.lancerLaserShoot`.
    pub const LANCER_LASER_SHOOT: EffectId = EffectId(199);

    /// `Fx.lancerLaserShootSmoke`.
    pub const LANCER_LASER_SHOOT_SMOKE: EffectId = EffectId(200);

    /// `Fx.lancerLaserCharge`.
    pub const LANCER_LASER_CHARGE: EffectId = EffectId(201);

    /// `Fx.lancerLaserChargeBegin`.
    pub const LANCER_LASER_CHARGE_BEGIN: EffectId = EffectId(202);

    /// `Fx.lightningCharge`.
    pub const LIGHTNING_CHARGE: EffectId = EffectId(203);

    /// `Fx.sparkShoot`.
    pub const SPARK_SHOOT: EffectId = EffectId(204);

    /// `Fx.lightningShoot`.
    pub const LIGHTNING_SHOOT: EffectId = EffectId(205);

    /// `Fx.thoriumShoot`.
    pub const THORIUM_SHOOT: EffectId = EffectId(206);

    /// `Fx.reactorsmoke`.
    pub const REACTORSMOKE: EffectId = EffectId(207);

    /// `Fx.redgeneratespark`.
    pub const REDGENERATESPARK: EffectId = EffectId(208);

    /// `Fx.turbinegenerate`.
    pub const TURBINEGENERATE: EffectId = EffectId(209);

    /// `Fx.generatespark`.
    pub const GENERATESPARK: EffectId = EffectId(210);

    /// `Fx.fuelburn`.
    pub const FUELBURN: EffectId = EffectId(211);

    /// `Fx.incinerateSlag`.
    pub const INCINERATE_SLAG: EffectId = EffectId(212);

    /// `Fx.coreBurn`.
    pub const CORE_BURN: EffectId = EffectId(213);

    /// `Fx.plasticburn`.
    pub const PLASTICBURN: EffectId = EffectId(214);

    /// `Fx.conveyorPoof`.
    pub const CONVEYOR_POOF: EffectId = EffectId(215);

    /// `Fx.pulverize`.
    pub const PULVERIZE: EffectId = EffectId(216);

    /// `Fx.pulverizeRed`.
    pub const PULVERIZE_RED: EffectId = EffectId(217);

    /// `Fx.pulverizeSmall`.
    pub const PULVERIZE_SMALL: EffectId = EffectId(218);

    /// `Fx.pulverizeMedium`.
    pub const PULVERIZE_MEDIUM: EffectId = EffectId(219);

    /// `Fx.unitMine`.
    pub const UNIT_MINE: EffectId = EffectId(220);

    /// `Fx.producesmoke`.
    pub const PRODUCESMOKE: EffectId = EffectId(221);

    /// `Fx.artilleryTrailSmoke`.
    pub const ARTILLERY_TRAIL_SMOKE: EffectId = EffectId(222);

    /// `Fx.smokeCloud`.
    pub const SMOKE_CLOUD: EffectId = EffectId(223);

    /// `Fx.smeltsmoke`.
    pub const SMELTSMOKE: EffectId = EffectId(224);

    /// `Fx.coalSmeltsmoke`.
    pub const COAL_SMELTSMOKE: EffectId = EffectId(225);

    /// `Fx.formsmoke`.
    pub const FORMSMOKE: EffectId = EffectId(226);

    /// `Fx.blastsmoke`.
    pub const BLASTSMOKE: EffectId = EffectId(227);

    /// `Fx.lava`.
    pub const LAVA: EffectId = EffectId(228);

    /// `Fx.dooropen`.
    pub const DOOROPEN: EffectId = EffectId(229);

    /// `Fx.doorclose`.
    pub const DOORCLOSE: EffectId = EffectId(230);

    /// `Fx.dooropenlarge`.
    pub const DOOROPENLARGE: EffectId = EffectId(231);

    /// `Fx.doorcloselarge`.
    pub const DOORCLOSELARGE: EffectId = EffectId(232);

    /// `Fx.generate`.
    pub const GENERATE: EffectId = EffectId(233);

    /// `Fx.mineWallSmall`.
    pub const MINE_WALL_SMALL: EffectId = EffectId(234);

    /// `Fx.mineSmall`.
    pub const MINE_SMALL: EffectId = EffectId(235);

    /// `Fx.mine`.
    pub const MINE: EffectId = EffectId(236);

    /// `Fx.mineBig`.
    pub const MINE_BIG: EffectId = EffectId(237);

    /// `Fx.mineHuge`.
    pub const MINE_HUGE: EffectId = EffectId(238);

    /// `Fx.mineImpact`.
    pub const MINE_IMPACT: EffectId = EffectId(239);

    /// `Fx.mineImpactWave`.
    pub const MINE_IMPACT_WAVE: EffectId = EffectId(240);

    /// `Fx.payloadReceive`.
    pub const PAYLOAD_RECEIVE: EffectId = EffectId(241);

    /// `Fx.teleportActivate`.
    pub const TELEPORT_ACTIVATE: EffectId = EffectId(242);

    /// `Fx.teleport`.
    pub const TELEPORT: EffectId = EffectId(243);

    /// `Fx.teleportOut`.
    pub const TELEPORT_OUT: EffectId = EffectId(244);

    /// `Fx.ripple`.
    pub const RIPPLE: EffectId = EffectId(245);

    /// `Fx.bubble`.
    pub const BUBBLE: EffectId = EffectId(246);

    /// `Fx.launchAccelerator`.
    pub const LAUNCH_ACCELERATOR: EffectId = EffectId(247);

    /// `Fx.launch`.
    pub const LAUNCH: EffectId = EffectId(248);

    /// `Fx.launchPod`.
    pub const LAUNCH_POD: EffectId = EffectId(249);

    /// `Fx.healWaveMend`.
    pub const HEAL_WAVE_MEND: EffectId = EffectId(250);

    /// `Fx.overdriveWave`.
    pub const OVERDRIVE_WAVE: EffectId = EffectId(251);

    /// `Fx.healBlock`.
    pub const HEAL_BLOCK: EffectId = EffectId(252);

    /// `Fx.healBlockFull`.
    pub const HEAL_BLOCK_FULL: EffectId = EffectId(253);

    /// `Fx.rotateBlock`.
    pub const ROTATE_BLOCK: EffectId = EffectId(254);

    /// `Fx.lightBlock`.
    pub const LIGHT_BLOCK: EffectId = EffectId(255);

    /// `Fx.overdriveBlockFull`.
    pub const OVERDRIVE_BLOCK_FULL: EffectId = EffectId(256);

    /// `Fx.shieldBreak`.
    pub const SHIELD_BREAK: EffectId = EffectId(257);

    /// `Fx.arcShieldBreak`.
    pub const ARC_SHIELD_BREAK: EffectId = EffectId(258);

    /// `Fx.coreLandDust`.
    pub const CORE_LAND_DUST: EffectId = EffectId(259);

    /// `Fx.podLandDust`.
    pub const POD_LAND_DUST: EffectId = EffectId(260);

    /// `Fx.unitShieldBreak`.
    pub const UNIT_SHIELD_BREAK: EffectId = EffectId(261);

    /// `Fx.chainLightning`.
    pub const CHAIN_LIGHTNING: EffectId = EffectId(262);

    /// `Fx.chainEmp`.
    pub const CHAIN_EMP: EffectId = EffectId(263);

    /// `Fx.legDestroy`.
    pub const LEG_DESTROY: EffectId = EffectId(264);

    /// `Fx.debugLine`.
    pub const DEBUG_LINE: EffectId = EffectId(265);

    /// `Fx.debugRect`.
    pub const DEBUG_RECT: EffectId = EffectId(266);
}

impl Default for EffectId {
    fn default() -> Self {
        EffectId::NONE
    }
}

/// All seed effects in upstream declaration order.
pub static EFFECTS: &[EffectMeta] = &[
    EffectMeta {
        name: "none",
        lifetime: 0.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "blockCrash",
        lifetime: 90.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "trailFade",
        lifetime: 400.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitSpawn",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitCapKill",
        lifetime: 80.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitEnvKill",
        lifetime: 80.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitControl",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitDespawn",
        lifetime: 100.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitSpirit",
        lifetime: 17.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "itemTransfer",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "pointBeam",
        lifetime: 25.0,
        clip: 300.0,
    },
    EffectMeta {
        name: "pointHit",
        lifetime: 8.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitScepterSecondary",
        lifetime: 8.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "lightning",
        lifetime: 10.0,
        clip: 500.0,
    },
    EffectMeta {
        name: "coreBuildShockwave",
        lifetime: 120.0,
        clip: 500.0,
    },
    EffectMeta {
        name: "coreBuildBlock",
        lifetime: 80.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "pointShockwave",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "moveCommand",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "attackCommand",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "commandSend",
        lifetime: 28.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "upgradeCore",
        lifetime: 120.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "upgradeCoreBloom",
        lifetime: 80.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "placeBlock",
        lifetime: 16.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "coreLaunchConstruct",
        lifetime: 35.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "tapBlock",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "breakBlock",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "payloadDeposit",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "select",
        lifetime: 23.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "smoke",
        lifetime: 100.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "fallSmoke",
        lifetime: 110.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitWreck",
        lifetime: 200.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "rocketSmoke",
        lifetime: 120.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "rocketSmokeLarge",
        lifetime: 220.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "magmasmoke",
        lifetime: 110.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "spawn",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitAssemble",
        lifetime: 70.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "padlaunch",
        lifetime: 10.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "breakProp",
        lifetime: 23.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitDrop",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitLand",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitDust",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitLandSmall",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitPickup",
        lifetime: 18.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "crawlDust",
        lifetime: 35.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "landShock",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "pickup",
        lifetime: 18.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "sparkExplosion",
        lifetime: 30.0,
        clip: 160.0,
    },
    EffectMeta {
        name: "titanExplosion",
        lifetime: 30.0,
        clip: 160.0,
    },
    EffectMeta {
        name: "titanExplosionLarge",
        lifetime: 45.0,
        clip: 220.0,
    },
    EffectMeta {
        name: "titanExplosionSmall",
        lifetime: 22.0,
        clip: 120.0,
    },
    EffectMeta {
        name: "titanExplosionFrag",
        lifetime: 20.0,
        clip: 50.0,
    },
    EffectMeta {
        name: "titanSmoke",
        lifetime: 300.0,
        clip: 300.0,
    },
    EffectMeta {
        name: "titanSmokeLarge",
        lifetime: 400.0,
        clip: 400.0,
    },
    EffectMeta {
        name: "titanSmokeSmall",
        lifetime: 200.0,
        clip: 200.0,
    },
    EffectMeta {
        name: "coreExplosion",
        lifetime: 55.0,
        clip: 240.0,
    },
    EffectMeta {
        name: "smokeAoeCloud",
        lifetime: 180.0,
        clip: 250.0,
    },
    EffectMeta {
        name: "missileTrailSmoke",
        lifetime: 180.0,
        clip: 300.0,
    },
    EffectMeta {
        name: "missileTrailSmokeSmall",
        lifetime: 120.0,
        clip: 200.0,
    },
    EffectMeta {
        name: "neoplasmSplat",
        lifetime: 400.0,
        clip: 300.0,
    },
    EffectMeta {
        name: "scatheExplosion",
        lifetime: 60.0,
        clip: 160.0,
    },
    EffectMeta {
        name: "scatheExplosionSmall",
        lifetime: 40.0,
        clip: 160.0,
    },
    EffectMeta {
        name: "scatheLight",
        lifetime: 60.0,
        clip: 160.0,
    },
    EffectMeta {
        name: "scatheLightSmall",
        lifetime: 60.0,
        clip: 160.0,
    },
    EffectMeta {
        name: "titanLightSmall",
        lifetime: 40.0,
        clip: 100.0,
    },
    EffectMeta {
        name: "scatheSlash",
        lifetime: 40.0,
        clip: 160.0,
    },
    EffectMeta {
        name: "dynamicSpikes",
        lifetime: 40.0,
        clip: 100.0,
    },
    EffectMeta {
        name: "greenBomb",
        lifetime: 40.0,
        clip: 100.0,
    },
    EffectMeta {
        name: "greenLaserCharge",
        lifetime: 80.0,
        clip: 100.0,
    },
    EffectMeta {
        name: "greenLaserChargeSmall",
        lifetime: 40.0,
        clip: 100.0,
    },
    EffectMeta {
        name: "greenCloud",
        lifetime: 80.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "healWaveDynamic",
        lifetime: 22.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "healWave",
        lifetime: 22.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "heal",
        lifetime: 11.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "dynamicWave",
        lifetime: 22.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shieldWave",
        lifetime: 22.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shieldApply",
        lifetime: 11.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "disperseTrail",
        lifetime: 13.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitBulletSmall",
        lifetime: 14.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitBulletColor",
        lifetime: 14.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitSquaresColor",
        lifetime: 14.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "squareWaveEffect",
        lifetime: 14.0,
        clip: 40.0,
    },
    EffectMeta {
        name: "hitFuse",
        lifetime: 14.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitBulletBig",
        lifetime: 13.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitFlameSmall",
        lifetime: 14.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitFlamePlasma",
        lifetime: 14.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitLiquid",
        lifetime: 16.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitLaserBlast",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitEmpSpark",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitLancer",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitLancerLow",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitBeam",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitFlameBeam",
        lifetime: 19.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitMeltdown",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitMeltHeal",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "instBomb",
        lifetime: 15.0,
        clip: 100.0,
    },
    EffectMeta {
        name: "instTrail",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "instShoot",
        lifetime: 24.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "instHit",
        lifetime: 20.0,
        clip: 200.0,
    },
    EffectMeta {
        name: "hitLaser",
        lifetime: 8.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "hitLaserColor",
        lifetime: 8.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "despawn",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "airBubble",
        lifetime: 100.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "flakExplosion",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "plasticExplosion",
        lifetime: 24.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "plasticExplosionFlak",
        lifetime: 28.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "blastExplosion",
        lifetime: 22.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "sapExplosion",
        lifetime: 25.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "massiveExplosion",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "artilleryTrail",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "incendTrail",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "missileTrail",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "missileTrailShort",
        lifetime: 22.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "bulletSparkSmokeTrailSmall",
        lifetime: 28.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "colorTrail",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "absorb",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "forceShrink",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "flakExplosionBig",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "burning",
        lifetime: 35.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "fireRemove",
        lifetime: 70.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "fire",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "fireHit",
        lifetime: 35.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "fireSmoke",
        lifetime: 35.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "neoplasmHeal",
        lifetime: 120.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "steam",
        lifetime: 35.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "ventSteam",
        lifetime: 140.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "drillSteam",
        lifetime: 220.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "fluxVapor",
        lifetime: 140.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "corrosionVapor",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "vapor",
        lifetime: 110.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "vaporSmall",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "fireballsmoke",
        lifetime: 25.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "ballfire",
        lifetime: 25.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "freezing",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "melting",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "wet",
        lifetime: 80.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "muddy",
        lifetime: 80.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "sapped",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "electrified",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "sporeSlowed",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "oily",
        lifetime: 42.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "overdriven",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "overclocked",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "dropItem",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shockwave",
        lifetime: 10.0,
        clip: 80.0,
    },
    EffectMeta {
        name: "shockwaveSmaller",
        lifetime: 9.0,
        clip: 80.0,
    },
    EffectMeta {
        name: "bigShockwave",
        lifetime: 10.0,
        clip: 80.0,
    },
    EffectMeta {
        name: "spawnShockwave",
        lifetime: 20.0,
        clip: 400.0,
    },
    EffectMeta {
        name: "podLandShockwave",
        lifetime: 12.0,
        clip: 80.0,
    },
    EffectMeta {
        name: "explosion",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "dynamicExplosion",
        lifetime: 30.0,
        clip: 500.0,
    },
    EffectMeta {
        name: "reactorExplosion",
        lifetime: 30.0,
        clip: 500.0,
    },
    EffectMeta {
        name: "impactReactorExplosion",
        lifetime: 30.0,
        clip: 500.0,
    },
    EffectMeta {
        name: "blockExplosionSmoke",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "steamCoolSmoke",
        lifetime: 35.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "smokePuff",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootSmall",
        lifetime: 8.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootSmallColor",
        lifetime: 8.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootHeal",
        lifetime: 8.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootHealYellow",
        lifetime: 8.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootSmallSmoke",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootBig",
        lifetime: 9.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootBig2",
        lifetime: 10.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootBigColor",
        lifetime: 11.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootScepterSecondary",
        lifetime: 4.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootQuellPulse",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootTitan",
        lifetime: 10.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootBigSmoke",
        lifetime: 17.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootBigSmoke2",
        lifetime: 18.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootSmokeDisperse",
        lifetime: 25.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootSmokeSquare",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootSmokeSquareSparse",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootSmokeSquareBig",
        lifetime: 32.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootSmokeTitan",
        lifetime: 70.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootSmokeSmite",
        lifetime: 70.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootSmokeMissile",
        lifetime: 130.0,
        clip: 300.0,
    },
    EffectMeta {
        name: "shootSmokeMissileColor",
        lifetime: 130.0,
        clip: 300.0,
    },
    EffectMeta {
        name: "regenParticle",
        lifetime: 100.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "regenSuppressParticle",
        lifetime: 35.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "regenSuppressSeek",
        lifetime: 140.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "surgeCruciSmoke",
        lifetime: 160.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "neoplasiaSmoke",
        lifetime: 280.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "heatReactorSmoke",
        lifetime: 180.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "circleColorSpark",
        lifetime: 21.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "colorSpark",
        lifetime: 21.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "colorSparkBig",
        lifetime: 25.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "randLifeSpark",
        lifetime: 24.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootPayloadDriver",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shootSmallFlame",
        lifetime: 32.0,
        clip: 80.0,
    },
    EffectMeta {
        name: "shootPyraFlame",
        lifetime: 33.0,
        clip: 80.0,
    },
    EffectMeta {
        name: "shootLiquid",
        lifetime: 15.0,
        clip: 80.0,
    },
    EffectMeta {
        name: "casing1",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "casing2",
        lifetime: 34.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "casing3",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "casing4",
        lifetime: 45.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "casing2Double",
        lifetime: 34.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "casing3Double",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "railShoot",
        lifetime: 24.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "railTrail",
        lifetime: 16.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "railHit",
        lifetime: 18.0,
        clip: 200.0,
    },
    EffectMeta {
        name: "lancerLaserShoot",
        lifetime: 21.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "lancerLaserShootSmoke",
        lifetime: 26.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "lancerLaserCharge",
        lifetime: 38.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "lancerLaserChargeBegin",
        lifetime: 60.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "lightningCharge",
        lifetime: 38.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "sparkShoot",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "lightningShoot",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "thoriumShoot",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "reactorsmoke",
        lifetime: 17.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "redgeneratespark",
        lifetime: 90.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "turbinegenerate",
        lifetime: 100.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "generatespark",
        lifetime: 18.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "fuelburn",
        lifetime: 23.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "incinerateSlag",
        lifetime: 34.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "coreBurn",
        lifetime: 23.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "plasticburn",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "conveyorPoof",
        lifetime: 35.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "pulverize",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "pulverizeRed",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "pulverizeSmall",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "pulverizeMedium",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitMine",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "producesmoke",
        lifetime: 12.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "artilleryTrailSmoke",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "smokeCloud",
        lifetime: 70.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "smeltsmoke",
        lifetime: 15.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "coalSmeltsmoke",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "formsmoke",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "blastsmoke",
        lifetime: 26.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "lava",
        lifetime: 18.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "dooropen",
        lifetime: 10.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "doorclose",
        lifetime: 10.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "dooropenlarge",
        lifetime: 10.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "doorcloselarge",
        lifetime: 10.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "generate",
        lifetime: 11.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "mineWallSmall",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "mineSmall",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "mine",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "mineBig",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "mineHuge",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "mineImpact",
        lifetime: 90.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "mineImpactWave",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "payloadReceive",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "teleportActivate",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "teleport",
        lifetime: 60.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "teleportOut",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "ripple",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "bubble",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "launchAccelerator",
        lifetime: 22.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "launch",
        lifetime: 28.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "launchPod",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "healWaveMend",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "overdriveWave",
        lifetime: 50.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "healBlock",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "healBlockFull",
        lifetime: 20.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "rotateBlock",
        lifetime: 30.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "lightBlock",
        lifetime: 60.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "overdriveBlockFull",
        lifetime: 60.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "shieldBreak",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "arcShieldBreak",
        lifetime: 40.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "coreLandDust",
        lifetime: 100.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "podLandDust",
        lifetime: 70.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "unitShieldBreak",
        lifetime: 35.0,
        clip: 0.0,
    },
    EffectMeta {
        name: "chainLightning",
        lifetime: 20.0,
        clip: 300.0,
    },
    EffectMeta {
        name: "chainEmp",
        lifetime: 30.0,
        clip: 300.0,
    },
    EffectMeta {
        name: "legDestroy",
        lifetime: 90.0,
        clip: 100.0,
    },
    EffectMeta {
        name: "debugLine",
        lifetime: 90.0,
        clip: 1000000000000.0,
    },
    EffectMeta {
        name: "debugRect",
        lifetime: 90.0,
        clip: 1000000000000.0,
    },
];

/// Looks up an effect by upstream name.
pub fn effect_by_name(name: &str) -> Option<EffectId> {
    EFFECTS
        .iter()
        .position(|meta| meta.name == name)
        .map(|index| EffectId(index as u16))
}

/// Inline (anonymous) effect class tag used inside content definitions
/// (`entities/effect/*` + composite `Effect`s; append-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InlineEffectKind {
    /// Plain `new Effect(lifetime[, clip], renderer)`.
    #[default]
    Effect = 0,
    /// `MultiEffect` (parallel children).
    MultiEffect = 1,
    /// `ExplosionEffect`.
    ExplosionEffect = 2,
    /// `WaveEffect`.
    WaveEffect = 3,
    /// `WrapEffect` (recolored child).
    WrapEffect = 4,
}

impl InlineEffectKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            InlineEffectKind::Effect => "Effect",
            InlineEffectKind::MultiEffect => "MultiEffect",
            InlineEffectKind::ExplosionEffect => "ExplosionEffect",
            InlineEffectKind::WaveEffect => "WaveEffect",
            InlineEffectKind::WrapEffect => "WrapEffect",
        }
    }
}

/// Inline effect metadata (constructor arguments + assigned fields).
///
/// Ported from `entities/effect/{MultiEffect,ExplosionEffect,WaveEffect,
/// WrapEffect}.java` (fields only). The renderer lambda bodies are draw code
/// owned by plan 17; this record preserves the parameterized data (lifetime,
/// clip, subclass fields, composite children).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectSpec {
    /// Effect class tag.
    pub kind: InlineEffectKind,
    /// `Effect.lifetime` (ticks).
    pub lifetime: f32,
    /// `Effect.clip` (radius; `Effect(lifetime, clip, ...)` when set at
    /// construction).
    pub clip: f32,
    /// Composite children (`MultiEffect`/`WrapEffect`).
    pub children: Vec<EffectRef>,
    /// `WrapEffect.color` (nullable upstream).
    pub color: Option<crate::content::color::Rgba>,
    /// `ExplosionEffect.waveStroke`.
    pub wave_stroke: f32,
    /// `ExplosionEffect.waveColor` / `WaveEffect.colorFrom`.
    pub wave_color: Option<crate::content::color::Rgba>,
    /// `ExplosionEffect.waveLife`.
    pub wave_life: f32,
    /// `ExplosionEffect.waveRad`.
    pub wave_rad: f32,
    /// `ExplosionEffect.waveRadBase`.
    pub wave_rad_base: f32,
    /// `ExplosionEffect.sparkColor`.
    pub spark_color: Option<crate::content::color::Rgba>,
    /// `ExplosionEffect.sparkRad`.
    pub spark_rad: f32,
    /// `ExplosionEffect.sparks`.
    pub sparks: i32,
    /// `ExplosionEffect.sparkLen`.
    pub spark_len: f32,
    /// `ExplosionEffect.sparkStroke`.
    pub spark_stroke: f32,
    /// `ExplosionEffect.smokeColor`.
    pub smoke_color: Option<crate::content::color::Rgba>,
    /// `ExplosionEffect.smokes`.
    pub smokes: i32,
    /// `ExplosionEffect.smokeSize`.
    pub smoke_size: f32,
    /// `ExplosionEffect.smokeSizeBase`.
    pub smoke_size_base: f32,
    /// `WaveEffect.colorTo`.
    pub color_to: Option<crate::content::color::Rgba>,
    /// `WaveEffect.sizeFrom`.
    pub size_from: f32,
    /// `WaveEffect.sizeTo`.
    pub size_to: f32,
    /// `WaveEffect.strokeFrom`.
    pub stroke_from: f32,
    /// `WaveEffect.strokeTo`.
    pub stroke_to: f32,
}

impl EffectSpec {
    /// `new Effect(lifetime, renderer)` (renderer body is plan 17).
    pub fn plain(lifetime: f32) -> Self {
        Self {
            lifetime,
            ..Self::default()
        }
    }

    /// `new Effect(lifetime, clip, renderer)`.
    pub fn plain_clip(lifetime: f32, clip: f32) -> Self {
        Self {
            lifetime,
            clip,
            ..Self::default()
        }
    }

    /// `new MultiEffect(children...)` (lifetime derived: max of children,
    /// resolved by the generator from the source).
    pub fn multi(lifetime: f32, children: Vec<EffectRef>) -> Self {
        Self {
            kind: InlineEffectKind::MultiEffect,
            lifetime,
            children,
            ..Self::default()
        }
    }

    /// `new ExplosionEffect(){{...}}` (fields applied by the caller).
    pub fn explosion() -> Self {
        Self {
            kind: InlineEffectKind::ExplosionEffect,
            ..Self::default()
        }
    }

    /// `new WaveEffect(){{...}}`.
    pub fn wave() -> Self {
        Self {
            kind: InlineEffectKind::WaveEffect,
            ..Self::default()
        }
    }

    /// `new WrapEffect(child, color)`.
    pub fn wrap(child: EffectRef, color: crate::content::color::Rgba, lifetime: f32) -> Self {
        Self {
            kind: InlineEffectKind::WrapEffect,
            lifetime,
            children: vec![child],
            color: Some(color),
            ..Self::default()
        }
    }
}

/// Effect reference: a named `Fx` entry or an inline anonymous effect.
///
/// Vanilla unit/turret definitions construct anonymous effects inline
/// (`shootEffect = new ExplosionEffect(){{...}}`); plan 17 replays both kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum EffectRef {
    /// `Fx.<name>` from the seed table.
    Named(EffectId),
    /// An inline `new Effect`/`ExplosionEffect`/... construction (data kept
    /// verbatim; renderer bodies are plan 17).
    Inline(Box<EffectSpec>),
}

impl EffectRef {
    /// Lifetime in ticks (inline effects carry their own).
    pub fn lifetime(&self) -> f32 {
        match self {
            EffectRef::Named(id) => id.meta().lifetime,
            EffectRef::Inline(spec) => spec.lifetime,
        }
    }
}

impl Default for EffectRef {
    fn default() -> Self {
        EffectRef::Named(EffectId::NONE)
    }
}

impl From<EffectId> for EffectRef {
    fn from(id: EffectId) -> Self {
        EffectRef::Named(id)
    }
}
