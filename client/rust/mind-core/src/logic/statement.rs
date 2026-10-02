// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Statement model (intermediate representation) and registry.
//!
//! Ported from `core/src/mindustry/logic/LStatements.java` (all 53 registered
//! statements; `CommentStatement` is intentionally unregistered upstream).
//! Field order in the table below **is** the text-IO serialization order — it is
//! ABI and must never be reordered or renamed.

use crate::content::ContentType;
use crate::logic::access::LAccess;
use crate::logic::enums::logic_enum;
use crate::logic::enums::*;
use crate::logic::ops::{ConditionOp, LogicOp};

/// One field of a statement (plan-14 editor metadata).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementField {
    /// Field name.
    pub name: &'static str,
    /// Field kind.
    pub kind: FieldKind,
}

/// Static metadata for one registered statement (plan 14 add-dialog + build halves).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementMeta {
    /// Registered name (`@RegisterStatement`).
    pub registered_name: &'static str,
    /// Rust variant name.
    pub rust_variant: &'static str,
    /// Fields in serialization order.
    pub fields: Vec<StatementField>,
    /// Editor category.
    pub category: LCategory,
    /// Privileged statement (world processors only).
    pub privileged: bool,
    /// Statement uses the wrapping editor layout.
    pub use_wrapping: bool,
    /// Hidden from the add dialog.
    pub hidden: bool,
}

impl Statement {
    /// `LStatement.statementKey()` (bundle key stem).
    pub fn statement_key(&self) -> String {
        self.registered_name().to_owned()
    }
}

/// Defines every registered statement with its exact serialized field order.
macro_rules! define_statements {
    ($(
        $variant:ident, $reg:literal, $cat:ident, $priv:literal, $wrap:literal, $hidden:literal {
            $( $f:ident : $t:ty = $d:expr ),* $(,)?
        }
    );* $(;)?) => {
        /// A parsed logic statement. One variant per registered class.
        #[derive(Clone, Debug, PartialEq)]
        pub enum Statement {
            $(
                #[allow(missing_docs)]
                $variant {
                    $( $f: $t ),*
                },
            )*
        }

        impl Statement {
            /// Registered name (`LogicIO` write prefix).
            pub fn registered_name(&self) -> &'static str {
                match self { $( Statement::$variant { .. } => $reg ),* }
            }

            /// Editor category.
            pub fn category(&self) -> LCategory {
                match self { $( Statement::$variant { .. } => LCategory::$cat ),* }
            }

            /// `LStatement.privileged()`.
            pub fn privileged(&self) -> bool {
                match self { $( Statement::$variant { .. } => $priv ),* }
            }

            /// `LStatement.useWrapping()`.
            pub fn use_wrapping(&self) -> bool {
                match self { $( Statement::$variant { .. } => $wrap ),* }
            }

            /// `LStatement.hidden()`.
            pub fn hidden(&self) -> bool {
                match self { $( Statement::$variant { .. } => $hidden ),* }
            }

            /// Field names + kinds in serialization order.
            pub fn fields(&self) -> Vec<StatementField> {
                match self {
                    $(
                        Statement::$variant { .. } => vec![
                            $( StatementField { name: stringify!($f), kind: <$t as MlogField>::KIND } ),*
                        ],
                    )*
                }
            }

            /// Appends `' ' + field.encoded()` for each field (after the name).
            pub fn write_fields(&self, out: &mut String) {
                match self {
                    $(
                        Statement::$variant { $( $f ),* } => {
                            $( out.push(' '); MlogField::write($f, out); )*
                        }
                    )*
                }
            }
        }

        /// Field names + kinds in serialization order.
        pub fn statement_fields(name: &str) -> Option<Vec<StatementField>> {
            default_statement(name).map(|s| s.fields())
        }

        /// Builds a statement populated entirely with upstream defaults.
        pub fn default_statement(name: &str) -> Option<Statement> {
            match name {
                $( $reg => Some(Statement::$variant { $( $f: $d ),* }), )*
                _ => None,
            }
        }

        /// Constructs a statement from `tokens[0]` = name and `tokens[1..]` = fields.
        ///
        /// Mirrors generated `LogicIO.read`: constructs defaults, assigns each
        /// present token, and returns `None` on an unknown name or invalid field.
        #[allow(unused_assignments, unused_variables)]
        pub fn read_statement(tokens: &[&str]) -> Option<Statement> {
            let name = *tokens.first()?;
            match name {
                $(
                    $reg => {
                        #[allow(unused_mut, unused_assignments, unused_variables)]
                        let mut idx = 1usize;
                        let mut st = Statement::$variant {
                            $(
                                $f: {
                                    let value = if idx < tokens.len() {
                                        match <$t as MlogField>::parse(tokens[idx]) {
                                            Ok(v) => v,
                                            Err(()) => return None,
                                        }
                                    } else {
                                        $d
                                    };
                                    idx += 1;
                                    value
                                },
                            )*
                        };
                        st.after_read();
                        Some(st)
                    }
                )*
                _ => None,
            }
        }

        /// All registered statements in declaration/serialization order.
        pub fn all_statements() -> Vec<StatementMeta> {
            vec![
                $(
                    StatementMeta {
                        registered_name: $reg,
                        rust_variant: stringify!($variant),
                        fields: statement_fields($reg).expect("registered"),
                        category: LCategory::$cat,
                        privileged: $priv,
                        use_wrapping: $wrap,
                        hidden: $hidden,
                    },
                )*
            ]
        }

        /// Number of registered statements.
        pub const STATEMENT_COUNT: usize = 0 $( + { let _ = stringify!($reg); 1 } )*;
    };
}

