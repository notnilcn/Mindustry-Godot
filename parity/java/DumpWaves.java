// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Parity dumper for plan 11 `Waves.generate` (R5): replicates the algorithm with
// Arc's exact `Rand` and a shape-identical (6x5) placeholder species table, then
// dumps the generated groups so the Rust port can be byte-compared.
//
// Build:
//   javac -d /tmp/mgwaves parity/java/DumpWaves.java ../Arc/arc-core/src/arc/math/Rand.java
// Run:
//   java -cp /tmp/mgwaves DumpWaves > parity/java/jvm_wave_generate.json

import arc.math.Rand;
import java.util.ArrayList;
import java.util.List;

public class DumpWaves{
    static final int NEVER = Integer.MAX_VALUE;

    static float lerp(float a, float b, float t){
        return a + (b - a) * t;
    }

    static float powf(float a, float b){
        return (float)Math.pow(a, b);
    }

    // Field defaults mirror `mindustry.game.SpawnGroup` initializers exactly.
    static class Group {
        String unit = "dagger";
        int begin = 0, end = NEVER, spacing = 1, max = 40, unitAmount = 1;
        float unitScaling = NEVER, shields = 0f, shieldScaling = 0f;
        boolean boss;
    }

    static List<Group> generate(float difficulty, Rand rand, boolean attack, boolean airOnly, boolean naval){
        // Shape-identical species table; row 5 consumes the two `chance` draws
        // exactly as upstream `new UnitType[][]{...}` does.
        String[][] speciesArr = {
            {"dagger", "mace", "fortress", "scepter", "reign"},
            {"nova", "pulsar", "quasar", "vela", "corvus"},
            {"crawler", "atrax", "spiroct", "arkyid", "toxopid"},
            {"risso", "minke", "bryde", "sei", "omura"},
            {"retusa", "oxynoe", "cyerce", "aegires", "navanax"},
            {"flare", "horizon", "zenith", rand.chance(0.5) ? "quad" : "antumbra", rand.chance(0.1) ? "quad" : "eclipse"}
        };
        boolean[] flying = {false, false, false, false, false, true};
        boolean[] navalFlag = {false, false, false, true, true, false};

        List<String[]> species = new ArrayList<>();
        for(int i = 0; i < speciesArr.length; i++){
            if(airOnly && !flying[i]) continue;
            if(naval){
                if(!(flying[i] || navalFlag[i])) continue;
            }else{
                if(navalFlag[i]) continue;
            }
            species.add(speciesArr[i]);
        }
        String[][] fspec = species.toArray(new String[0][]);

        List<Group> out = new ArrayList<>();
        int cap = 150;
        float shieldStart = 30, shieldsPerWave = 20 + difficulty * 30f;
        float[] scaling = {1, 2f, 3f, 4f, 5f};

        // createProgression is a closure over `curSpecies`/`curTier`.
        final String[][] fspecFinal = fspec;
        final Rand rng = rand;
        final List<Group> outFinal = out;
        final float difficultyFinal = difficulty;
        final float shieldsPerWaveFinal = shieldsPerWave;
        final float shieldStartFinal = shieldStart;
        final float[] scalingFinal = scaling;
        final int capFinal = cap;

        class Progress {
            String[] curSpecies;
            int curTier;
            void run(int start){
                curSpecies = fspecFinal[rng.random(fspecFinal.length - 1)];
                curTier = 0;
                for(int i = start; i < capFinal;){
                    int f = i;
                    int next = rng.random(8, 16) + (int)lerp(5f, 0f, difficultyFinal) + curTier * 4;
                    float shieldAmount = Math.max((i - shieldStartFinal) * shieldsPerWaveFinal, 0);
                    int space = start == 0 ? 1 : rng.random(1, 2);
                    int ctier = curTier;

                    Group main = new Group();
                    main.unit = curSpecies[Math.min(curTier, curSpecies.length - 1)];
                    main.unitAmount = f == start ? 1 : 6 / (int)scalingFinal[ctier];
                    main.begin = f;
                    main.end = f + next >= capFinal ? NEVER : f + next;
                    main.max = 13;
                    main.unitScaling = (difficultyFinal < 0.4f ? rng.random(2.5f, 5f) : rng.random(1f, 4f)) * scalingFinal[ctier];
                    main.shields = shieldAmount;
                    main.shieldScaling = shieldsPerWaveFinal;
                    main.spacing = space;
                    outFinal.add(main);

                    Group extra = new Group();
                    extra.unit = curSpecies[Math.min(curTier, curSpecies.length - 1)];
                    extra.unitAmount = 3 / (int)scalingFinal[ctier];
                    extra.begin = f + next - 1;
                    extra.end = f + next + rng.random(6, 10);
                    extra.max = 6;
                    extra.unitScaling = rng.random(2f, 4f);
                    extra.spacing = rng.random(2, 4);
                    extra.shields = shieldAmount / 2f;
                    extra.shieldScaling = shieldsPerWaveFinal;
                    outFinal.add(extra);

                    i += next + 1;
                    if(curTier < 3 || (rng.chance(0.05) && difficultyFinal > 0.8)){
                        curTier++;
                    }
                    curTier = Math.min(curTier, 3);
                    if(rng.chance(0.3)){
                        curSpecies = fspecFinal[rng.random(fspecFinal.length - 1)];
                    }
                }
            }
        }
        Progress progress = new Progress();
        progress.run(0);

        int step = 5 + rng.random(5);
        while(step <= capFinal){
            progress.run(step);
            step += (int)(rng.random(15, 30) * lerp(1f, 0.5f, difficultyFinal));
        }

        int bossWave = (int)(rng.random(50, 70) * lerp(1f, 0.5f, difficultyFinal));
        int bossSpacing = (int)(rng.random(25, 40) * lerp(1f, 0.5f, difficultyFinal));
        int bossTier = difficultyFinal < 0.6f ? 3 : 4;

        Group mainBoss = new Group();
        mainBoss.unit = species.get(rng.random(species.size() - 1))[bossTier];
        mainBoss.unitAmount = 1;
        mainBoss.begin = bossWave;
        mainBoss.spacing = bossSpacing;
        mainBoss.end = NEVER;
        mainBoss.max = 16;
        mainBoss.unitScaling = bossSpacing;
        mainBoss.shieldScaling = shieldsPerWaveFinal;
        mainBoss.boss = true;
        outFinal.add(mainBoss);

        Group altBoss = new Group();
        altBoss.unit = species.get(rng.random(species.size() - 1))[bossTier];
        altBoss.unitAmount = 1;
        altBoss.begin = bossWave + rng.random(3, 5) * bossSpacing;
        altBoss.spacing = bossSpacing;
        altBoss.end = NEVER;
        altBoss.max = 16;
        altBoss.unitScaling = bossSpacing;
        altBoss.shieldScaling = shieldsPerWaveFinal;
        altBoss.boss = true;
        outFinal.add(altBoss);

        int finalBossStart = 120 + rng.random(30);

        Group finalBoss = new Group();
        finalBoss.unit = species.get(rng.random(species.size() - 1))[bossTier];
        finalBoss.unitAmount = 1;
        finalBoss.begin = finalBossStart;
        finalBoss.spacing = bossSpacing / 2;
        finalBoss.end = NEVER;
        finalBoss.max = 16;
        finalBoss.unitScaling = bossSpacing;
        finalBoss.shields = 500;
        finalBoss.shieldScaling = shieldsPerWaveFinal * 4;
        finalBoss.boss = true;
        outFinal.add(finalBoss);

        Group finalBossAlt = new Group();
        finalBossAlt.unit = species.get(rng.random(species.size() - 1))[bossTier];
        finalBossAlt.unitAmount = 1;
        finalBossAlt.begin = finalBossStart + 15;
        finalBossAlt.spacing = bossSpacing / 2;
        finalBossAlt.end = NEVER;
        finalBossAlt.max = 16;
        finalBossAlt.unitScaling = bossSpacing;
        finalBossAlt.shields = 500;
        finalBossAlt.shieldScaling = shieldsPerWaveFinal * 4;
        finalBossAlt.boss = true;
        outFinal.add(finalBossAlt);

        if(attack && difficulty >= 0.5f){
            int amount = rng.random(1, 3 + (int)(difficulty * 2));
            for(int i = 0; i < amount; i++){
                int wave = rng.random(3, 20);
                Group mega = new Group();
                mega.unit = "mega";
                mega.unitAmount = 1;
                mega.begin = wave;
                mega.end = wave;
                mega.max = 16;
                outFinal.add(mega);
            }
        }

        int shift = Math.max((int)(difficulty * 14 - 5), 0);
        for(Group g : outFinal){
            g.begin -= shift;
            g.end -= shift;
        }

        return outFinal;
    }

