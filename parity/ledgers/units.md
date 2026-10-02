# Ledger — units (upstream `content/UnitTypes.java`, 65 entries)

> Source-derived fingerprint (`golden_sha` = sha256/12 of the parsed metadata tree).
> The JVM golden (`parity/golden_content.json`) is pending (NUD-10); this ledger is the
> M5 mechanical audit per plan 02 §6.2/§9.

| pos | name | kind | ported | golden_sha | wave | notes |
|-----|------|------|--------|-----------|------|-------|
| 0 | dagger | UnitType | [x] | fe4f5a59f65b | standard |  |
| 1 | mace | UnitType | [x] | 7e5a9f0ff1f0 | standard |  |
| 2 | fortress | UnitType | [x] | 743d98fd21cf | standard |  |
| 3 | scepter | UnitType | [x] | 5dbee6b45649 | standard |  |
| 4 | reign | UnitType | [x] | 03a3fc221d5c | standard |  |
| 5 | nova | UnitType | [x] | 7c593a70afa6 | standard |  |
| 6 | pulsar | UnitType | [x] | 7fec73a793de | standard |  |
| 7 | quasar | UnitType | [x] | c5a5fe43f6db | standard |  |
| 8 | vela | UnitType | [x] | 556e060ea36f | standard |  |
| 9 | corvus | UnitType | [x] | a9893eb49c04 | standard |  |
| 10 | crawler | UnitType | [x] | 4467f8137534 | standard |  |
| 11 | atrax | UnitType | [x] | 13b054a6bac7 | standard |  |
| 12 | spiroct | UnitType | [x] | 2c7fe5637d61 | standard |  |
| 13 | arkyid | UnitType | [x] | a73b3c08d11b | standard |  |
| 14 | toxopid | UnitType | [x] | 00f6c69725c4 | standard |  |
| 15 | flare | UnitType | [x] | 6b67f5df80e1 | standard |  |
| 16 | horizon | UnitType | [x] | e02d9a2e4884 | standard |  |
| 17 | zenith | UnitType | [x] | 81ccc48ed695 | standard |  |
| 18 | antumbra | UnitType | [x] | 0f684dc4ae0b | standard |  |
| 19 | eclipse | UnitType | [x] | 4b9578ceafdf | standard |  |
| 20 | mono | UnitType | [x] | 325a072aeacf | standard |  |
| 21 | poly | UnitType | [x] | 3c4d8d547d1c | standard |  |
| 22 | mega | UnitType | [x] | b0b884e93d02 | standard |  |
| 23 | quad | UnitType | [x] | 6592a0fd059b | standard |  |
| 24 | oct | UnitType | [x] | 8a099913fb78 | standard |  |
| 25 | risso | UnitType | [x] | 441336367724 | standard |  |
| 26 | minke | UnitType | [x] | 075851ba7f39 | standard |  |
| 27 | bryde | UnitType | [x] | 8c5fe0e55345 | standard |  |
| 28 | sei | UnitType | [x] | fc19147e0791 | standard |  |
| 29 | omura | UnitType | [x] | dcca963c2e42 | standard |  |
| 30 | retusa | UnitType | [x] | fae99720d87c | standard |  |
| 31 | oxynoe | UnitType | [x] | bdf2d0a91418 | standard |  |
| 32 | cyerce | UnitType | [x] | a465768fcd41 | standard |  |
| 33 | aegires | UnitType | [x] | 5982bf8aad3e | standard |  |
| 34 | navanax | UnitType | [x] | 1c7db1d0e8d2 | standard |  |
| 35 | alpha | UnitType | [x] | 4a003274503f | standard |  |
| 36 | beta | UnitType | [x] | c496d109eb9b | standard |  |
| 37 | gamma | UnitType | [x] | a969ad5b50c5 | standard |  |
| 38 | stell | TankUnitType | [x] | f1293bf76bef | erekir |  |
| 39 | locus | TankUnitType | [x] | 1c0a99940a2b | erekir |  |
| 40 | precept | TankUnitType | [x] | c2e3ef77986a | erekir |  |
| 41 | vanquish | TankUnitType | [x] | 2a8237a960f0 | erekir |  |
| 42 | conquer | TankUnitType | [x] | d02e2faf28a1 | erekir |  |
| 43 | merui | ErekirUnitType | [x] | d63339f44b32 | erekir |  |
| 44 | cleroi | ErekirUnitType | [x] | 73bc2c0e6771 | erekir |  |
| 45 | anthicus | ErekirUnitType | [x] | eb7826d012ce | erekir |  |
| 46 | anthicus-missile | MissileUnitType | [x] | dca4bd621a3b | erekir | inline `spawnUnit` |
| 47 | tecta | ErekirUnitType | [x] | 8e9506627aa1 | erekir |  |
| 48 | collaris | ErekirUnitType | [x] | f5494a59cb5b | erekir |  |
| 49 | elude | ErekirUnitType | [x] | 01975611a401 | erekir |  |
| 50 | avert | ErekirUnitType | [x] | 25ad9aa4e0b6 | erekir |  |
| 51 | obviate | ErekirUnitType | [x] | be32b2d07c8f | erekir |  |
| 52 | quell | ErekirUnitType | [x] | 6f5eee85e20f | erekir |  |
| 53 | quell-missile | MissileUnitType | [x] | 65bb14e4f1fc | erekir | inline `spawnUnit` (nested) |
| 54 | disrupt | ErekirUnitType | [x] | ce54f2cd30b1 | erekir |  |
| 55 | disrupt-missile | MissileUnitType | [x] | c842185df106 | erekir | inline `spawnUnit` |
| 56 | renale | NeoplasmUnitType | [x] | a036e1ae1529 | erekir |  |
| 57 | latum | NeoplasmUnitType | [x] | b94b257b9743 | erekir |  |
| 58 | evoke | ErekirUnitType | [x] | dfb39ce96b74 | erekir |  |
| 59 | incite | ErekirUnitType | [x] | a920c7cbeb90 | erekir |  |
| 60 | emanate | ErekirUnitType | [x] | a7ac461c36cf | erekir |  |
| 61 | block | UnitType | [x] | 33f425777f87 | special |  |
| 62 | manifold | ErekirUnitType | [x] | e8c30caf86b8 | special |  |
| 63 | assembly-drone | ErekirUnitType | [x] | 119b40e83fc8 | special |  |
| 64 | dummy | UnitType | [x] | 67913aff314d | special |  |

- Unported: 0
- Waves: standard 38, erekir 20, special 4
- The `missile` static field is codegen-only (never constructed) and is not a content record.
