// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
//
// Plan 23 M2 / plan 13 §6.2 JVM oracle dump. Standalone tool: compiled and run
// against the upstream checkout's `:core` + `:tests` classpath (see run.sh).
// Emits the LogicIO statement field order (Java declaration order) so the Rust
// port's `LogicIO` text round-trip can be pinned.
//
// Usage: java -cp <classpath> DumpLogicIO [out.json] [mindyVersion]
//
// NOTE: requires a Gradle-built checkout. When that build is unavailable the
// source-derived fallback `parity/java/extract_source_goldens.py` produces the
// equivalent `parity/golden/logic/field_order.json` and the deferral is recorded
// in 23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md.

import arc.util.serialization.Jval;
import mindustry.logic.LStatement;
import mindustry.logic.LStatements;

import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

public class DumpLogicIO{
    public static void main(String[] args) throws Exception{
        // Boots the headless application exactly like `ApplicationTests`.
        ApplicationTests.launchApplication(false);

        Jval root = Jval.newObject();
        root.put("format", 1);
        root.put("mindustry_version", args.length > 1 ? args[1] : ("v" + mindustry.core.Version.number));
        root.put("generator", "parity/java/DumpLogicIO.java");

        Jval statements = Jval.newArray();
        // Every nested `LStatement` subclass is a registered statement.
        for(Class<?> cls : LStatements.class.getDeclaredClasses()){
            if(!LStatement.class.isAssignableFrom(cls) || Modifier.isAbstract(cls.getModifiers())) continue;
            LStatement instance;
            try{
                instance = (LStatement)cls.getDeclaredConstructor().newInstance();
            }catch(Exception ignored){
                continue;
            }
            String reg = instance.name;
            Jval out = Jval.newObject();
            out.put("reg", reg);
            out.put("variant", cls.getSimpleName());
            Jval fields = Jval.newArray();
            // Serialized fields are the declared `Var`/primitive fields in
            // declaration order (static/transient excluded).
            for(Field field : cls.getDeclaredFields()){
                int mods = field.getModifiers();
                if(Modifier.isStatic(mods) || Modifier.isTransient(mods)) continue;
                fields.add(field.getName());
            }
            out.put("fields", fields);
            statements.add(out);
        }
        root.put("statements", statements);
        root.put("count", statements.size);

        Path out = Paths.get(args.length > 0 ? args[0] : "../../parity/golden/logic/field_order.json");
        Files.createDirectories(out.toAbsolutePath().getParent());
        Files.writeString(out, root.toString(Jval.Jformat.formatted) + "\n", StandardCharsets.UTF_8);
        System.out.println("wrote " + out.toAbsolutePath());
        System.exit(0);
    }
}
