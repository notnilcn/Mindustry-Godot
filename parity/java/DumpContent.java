// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
//
// Plan 02 §6.4 JVM golden dump (NUD-10). Standalone tool: compiled and run
// against the upstream checkout's `:core` + `:tests` classpath (see run.sh).
// Writes only into mindustry-godot/parity/ and never modifies the upstream repo.
//
// Usage: java -cp <classpath> DumpContent [out.json] [mindyVersion]

import arc.struct.*;
import arc.util.*;
import arc.util.serialization.Jval;
import mindustry.*;
import mindustry.ai.*;
import mindustry.content.*;
import mindustry.core.*;
import mindustry.ctype.*;
import mindustry.entities.bullet.*;
import mindustry.game.Objectives.*;
import mindustry.type.*;
import mindustry.world.*;
import mindustry.world.meta.*;

import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

public class DumpContent{
    /** Content types covered by the Rust golden (plan 02 `parity.rs`). */
    static final ContentType[] LIVE = {
        ContentType.item, ContentType.block, ContentType.bullet, ContentType.liquid,
        ContentType.status, ContentType.unit, ContentType.weather, ContentType.sector,
        ContentType.planet, ContentType.team, ContentType.unitCommand, ContentType.unitStance
    };

    public static void main(String[] args) throws Exception{
        // Boots the headless application exactly like `ApplicationTests`.
        ApplicationTests.launchApplication(false);

        String version = args.length > 1 ? args[1] : ("v" + Version.number + "." + Version.build);
        Jval root = Jval.newObject();
        root.put("format", 1);
        root.put("mindustry_version", version);
        root.put("generator", "parity/java/DumpContent.java");

        Jval counts = Jval.newObject();
        Jval types = Jval.newArray();
        for(ContentType type : LIVE){
            Seq<Content> seq = Vars.content.getBy(type);
            counts.put(type.name(), seq.size);
            Jval section = Jval.newObject();
            section.put("type", type.name());
            Jval entries = Jval.newArray();
            for(Content content : seq){
                entries.add(dumpEntry(content));
            }
            section.put("entries", entries);
            types.add(section);
        }
        root.put("counts", counts);
        root.put("types", types);

        Jval trees = Jval.newArray();
        for(TechTree.TechNode node : TechTree.roots){
            Jval tree = Jval.newObject();
            tree.put("root", node.name == null ? "" : node.name);
            Jval nodes = Jval.newArray();
            dumpNode(node, nodes);
            tree.put("nodes", nodes);
            trees.add(tree);
        }
        root.put("tech_trees", trees);

        Jval names = Jval.newObject();
        names.put("craters", "crater-stone");
        names.put("deepwater", "deep-water");
        names.put("water", "shallow-water");
        names.put("slag", "molten-slag");
        root.put("mod_content_name_map", names);

        Path out = Paths.get(args.length > 0 ? args[0] : "../../parity/golden_content.json");
        Files.writeString(out, root.toString(Jval.Jformat.formatted) + "\n", StandardCharsets.UTF_8);
        System.out.println("wrote " + out.toAbsolutePath());
        System.exit(0);
    }

    static Jval dumpEntry(Content content){
        Jval entry = Jval.newObject();
        entry.put("id", content.id);
        String name = content instanceof MappableContent m ? m.name : null;
        entry.put("name", Jval.valueOf(name));
        entry.put("kind", kindName(content));
        String localized = content instanceof UnlockableContent u ? u.localizedName : (name == null ? "" : name);
        entry.put("localized", localized);
        String key = bundleKey(content, name);
        if(key == null){
            entry.put("bundle", Jval.NULL);
        }else{
            Jval bundle = Jval.newObject();
            bundle.put("name", key);
            entry.put("bundle", bundle);
        }
        Jval regions = Jval.newArray();
        if(name != null && content.getContentType() == ContentType.item){
            regions.add("item-" + name);
        }else if(name != null && content.getContentType() == ContentType.liquid){
            regions.add("liquid-" + name);
        }
        entry.put("regions", regions);
        entry.put("fields", dumpFields(content));
        return entry;
    }

    static String bundleKey(Content content, String name){
        if(name == null) return null;
        ContentType type = content.getContentType();
        if(type == ContentType.unitCommand) return "command." + name;
        if(type == ContentType.unitStance){
            return content instanceof ItemUnitStance ? "stance.mine" : "stance." + name;
        }
        if(type == ContentType.bullet || type == ContentType.error) return null;
        return type.name() + "." + name + ".name";
    }

    static String kindName(Content content){
        Class<?> cls = content.getClass();
        while(cls != null && (cls.isAnonymousClass() || cls.getSimpleName().isEmpty())){
            cls = cls.getSuperclass();
        }
        return cls == null ? "Content" : cls.getSimpleName();
    }