define_statements! {
    Invalid, "noop", Unknown, false, true, false {};
    Read, "read", Io, false, true, false {
        output: String = "result".to_string(),
        target: String = "cell1".to_string(),
        address: String = "0".to_string()
    };
    Write, "write", Io, false, true, false {
        input: String = "result".to_string(),
        target: String = "cell1".to_string(),
        address: String = "0".to_string()
    };
    Draw, "draw", Io, false, true, false {
        type_: GraphicsType = GraphicsType::Clear,
        x: String = "0".to_string(),
        y: String = "0".to_string(),
        p1: String = "0".to_string(),
        p2: String = "0".to_string(),
        p3: String = "0".to_string(),
        p4: String = "0".to_string()
    };
    Print, "print", Io, false, true, false {
        value: String = "\"frog\"".to_string()
    };
    PrintChar, "printchar", Io, false, true, false {
        value: String = "65".to_string()
    };
    Format, "format", Io, false, true, false {
        value: String = "\"frog\"".to_string()
    };
    DrawFlush, "drawflush", Block, false, true, false {
        target: String = "display1".to_string()
    };
    PrintFlush, "printflush", Block, false, true, false {
        target: String = "message1".to_string()
    };
    GetLink, "getlink", Block, false, true, false {
        output: String = "result".to_string(),
        address: String = "0".to_string()
    };
    Control, "control", Block, false, true, false {
        type_: LAccess = LAccess::Enabled,
        target: String = "block1".to_string(),
        p1: String = "0".to_string(),
        p2: String = "0".to_string(),
        p3: String = "0".to_string(),
        p4: String = "0".to_string()
    };
    Radar, "radar", Block, false, true, false {
        target1: RadarTarget = RadarTarget::Enemy,
        target2: RadarTarget = RadarTarget::Any,
        target3: RadarTarget = RadarTarget::Any,
        sort: RadarSort = RadarSort::Distance,
        radar: String = "turret1".to_string(),
        sort_order: String = "1".to_string(),
        output: String = "result".to_string()
    };
    Sensor, "sensor", Block, false, true, false {
        to: String = "result".to_string(),
        from: String = "block1".to_string(),
        type_: String = "@copper".to_string()
    };
    Set, "set", Operation, false, true, false {
        to: String = "result".to_string(),
        from: String = "0".to_string()
    };
    Operation, "op", Operation, false, true, false {
        op: LogicOp = LogicOp::Add,
        dest: String = "result".to_string(),
        a: String = "a".to_string(),
        b: String = "b".to_string()
    };
    Select, "select", Operation, false, true, false {
        result: String = "result".to_string(),
        op: ConditionOp = ConditionOp::NotEqual,
        comp0: String = "x".to_string(),
        comp1: String = "false".to_string(),
        a: String = "a".to_string(),
        b: String = "b".to_string()
    };
    Wait, "wait", Control, false, true, false {
        value: String = "0.5".to_string()
    };
    Stop, "stop", Control, false, true, false {};
    Lookup, "lookup", Operation, false, true, false {
        type_: ContentType = ContentType::Item,
        result: String = "result".to_string(),
        id: String = "0".to_string()
    };
    PackColor, "packcolor", Operation, false, true, false {
        result: String = "result".to_string(),
        r: String = "1".to_string(),
        g: String = "0".to_string(),
        b: String = "0".to_string(),
        a: String = "1".to_string()
    };
    UnpackColor, "unpackcolor", Operation, false, true, false {
        r: String = "r".to_string(),
        g: String = "g".to_string(),
        b: String = "b".to_string(),
        a: String = "a".to_string(),
        value: String = "color".to_string()
    };
    End, "end", Control, false, true, false {};
    Jump, "jump", Control, false, false, false {
        dest_index: i32 = 0,
        op: ConditionOp = ConditionOp::NotEqual,
        value: String = "x".to_string(),
        compare: String = "false".to_string()
    };
    UnitBind, "ubind", Unit, false, true, false {
        type_: String = "@poly".to_string()
    };
    UnitControl, "ucontrol", Unit, false, true, false {
        type_: LUnitControl = LUnitControl::Move,
        p1: String = "0".to_string(),
        p2: String = "0".to_string(),
        p3: String = "0".to_string(),
        p4: String = "0".to_string(),
        p5: String = "0".to_string()
    };
    UnitRadar, "uradar", Unit, false, true, false {
        target1: RadarTarget = RadarTarget::Enemy,
        target2: RadarTarget = RadarTarget::Any,
        target3: RadarTarget = RadarTarget::Any,
        sort: RadarSort = RadarSort::Distance,
        radar: String = "0".to_string(),
        sort_order: String = "1".to_string(),
        output: String = "result".to_string()
    };
    UnitLocate, "ulocate", Unit, false, true, false {
        locate: LLocate = LLocate::Building,
        flag: BlockFlag = BlockFlag::Core,
        enemy: String = "true".to_string(),
        ore: String = "@copper".to_string(),
        out_x: String = "outx".to_string(),
        out_y: String = "outy".to_string(),
        out_found: String = "found".to_string(),
        out_build: String = "building".to_string()
    };
    Query, "query", World, true, true, false {
        shape: QueryShape = QueryShape::Circle,
        type_: QueryType = QueryType::Unit,
        team: String = "null".to_string(),
        x: String = "0".to_string(),
        y: String = "0".to_string(),
        w: String = "10".to_string(),
        h: String = "10".to_string()
    };
    GetBlock, "getblock", World, true, true, false {
        layer: TileLayer = TileLayer::Block,
        result: String = "result".to_string(),
        x: String = "0".to_string(),
        y: String = "0".to_string()
    };
    SetBlock, "setblock", World, true, true, false {
        layer: TileLayer = TileLayer::Block,
        block: String = "@air".to_string(),
        x: String = "0".to_string(),
        y: String = "0".to_string(),
        team: String = "@derelict".to_string(),
        rotation: String = "0".to_string()
    };
    SpawnUnit, "spawn", World, true, true, false {
        type_: String = "@dagger".to_string(),
        x: String = "10".to_string(),
        y: String = "10".to_string(),
        rotation: String = "90".to_string(),
        team: String = "@sharded".to_string(),
        result: String = "result".to_string(),
        effect: String = "true".to_string()
    };
    SpawnBullet, "bullet", World, true, true, false {
        result: String = "result".to_string(),
        from: String = "@dagger".to_string(),
        index: String = "0".to_string(),
        x: String = "x".to_string(),
        y: String = "y".to_string(),
        rotation: String = "angle".to_string(),
        team: String = "null".to_string(),
        owner: String = "null".to_string(),
        damage: String = "-1".to_string(),
        velocity_scl: String = "1".to_string(),
        life_scl: String = "1".to_string(),
        aim_x: String = "-1".to_string(),
        aim_y: String = "-1".to_string()
    };
    ApplyStatus, "status", World, true, true, false {
        clear: bool = false,
        effect: String = "@status-wet".to_string(),
        unit: String = "unit".to_string(),
        duration: String = "10".to_string()
    };
    WeatherSense, "weathersense", World, true, true, false {
        to: String = "result".to_string(),
        weather: String = "@rain".to_string()
    };
    WeatherSet, "weatherset", World, true, true, false {
        weather: String = "@rain".to_string(),
        state: String = "true".to_string()
    };
    SpawnWave, "spawnwave", World, true, true, false {
        x: String = "10".to_string(),
        y: String = "10".to_string(),
        natural: String = "false".to_string()
    };
    SetRule, "setrule", World, true, true, false {
        rule: LogicRule = LogicRule::WaveSpacing,
        value: String = "10".to_string(),
        p1: String = "0".to_string(),
        p2: String = "0".to_string(),
        p3: String = "100".to_string(),
        p4: String = "100".to_string()
    };
    FlushMessage, "message", World, true, true, false {
        type_: MessageType = MessageType::Announce,
        duration: String = "3".to_string(),
        out_success: String = "@wait".to_string()
    };
    Cutscene, "cutscene", World, true, true, false {
        action: CutsceneAction = CutsceneAction::Pan,
        p1: String = "100".to_string(),
        p2: String = "100".to_string(),
        p3: String = "0.06".to_string(),
        p4: String = "0".to_string()
    };
    Effect, "effect", World, true, true, false {
        type_: String = "warn".to_string(),
        x: String = "0".to_string(),
        y: String = "0".to_string(),
        sizerot: String = "2".to_string(),
        color: String = "%ffaaff".to_string(),
        data: String = "".to_string()
    };
    Explosion, "explosion", World, true, true, false {
        team: String = "@crux".to_string(),
        x: String = "0".to_string(),
        y: String = "0".to_string(),
        radius: String = "5".to_string(),
        damage: String = "50".to_string(),
        air: String = "true".to_string(),
        ground: String = "true".to_string(),
        pierce: String = "false".to_string(),
        effect: String = "true".to_string()
    };
    SetRate, "setrate", Control, false, true, false {
        amount: String = "10".to_string()
    };
    Fetch, "fetch", World, true, true, false {
        type_: FetchType = FetchType::Unit,
        result: String = "result".to_string(),
        team: String = "@sharded".to_string(),
        index: String = "0".to_string(),
        extra: String = "@conveyor".to_string()
    };
    Sync, "sync", World, true, true, false {
        variable: String = "var".to_string()
    };
    ClientData, "clientdata", World, true, true, true {
        channel: String = "\"frog\"".to_string(),
        value: String = "\"bar\"".to_string(),
        reliable: String = "0".to_string()
    };
    GetFlag, "getflag", World, true, true, false {
        result: String = "result".to_string(),
        flag: String = "\"flag\"".to_string()
    };
    SetFlag, "setflag", World, true, true, false {
        flag: String = "\"flag\"".to_string(),
        value: String = "true".to_string()
    };
    SetProp, "setprop", World, true, true, false {
        type_: String = "@copper".to_string(),
        of: String = "block1".to_string(),
        value: String = "0".to_string()
    };
    PlaySound, "playsound", World, true, true, false {
        positional: bool = false,
        id: String = "@sfx-shoot".to_string(),
        volume: String = "1".to_string(),
        pitch: String = "1".to_string(),
        pan: String = "0".to_string(),
        x: String = "@thisx".to_string(),
        y: String = "@thisy".to_string(),
        limit: String = "true".to_string()
    };
    PlayMusic, "playmusic", World, true, true, false {
        name: String = "\"game1\"".to_string(),
        interrupt: String = "true".to_string()
    };
    SetMarker, "setmarker", World, true, true, false {
        type_: LMarkerControl = LMarkerControl::Pos,
        id: String = "0".to_string(),
        p1: String = "0".to_string(),
        p2: String = "0".to_string(),
        p3: String = "0".to_string()
    };
    MakeMarker, "makemarker", World, true, true, false {
        type_: String = "shape".to_string(),
        id: String = "0".to_string(),
        x: String = "0".to_string(),
        y: String = "0".to_string(),
        replace: String = "true".to_string()
    };
    LocalePrint, "localeprint", World, true, true, false {
        value: String = "\"name\"".to_string()
    };
}

