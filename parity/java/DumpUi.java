// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
//
// Plan 23 M2 / plan 14 §7d JVM oracle dump. Standalone tool: compiled and run
// against the upstream checkout's `:core` + `:tests` classpath (see run.sh).
// Emits the `UiKey` ordinal table used by plan 14's MSUI `ui_node` byte format.
//
// Usage: java -cp <classpath> DumpUi [out.json] [mindyVersion]
//
// NOTE: requires a Gradle-built checkout. The source-derived fallback
// `parity/java/extract_source_goldens.py` writes the equivalent
// `parity/golden/ui/ui_keys.json` (the enum declaration order).

import arc.util.serialization.Jval;
import mindustry.ui.builder.UiKey;

import java.nio.charset.StandardCharsets;
import java.nio.file.*;

public class DumpUi{
    public static void main(String[] args) throws Exception{
        ApplicationTests.launchApplication(false);

        Jval root = Jval.newObject();
        root.put("format", 1);
        root.put("mindustry_version", args.length > 1 ? args[1] : ("v" + mindustry.core.Version.number));
        root.put("generator", "parity/java/DumpUi.java");

        Jval keys = Jval.newArray();
        for(UiKey key : UiKey.all){
            Jval entry = Jval.newObject();
            entry.put("ordinal", key.ordinal());
            entry.put("name", key.name());
            keys.add(entry);
        }
        root.put("count", keys.size);
        root.put("keys", keys);

        Path out = Paths.get(args.length > 0 ? args[0] : "../../parity/golden/ui/ui_keys.json");
        Files.createDirectories(out.toAbsolutePath().getParent());
        Files.writeString(out, root.toString(Jval.Jformat.formatted) + "\n", StandardCharsets.UTF_8);
        System.out.println("wrote " + out.toAbsolutePath());
        System.exit(0);
    }
}
