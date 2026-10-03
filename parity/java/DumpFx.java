// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
//
// Plan 23 M2 / plan 17 §7d JVM oracle dump. Standalone tool: compiled and run
// against the upstream checkout's `:core` + `:tests` classpath (see run.sh).
// Emits the `Fx` effect catalogue in declaration order (the plan-17 `fx audit`
// 267/267 order hash).
//
// Usage: java -cp <classpath> DumpFx [out.json] [mindyVersion]
//
// NOTE: requires a Gradle-built checkout. The source-derived fallback
// `parity/java/extract_source_goldens.py` writes the equivalent
// `parity/golden/fx/fx_order.json` from `content/Fx.java`.

import arc.util.serialization.Jval;
import mindustry.content.Fx;
import mindustry.entities.Effect;

import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;

public class DumpFx{
    public static void main(String[] args) throws Exception{
        ApplicationTests.launchApplication(false);

        Jval root = Jval.newObject();
        root.put("format", 1);
        root.put("mindustry_version", args.length > 1 ? args[1] : ("v" + mindustry.core.Version.number));
        root.put("generator", "parity/java/DumpFx.java");

        Jval effects = Jval.newArray();
        for(Field field : Fx.class.getDeclaredFields()){
            int mods = field.getModifiers();
            if(!Modifier.isStatic(mods) || !Effect.class.isAssignableFrom(field.getType())) continue;
            effects.add(field.getName());
        }
        root.put("count", effects.size);
        root.put("effects", effects);

        Path out = Paths.get(args.length > 0 ? args[0] : "../../parity/golden/fx/fx_order.json");
        Files.createDirectories(out.toAbsolutePath().getParent());
        Files.writeString(out, root.toString(Jval.Jformat.formatted) + "\n", StandardCharsets.UTF_8);
        System.out.println("wrote " + out.toAbsolutePath());
        System.exit(0);
    }
}