// `LogicRule` port (`core/src/mindustry/logic/LogicRule.java`).
logic_enum!(
    /// World-rule identifiers for `setrule`.
    LogicRule {
        CurrentWaveTime => "currentWaveTime",
        WaveTimer => "waveTimer",
        Waves => "waves",
        Wave => "wave",
        WaveSpacing => "waveSpacing",
        WaveSending => "waveSending",
        AttackMode => "attackMode",
        EnemyCoreBuildRadius => "enemyCoreBuildRadius",
        DropZoneRadius => "dropZoneRadius",
        UnitCap => "unitCap",
        MapArea => "mapArea",
        Lighting => "lighting",
        CanGameOver => "canGameOver",
        AmbientLight => "ambientLight",
        UnitLight => "unitLight",
        SolarMultiplier => "solarMultiplier",
        DragMultiplier => "dragMultiplier",
        Ban => "ban",
        Unban => "unban",
        PauseDisabled => "pauseDisabled",
        MusicVolume => "musicVolume",
        BuildSpeed => "buildSpeed",
        UnitHealth => "unitHealth",
        UnitBuildSpeed => "unitBuildSpeed",
        UnitMineSpeed => "unitMineSpeed",
        UnitCost => "unitCost",
        UnitDamage => "unitDamage",
        BlockHealth => "blockHealth",
        BlockDamage => "blockDamage",
        RtsMinWeight => "rtsMinWeight",
        RtsMinSquad => "rtsMinSquad"
    }
);