    static Jval dumpFields(Content content){
        Jval fields = Jval.newObject();
        ContentType type = content.getContentType();
        if(type == ContentType.item && content instanceof Item item){
            fields.put("hardness", item.hardness);
            fields.put("cost", f(item.cost));
            fields.put("health_scaling", f(item.healthScaling));
            fields.put("explosiveness", f(item.explosiveness));
            fields.put("flammability", f(item.flammability));
            fields.put("radioactivity", f(item.radioactivity));
            fields.put("charge", f(item.charge));
            fields.put("buildable", item.buildable);
            fields.put("hidden", item.hidden);
            fields.put("frames", item.frames);
            fields.put("transition_frames", item.transitionFrames);
            fields.put("frame_time", f(item.frameTime));
        }else if(type == ContentType.block && content instanceof Block block){
            fields.put("size", block.size);
            fields.put("health", block.health);
            fields.put("scaled_health", f(block.scaledHealth));
            fields.put("armor", f(block.armor));
            fields.put("category", block.category.name());
            fields.put("build_cost_multiplier", f(block.buildCostMultiplier));
            fields.put("build_time", f(block.buildTime));
            fields.put("item_capacity", block.itemCapacity);
            fields.put("liquid_capacity", f(block.liquidCapacity));
            fields.put("solid", block.solid);
            fields.put("update", block.update);
            fields.put("configurable", block.configurable);
            fields.put("has_items", block.hasItems);
            fields.put("has_liquids", block.hasLiquids);
            fields.put("has_power", block.hasPower);
        }else if(type == ContentType.unit && content instanceof UnitType unit){
            fields.put("health", f(unit.health));
            fields.put("armor", f(unit.armor));
            fields.put("speed", f(unit.speed));
            fields.put("hit_size", f(unit.hitSize));
            fields.put("rotate_speed", f(unit.rotateSpeed));
            fields.put("item_capacity", unit.itemCapacity);
            fields.put("range", f(unit.range));
            fields.put("max_range", f(unit.maxRange));
            fields.put("fog_radius", f(unit.fogRadius));
            fields.put("payload_capacity", f(unit.payloadCapacity));
            fields.put("hidden", unit.hidden);
            fields.put("can_attack", unit.canAttack);
        }else if(type == ContentType.bullet && content instanceof BulletType bullet){
            fields.put("speed", f(bullet.speed));
            fields.put("lifetime", f(bullet.lifetime));
            fields.put("damage", f(bullet.damage));
            fields.put("splash_damage", f(bullet.splashDamage));
            fields.put("pierce", bullet.pierce);
            fields.put("collides", bullet.collides);
        }else if(type == ContentType.liquid && content instanceof Liquid liquid){
            fields.put("gas", liquid.gas);
            fields.put("temperature", f(liquid.temperature));
            fields.put("viscosity", f(liquid.viscosity));
            fields.put("flammability", f(liquid.flammability));
            fields.put("heat_capacity", f(liquid.heatCapacity));
            fields.put("explosiveness", f(liquid.explosiveness));
            fields.put("coolant", liquid.coolant);
            fields.put("hidden", liquid.hidden);
        }
        return fields;
    }

    /** Shortest-decimal f32 like the Rust golden (`parity.rs::finite`). */
    static float f(float value){
        return value;
    }

    static void dumpNode(TechTree.TechNode node, Jval nodes){
        Jval out = Jval.newObject();
        out.put("path", node.content.name);
        out.put("content", Jval.valueOf(node.content.name));
        if(node.parent == null){
            out.put("parent", Jval.NULL);
        }else{
            out.put("parent", Jval.valueOf(node.parent.content.name));
        }
        out.put("depth", node.depth);
        Jval requirements = Jval.newArray();
        if(node.requirements != null){
            for(ItemStack stack : node.requirements){
                Jval pair = Jval.newArray();
                pair.add(stack.item.name);
                pair.add(stack.amount);
                requirements.add(pair);
            }
        }
        out.put("requirements", requirements);
        Jval objectives = Jval.newArray();
        for(Objective objective : node.objectives){
            objectives.add(objectiveName(objective));
        }
        out.put("objectives", objectives);
        nodes.add(out);
        for(TechTree.TechNode child : node.children){
            dumpNode(child, nodes);
        }
    }

    static String objectiveName(Objective objective){
        if(objective instanceof SectorComplete o) return "sector-complete:" + o.preset.name;
        if(objective instanceof OnSector o) return "on-sector:" + o.preset.name;
        if(objective instanceof OnPlanet o) return "on-planet:" + o.planet.id;
        if(objective instanceof Research o) return "research:" + o.content.name;
        if(objective instanceof Produce o) return "produce:" + o.content.name;
        return objective.getClass().getSimpleName();
    }
}
