// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/content/SerpuloTechTree.java
//
//! Vanilla tech-tree node data (flat pre-order; depth tracks Java nesting).
//!
//! Generated once from the upstream source with a mechanical converter; node
//! order, parents, objectives and produce nodes match `TechTree.node` nesting.
//! Names for content whose registries land in M3/M5 stay unresolved until then
//! (`TechTreeBuildReport::missing`), the same data resolving completely afterwards.

use super::{NodeObjective as obj, TechTreeBuilder};

/// Loads the tree into `t`.
pub fn load(t: &mut TechTreeBuilder<'_>) {
    t.root("Serpulo", "core-shard", false);
    t.at(1, "conveyor");
    t.at(2, "junction");
    t.at(3, "router");
    t.at_obj(
        4,
        "advanced-launch-pad",
        &[obj::SectorComplete("extractionOutpost")],
    );
    t.at(5, "landing-pad");
    t.at_obj(
        6,
        "interplanetary-accelerator",
        &[obj::SectorComplete("planetaryTerminal")],
    );
    t.at(4, "distributor");
    t.at(4, "sorter");
    t.at(5, "inverted-sorter");
    t.at(5, "overflow-gate");
    t.at(6, "underflow-gate");
    t.at_obj(4, "container", &[obj::SectorComplete("biomassFacility")]);
    t.at(5, "unloader");
    t.at_obj(5, "vault", &[obj::SectorComplete("stainedMountains")]);
    t.at(4, "bridge-conveyor");
    t.at_obj(
        5,
        "titanium-conveyor",
        &[obj::SectorComplete("crateredBattleground")],
    );
    t.at_obj(6, "mass-driver", &[obj::SectorComplete("tarFields")]);
    t.at(7, "phase-conveyor");
    t.at(6, "payload-conveyor");
    t.at(7, "payload-router");
    t.at(6, "plastanium-conveyor");
    t.at(7, "armored-conveyor");
    t.at(1, "core-foundation");
    t.at(2, "core-nucleus");
    t.at(1, "mechanical-drill");
    t.at(2, "mechanical-pump");
    t.at(3, "conduit");
    t.at(4, "liquid-junction");
    t.at(5, "liquid-router");
    t.at(6, "liquid-container");
    t.at(7, "liquid-tank");
    t.at(6, "bridge-conduit");
    t.at_obj(
        6,
        "pulse-conduit",
        &[obj::SectorComplete("windsweptIslands")],
    );
    t.at(7, "phase-conduit");
    t.at(7, "plated-conduit");
    t.at(7, "rotary-pump");
    t.at(8, "impulse-pump");
    t.at(2, "graphite-press");
    t.at_obj(3, "pneumatic-drill", &[obj::SectorComplete("frozenForest")]);
    t.at_obj(4, "cultivator", &[obj::SectorComplete("biomassFacility")]);
    t.at(4, "laser-drill");
    t.at_obj(5, "blast-drill", &[obj::SectorComplete("nuclearComplex")]);
    t.at_obj(5, "water-extractor", &[obj::SectorComplete("saltFlats")]);
    t.at(6, "oil-extractor");
    t.at_obj(
        3,
        "pyratite-mixer",
        &[obj::SectorComplete("crateredBattleground")],
    );
    t.at_obj(4, "blast-mixer", &[obj::SectorComplete("facility32m")]);
    t.at_obj(3, "silicon-smelter", &[obj::SectorComplete("frozenForest")]);
    t.at(4, "spore-press");
    t.at(5, "coal-centrifuge");
    t.at(6, "multi-press");
    t.at(7, "silicon-crucible");
    t.at_obj(
        5,
        "plastanium-compressor",
        &[obj::SectorComplete("windsweptIslands")],
    );
    t.at_obj(6, "phase-weaver", &[obj::SectorComplete("impact0078")]);
    t.at_obj(4, "kiln", &[obj::OnSector("crateredBattleground")]);
    t.at(5, "pulverizer");
    t.at(6, "incinerator");
    t.at(7, "melter");
    t.at_obj(8, "surge-smelter", &[obj::SectorComplete("coastline")]);
    t.at(8, "separator");
    t.at(9, "disassembler");
    t.at(8, "cryofluid-mixer");
    t.at(4, "micro-processor");
    t.at(5, "switch");
    t.at(6, "message");
    t.at(7, "logic-display");
    t.at(8, "large-logic-display");
    t.at(8, "tile-logic-display");
    t.at(7, "memory-cell");
    t.at(8, "memory-bank");
    t.at(6, "logic-processor");
    t.at(7, "hyper-processor");
    t.at(4, "illuminator");
    t.at_obj(2, "combustion-generator", &[obj::Research("coal")]);
    t.at(3, "power-node");
    t.at(4, "power-node-large");
    t.at(5, "diode");
    t.at(6, "surge-tower");
    t.at(4, "battery");
    t.at(5, "battery-large");
    t.at(4, "mender");
    t.at(5, "mend-projector");
    t.at_obj(6, "force-projector", &[obj::SectorComplete("impact0078")]);
    t.at_obj(
        7,
        "overdrive-projector",
        &[obj::SectorComplete("impact0078")],
    );
    t.at_obj(8, "overdrive-dome", &[obj::SectorComplete("desolateRift")]);
    t.at(6, "repair-point");
    t.at(7, "repair-turret");
    t.at_obj(
        4,
        "steam-generator",
        &[obj::SectorComplete("crateredBattleground")],
    );
    t.at(5, "thermal-generator");
    t.at(6, "differential-generator");
    t.at_obj(
        7,
        "thorium-reactor",
        &[obj::Research("cryofluid"), obj::OnSector("nuclearComplex")],
    );
    t.at(8, "impact-reactor");
    t.at(8, "rtg-generator");
    t.at(4, "solar-panel");
    t.at(5, "solar-panel-large");
    t.at(1, "duo");
    t.at(2, "copper-wall");
    t.at(3, "copper-wall-large");
    t.at(4, "scrap-wall");
    t.at(5, "scrap-wall-large");
    t.at(6, "scrap-wall-huge");
    t.at(7, "scrap-wall-gigantic");
    t.at(4, "titanium-wall");
    t.at(5, "titanium-wall-large");
    t.at(5, "door");
    t.at(6, "door-large");
    t.at(5, "plastanium-wall");
    t.at(6, "plastanium-wall-large");
    t.at(5, "thorium-wall");
    t.at(6, "thorium-wall-large");
    t.at(6, "surge-wall");
    t.at(7, "surge-wall-large");
    t.at(7, "phase-wall");
    t.at(8, "phase-wall-large");
    t.at(2, "scatter");
    t.at_obj(3, "hail", &[obj::SectorComplete("crateredBattleground")]);
    t.at(4, "salvo");
    t.at(5, "swarmer");
    t.at(6, "cyclone");
    t.at_obj(7, "spectre", &[obj::SectorComplete("nuclearComplex")]);
    t.at(5, "ripple");
    t.at(6, "fuse");
    t.at_obj(2, "arc", &[obj::OnSector("frozenForest")]);
    t.at(3, "scorch");
    t.at(4, "wave");
    t.at(5, "parallax");
    t.at(6, "segment");
    t.at_obj(5, "tsunami", &[obj::SectorComplete("navalFortress")]);
    t.at(4, "lancer");
    t.at(5, "meltdown");
    t.at(6, "foreshadow");
    t.at(5, "shock-mine");
    t.at(1, "ground-factory");
    t.at(2, "dagger");
    t.at(3, "mace");
    t.at(4, "fortress");
    t.at(5, "scepter");
    t.at_obj(6, "reign", &[obj::SectorComplete("desolateRift")]);
    t.at_obj(3, "nova", &[obj::SectorComplete("fungalPass")]);
    t.at(4, "pulsar");
    t.at(5, "quasar");
    t.at(6, "vela");
    t.at(7, "corvus");
    t.at_req(3, "crawler", &[("silicon", 400), ("graphite", 400)]);
    t.at(4, "atrax");
    t.at(5, "spiroct");
    t.at(6, "arkyid");
    t.at_obj(7, "toxopid", &[obj::SectorComplete("mycelialBastion")]);
    t.at(2, "air-factory");
    t.at(3, "flare");
    t.at(4, "horizon");
    t.at(5, "zenith");
    t.at(6, "antumbra");
    t.at(7, "eclipse");
    t.at(4, "mono");
    t.at(5, "poly");
    t.at(6, "mega");
    t.at(7, "quad");
    t.at(8, "oct");
    t.at_obj(3, "naval-factory", &[obj::SectorComplete("ruinousShores")]);
    t.at(4, "risso");
    t.at(5, "minke");
    t.at(6, "bryde");
    t.at(7, "sei");
    t.at_obj(8, "omura", &[obj::SectorComplete("littoralShipyard")]);
    t.at_obj(5, "retusa", &[obj::SectorComplete("windsweptIslands")]);
    t.at_obj(6, "oxynoe", &[obj::SectorComplete("coastline")]);
    t.at_obj(7, "cyerce", &[obj::SectorComplete("perilousHarbor")]);
    t.at(8, "aegires");
    t.at_obj(9, "navanax", &[obj::SectorComplete("navalFortress")]);
    t.at_obj(
        2,
        "additive-reconstructor",
        &[obj::SectorComplete("fungalPass")],
    );
    t.at_obj(
        3,
        "multiplicative-reconstructor",
        &[obj::SectorComplete("frontier")],
    );
    t.at_obj(
        4,
        "exponential-reconstructor",
        &[obj::SectorComplete("overgrowth")],
    );
    t.at_obj(
        5,
        "tetrative-reconstructor",
        &[obj::SectorComplete("mycelialBastion")],
    );
    t.at(1, "groundZero");
    t.at_obj(
        2,
        "frozenForest",
        &[
            obj::SectorComplete("groundZero"),
            obj::Research("junction"),
            obj::Research("router"),
        ],
    );
    t.at_obj(
        3,
        "crateredBattleground",
        &[
            obj::SectorComplete("frozenForest"),
            obj::Research("mender"),
            obj::Research("combustion-generator"),
        ],
    );
    t.at_obj(
        4,
        "ruinousShores",
        &[
            obj::SectorComplete("crateredBattleground"),
            obj::Research("graphite-press"),
            obj::Research("kiln"),
            obj::Research("mechanical-pump"),
        ],
    );
    t.at_obj(
        5,
        "windsweptIslands",
        &[
            obj::SectorComplete("ruinousShores"),
            obj::Research("pneumatic-drill"),
            obj::Research("hail"),
            obj::Research("silicon-smelter"),
            obj::Research("steam-generator"),
        ],
    );
    t.at_obj(
        6,
        "saltFlats",
        &[
            obj::SectorComplete("windsweptIslands"),
            obj::SectorComplete("fungalPass"),
            obj::SectorComplete("frontier"),
            obj::Research("ground-factory"),
            obj::Research("additive-reconstructor"),
            obj::Research("air-factory"),
            obj::Research("door"),
        ],
    );
    t.at_obj(
        7,
        "tarFields",
        &[
            obj::SectorComplete("saltFlats"),
            obj::Research("coal-centrifuge"),
            obj::Research("conduit"),
            obj::Research("wave"),
        ],
    );
    t.at_obj(
        8,
        "impact0078",
        &[
            obj::SectorComplete("tarFields"),
            obj::Research("thorium"),
            obj::Research("lancer"),
            obj::Research("salvo"),
            obj::Research("core-foundation"),
        ],
    );
    t.at_obj(
        9,
        "desolateRift",
        &[
            obj::SectorComplete("impact0078"),
            obj::Research("thermal-generator"),
            obj::Research("thorium-reactor"),
            obj::Research("core-nucleus"),
        ],
    );
    t.at_obj(
        10,
        "planetaryTerminal",
        &[
            obj::SectorComplete("desolateRift"),
            obj::SectorComplete("nuclearComplex"),
            obj::SectorComplete("extractionOutpost"),
            obj::SectorComplete("mycelialBastion"),
            obj::SectorComplete("littoralShipyard"),
            obj::Research("omura"),
            obj::Research("advanced-launch-pad"),
            obj::Research("mass-driver"),
            obj::Research("impact-reactor"),
            obj::Research("tetrative-reconstructor"),
        ],
    );
    t.at_obj(
        7,
        "coastline",
        &[
            obj::SectorComplete("tarFields"),
            obj::SectorComplete("saltFlats"),
            obj::Research("naval-factory"),
            obj::Research("payload-conveyor"),
        ],
    );
    t.at_obj(
        8,
        "testingGrounds",
        &[
            obj::SectorComplete("coastline"),
            obj::Research("cryofluid-mixer"),
            obj::Research("cryofluid"),
            obj::Research("water-extractor"),
            obj::Research("ripple"),
        ],
    );
    t.at_obj(
        8,
        "navalFortress",
        &[
            obj::SectorComplete("coastline"),
            obj::SectorComplete("extractionOutpost"),
            obj::Research("core-nucleus"),
            obj::Research("mass-driver"),
            obj::Research("oxynoe"),
            obj::Research("minke"),
            obj::Research("bryde"),
            obj::Research("cyclone"),
            obj::Research("ripple"),
        ],
    );
    t.at_obj(
        9,
        "sunkenPier",
        &[
            obj::SectorComplete("navalFortress"),
            obj::SectorComplete("coastline"),
            obj::Research("multiplicative-reconstructor"),
        ],
    );
    t.at_obj(
        9,
        "weatheredChannels",
        &[
            obj::SectorComplete("impact0078"),
            obj::SectorComplete("navalFortress"),
            obj::Research("bryde"),
            obj::Research("surge-smelter"),
            obj::Research("overdrive-projector"),
        ],
    );
    t.at_obj(
        4,
        "biomassFacility",
        &[
            obj::SectorComplete("crateredBattleground"),
            obj::Research("power-node"),
            obj::Research("steam-generator"),
            obj::Research("scatter"),
            obj::Research("graphite-press"),
        ],
    );
    t.at_obj(
        5,
        "stainedMountains",
        &[
            obj::SectorComplete("biomassFacility"),
            obj::Research("pneumatic-drill"),
            obj::Research("silicon-smelter"),
        ],
    );
    t.at_obj(
        6,
        "facility32m",
        &[
            obj::Research("plastanium-compressor"),
            obj::Research("lancer"),
            obj::Research("salvo"),
            obj::SectorComplete("stainedMountains"),
            obj::SectorComplete("windsweptIslands"),
        ],
    );
    t.at_obj(
        6,
        "infestedCanyons",
        &[
            obj::SectorComplete("fungalPass"),
            obj::SectorComplete("frontier"),
            obj::Research("naval-factory"),
            obj::Research("risso"),
            obj::Research("minke"),
            obj::Research("additive-reconstructor"),
        ],
    );
    t.at_obj(
        7,
        "nuclearComplex",
        &[
            obj::SectorComplete("infestedCanyons"),
            obj::Research("thermal-generator"),
            obj::Research("laser-drill"),
            obj::Research("plastanium"),
            obj::Research("swarmer"),
        ],
    );
    t.at_obj(
        7,
        "taintedWoods",
        &[
            obj::SectorComplete("infestedCanyons"),
            obj::Research("spore-pod"),
            obj::Research("plastanium"),
            obj::Research("wave"),
        ],
    );
    t.at_obj(
        5,
        "fungalPass",
        &[obj::Research("ground-factory"), obj::Research("dagger")],
    );
    t.at_obj(
        6,
        "frontier",
        &[
            obj::SectorComplete("biomassFacility"),
            obj::SectorComplete("fungalPass"),
            obj::Research("ground-factory"),
            obj::Research("air-factory"),
            obj::Research("additive-reconstructor"),
            obj::Research("mace"),
            obj::Research("mono"),
        ],
    );
    t.at_obj(
        7,
        "perilousHarbor",
        &[
            obj::SectorComplete("biomassFacility"),
            obj::SectorComplete("frontier"),
            obj::Research("naval-factory"),
            obj::Research("risso"),
            obj::Research("retusa"),
            obj::Research("steam-generator"),
            obj::Research("cultivator"),
            obj::Research("coal-centrifuge"),
        ],
    );
    t.at_obj(
        8,
        "extractionOutpost",
        &[
            obj::SectorComplete("windsweptIslands"),
            obj::SectorComplete("perilousHarbor"),
            obj::SectorComplete("facility32m"),
            obj::Research("multiplicative-reconstructor"),
            obj::Research("risso"),
            obj::Research("minke"),
            obj::Research("fortress"),
        ],
    );
    t.at_obj(
        9,
        "atolls",
        &[
            obj::SectorComplete("extractionOutpost"),
            obj::Research("poly"),
            obj::Research("mega"),
        ],
    );
    t.at_obj(
        7,
        "overgrowth",
        &[
            obj::SectorComplete("frontier"),
            obj::SectorComplete("windsweptIslands"),
            obj::Research("multiplicative-reconstructor"),
            obj::Research("fortress"),
            obj::Research("ripple"),
            obj::Research("salvo"),
            obj::Research("cultivator"),
            obj::Research("spore-press"),
        ],
    );
    t.at_obj(
        8,
        "mycelialBastion",
        &[
            obj::Research("atrax"),
            obj::Research("spiroct"),
            obj::Research("arkyid"),
            obj::Research("multiplicative-reconstructor"),
            obj::Research("exponential-reconstructor"),
        ],
    );
    t.at_obj(
        9,
        "littoralShipyard",
        &[
            obj::SectorComplete("desolateRift"),
            obj::SectorComplete("navalFortress"),
            obj::SectorComplete("mycelialBastion"),
            obj::Research("risso"),
            obj::Research("minke"),
            obj::Research("bryde"),
            obj::Research("sei"),
            obj::Research("spectre"),
            obj::Research("additive-reconstructor"),
            obj::Research("exponential-reconstructor"),
        ],
    );
    t.at_obj(1, "copper", &[obj::Produce("copper")]);
    t.at_obj(2, "water", &[obj::Produce("water")]);
    t.at_obj(2, "lead", &[obj::Produce("lead")]);
    t.at_obj(3, "titanium", &[obj::Produce("titanium")]);
    t.at_obj(4, "cryofluid", &[obj::Produce("cryofluid")]);
    t.at_obj(4, "thorium", &[obj::Produce("thorium")]);
    t.at_obj(5, "surge-alloy", &[obj::Produce("surge-alloy")]);
    t.at_obj(5, "phase-fabric", &[obj::Produce("phase-fabric")]);
    t.at_obj(3, "metaglass", &[obj::Produce("metaglass")]);
    t.at_obj(2, "sand", &[obj::Produce("sand")]);
    t.at_obj(3, "scrap", &[obj::Produce("scrap")]);
    t.at_obj(4, "slag", &[obj::Produce("slag")]);
    t.at_obj(3, "coal", &[obj::Produce("coal")]);
    t.at_obj(4, "graphite", &[obj::Produce("graphite")]);
    t.at_obj(5, "silicon", &[obj::Produce("silicon")]);
    t.at_obj(4, "pyratite", &[obj::Produce("pyratite")]);
    t.at_obj(5, "blast-compound", &[obj::Produce("blast-compound")]);
    t.at_obj(4, "spore-pod", &[obj::Produce("spore-pod")]);
    t.at_obj(4, "oil", &[obj::Produce("oil")]);
    t.at_obj(5, "plastanium", &[obj::Produce("plastanium")]);
}