impl Statement {
    /// `LStatement.afterRead()` post-processing.
    pub fn after_read(&mut self) {
        if let Statement::Draw { type_, p1, p2, .. } = self {
            if *type_ == GraphicsType::Color && p2 == "0" {
                *p2 = "255".to_owned();
            }
            if *type_ == GraphicsType::Print && name_to_align(p1).is_some() {
                *p1 = format!("@{p1}");
            }
        }
    }

    /// `LStatement.sanitize` — makes user text-field input safe for the parser.
    pub fn sanitize(value: &str) -> String {
        if value.is_empty() {
            return String::new();
        }
        if value.chars().count() == 1
            && matches!(
                value.chars().next(),
                Some('"' | ';' | ' ' | '\n' | '\t' | '#')
            )
        {
            return "invalid".to_owned();
        }

        let mut res = String::with_capacity(value.len());
        let bytes = value.as_bytes();
        if bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
            res.push('"');
            let chars: Vec<char> = value.chars().collect();
            let mut i = 1;
            while i < chars.len() - 1 {
                let c = chars[i];
                if c == '\\' && i + 1 < chars.len() - 1 {
                    let next = chars[i + 1];
                    if next == '"' || next == '\\' || next == 'n' {
                        res.push(c);
                        res.push(next);
                        i += 2;
                        continue;
                    }
                    if next == 'u' && i + 5 < chars.len() - 1 && is_hex(&chars, i + 2) {
                        res.extend(chars[i..i + 6].iter());
                        i += 6;
                        continue;
                    }
                }
                match c {
                    '"' => res.push_str("\\\""),
                    '\\' => res.push_str("\\\\"),
                    '\n' => res.push_str("\\n"),
                    _ => res.push(c),
                }
                i += 1;
            }
            res.push('"');
        } else {
            for c in value.chars() {
                match c {
                    ';' => res.push('s'),
                    '"' => res.push('\''),
                    ' ' | '\t' | '\n' | '#' => res.push('_'),
                    _ => res.push(c),
                }
            }
        }
        res
    }

    /// `LStatement.copy()` — `write` then a privileged `read`; `None` if empty.
    pub fn copy(&self) -> Option<Statement> {
        let mut out = String::new();
        self.write(&mut out);
        let mut tokens: Vec<&str> = out.split(' ').collect();
        // `write` starts with the name; drop empty trailing token if present.
        tokens.retain(|t| !t.is_empty());
        let st = read_statement(&tokens)?;
        Some(st)
    }

    /// Writes the statement (name + fields, no trailing newline).
    pub fn write(&self, out: &mut String) {
        out.push_str(self.registered_name());
        self.write_fields(out);
    }
}

