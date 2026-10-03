// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
//
// Plan 23 M2 / plan 04 §6.2 JVM oracle dump. Standalone tool: compiled and run
// against the upstream checkout's `:core` + `:tests` classpath (see run.sh).
// Emits the `Rules` public field order (the JSON key order JsonIO tolerates).
//
// Usage: java -cp <classpath> DumpJsonIO [out.json] [mindyVersion]
//
// NOTE: requires a Gradle-built checkout. The source-derived fallback
// `parity/java/extract_source_goldens.py` writes the equivalent
// `parity/golden/io/rules_fields.json` from `game/Rules.java`.

import arc.util.serialization.Jval;
import mindustry.game.Rules;

import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

public class DumpJsonIO{
    public static void main(String[] args) throws Exception{
        ApplicationTests.launchApplication(false);

        Jval root = Jval.newObject();
        root.put("format", 1);
        root.put("mindustry_version", args.length > 1 ? args[1] : ("v" + mindustry.core.Version.number));
        root.put("generator", "parity/java/DumpJsonIO.java");

        Jval fields = Jval.newArray();
        for(Field field : Rules.class.getDeclaredFields()){
            int mods = field.getModifiers();
            if(!Modifier.isPublic(mods) || Modifier.isStatic(mods) || Modifier.isTransient(mods)) continue;
            Jval entry = Jval.newObject();
            entry.put("type", field.getType().getSimpleName());
            entry.put("name", field.getName());
            fields.add(entry);
        }
        root.put("count", fields.size);
        root.put("fields", fields);

        Path out = Paths.get(args.length > 0 ? args[0] : "../../parity/golden/io/rules_fields.json");
        Files.createDirectories(out.toAbsolutePath().getParent());
        Files.writeString(out, root.toString(Jval.Jformat.formatted) + "\n", StandardCharsets.UTF_8);
        System.out.println("wrote " + out.toAbsolutePath());
        System.exit(0);
    }
}