    static String fbits(float f){
        return Integer.toUnsignedString(Float.floatToIntBits(f));
    }

    public static void main(String[] args){
        StringBuilder sb = new StringBuilder();
        sb.append("{\n  \"format\": 1,\n  \"vectors\": {");
        Rand v = new Rand(1);
        sb.append("\n    \"seed1_nextlong\": \"").append(Long.toUnsignedString(v.nextLong())).append("\",");
        Rand vf = new Rand(42);
        sb.append("\n    \"seed42_nextfloat_bits\": \"").append(Integer.toUnsignedString(Float.floatToIntBits(vf.nextFloat()))).append("\",");
        Rand vi = new Rand(7);
        sb.append("\n    \"seed7_first_three_bounds\": [")
          .append(vi.nextInt(100)).append(", ").append(vi.nextInt(100)).append(", ").append(vi.nextInt(100)).append("]\n  },\n");

        sb.append("  \"cases\": [\n");
        long[] seeds = {1, 2, 3};
        float[] diffs = {0f, 0.35f, 0.7f, 1.0f};
        boolean[][] flags = {{false,false,false}, {true,false,false}, {false,true,false}, {false,false,true}};
        boolean firstCase = true;
        for(long seed : seeds){
            for(float diff : diffs){
                for(boolean[] fl : flags){
                    Rand rand = new Rand(seed);
                    List<Group> groups = generate(diff, rand, fl[0], fl[1], fl[2]);
                    if(!firstCase) sb.append(",\n");
                    firstCase = false;
                    sb.append("    {\"seed\": ").append(seed)
                      .append(", \"difficulty\": ").append(Float.toString(diff))
                      .append(", \"attack\": ").append(fl[0])
                      .append(", \"airOnly\": ").append(fl[1])
                      .append(", \"naval\": ").append(fl[2])
                      .append(", \"groups\": [");
                    for(int i = 0; i < groups.size(); i++){
                        Group g = groups.get(i);
                        if(i > 0) sb.append(", ");
                        sb.append("{\"unit\": \"").append(g.unit)
                          .append("\", \"begin\": ").append(g.begin)
                          .append(", \"end\": ").append(g.end)
                          .append(", \"spacing\": ").append(g.spacing)
                          .append(", \"max\": ").append(g.max)
                          .append(", \"unitAmount\": ").append(g.unitAmount)
                          .append(", \"unitScalingBits\": \"").append(fbits(g.unitScaling))
                          .append("\", \"shieldsBits\": \"").append(fbits(g.shields))
                          .append("\", \"shieldScalingBits\": \"").append(fbits(g.shieldScaling))
                          .append("\", \"boss\": ").append(g.boss)
                          .append("}");
                    }
                    sb.append("]}");
                }
            }
        }
        sb.append("\n  ]\n}\n");
        System.out.print(sb);
    }
}