/// `LStatement.nameToAlign` lookup.
pub fn name_to_align(name: &str) -> Option<i32> {
    let value = match name {
        "center" => 1,
        "top" => 2,
        "bottom" => 4,
        "left" => 8,
        "right" => 16,
        "topLeft" => 3,
        "topRight" => 18,
        "bottomLeft" => 12,
        "bottomRight" => 20,
        _ => return None,
    };
    Some(value)
}

fn is_hex(chars: &[char], from: usize) -> bool {
    if from + 4 > chars.len() {
        return false;
    }
    chars[from..from + 4].iter().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_table() {
        assert_eq!(Statement::sanitize("\""), "invalid");
        assert_eq!(Statement::sanitize(";"), "invalid");
        assert_eq!(Statement::sanitize(" "), "invalid");
        assert_eq!(Statement::sanitize("a"), "a");
        assert_eq!(Statement::sanitize(""), "");
        assert_eq!(Statement::sanitize("\"hello\""), "\"hello\"");
        assert_eq!(Statement::sanitize("\"hello\\\""), "\"hello\\\\\"");
        assert_eq!(Statement::sanitize("\"a\"b\""), "\"a\\\"b\"");
        assert_eq!(Statement::sanitize("\"C:\\Users\""), "\"C:\\\\Users\"");
        assert_eq!(Statement::sanitize("\"line1\nline2\""), "\"line1\\nline2\"");
        assert_eq!(Statement::sanitize("\"a\\b\"c\nd\""), "\"a\\\\b\\\"c\\nd\"");
        assert_eq!(
            Statement::sanitize("hello world;test\"quote"),
            "hello_worldstest'quote"
        );
        assert_eq!(Statement::sanitize("a\nb\nc"), "a_b_c");
        assert_eq!(Statement::sanitize("a\tb"), "a_b");
        assert_eq!(Statement::sanitize("a#b"), "a_b");
        assert_eq!(Statement::sanitize("\n"), "invalid");
        assert_eq!(Statement::sanitize("\t"), "invalid");
        assert_eq!(Statement::sanitize("#"), "invalid");
        assert_eq!(Statement::sanitize("\"\\u0041\""), "\"\\u0041\"");
        assert_eq!(
            Statement::sanitize("\"\\u0041\\n\\u0042\\\\end\""),
            "\"\\u0041\\n\\u0042\\\\end\""
        );
        assert_eq!(Statement::sanitize("\"\\u12\""), "\"\\\\u12\"");
        assert_eq!(Statement::sanitize("\"\\u12zz\""), "\"\\\\u12zz\"");
        assert_eq!(Statement::sanitize("\"a\\u123\""), "\"a\\\\u123\"");
    }

    #[test]
    fn statement_count_is_53() {
        assert_eq!(STATEMENT_COUNT, 53);
        assert_eq!(all_statements().len(), 53);
    }

    #[test]
    fn registered_names_are_unique() {
        let mut names: Vec<&str> = all_statements().iter().map(|m| m.registered_name).collect();
        names.sort_unstable();
        let len = names.len();
        names.dedup();
        assert_eq!(names.len(), len);
    }

    #[test]
    fn default_and_read_roundtrip_one() {
        let st = default_statement("set").unwrap();
        assert_eq!(st.registered_name(), "set");
        let mut out = String::new();
        st.write(&mut out);
        assert_eq!(out, "set result 0");
    }
}
