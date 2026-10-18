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
    /// Java enum names for `FieldKind::Enum` fields (editor `showSelect`).
    pub options: &'static [&'static str],
    /// Java enum class name for `FieldKind::Enum` fields (editor labels).
    pub enum_class: &'static str,
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
                            $(
                                StatementField {
                                    name: stringify!($f),
                                    kind: <$t as MlogField>::KIND,
                                    options: <$t as MlogField>::OPTIONS,
                                    enum_class: <$t as MlogField>::ENUM_CLASS,
                                }
                            ),*
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

            /// Encoded field values in serialization order (the editor's field
            /// texts; `read_statement([name, ..values])` rebuilds the statement).
            pub fn field_values(&self) -> Vec<String> {
                match self {
                    $(
                        Statement::$variant { $( $f ),* } => vec![
                            $(
                                {
                                    let mut text = String::new();
                                    MlogField::write($f, &mut text);
                                    text
                                }
                            ),*
                        ],
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

impl LogicRule {
    /// Rules whose rebuild shows the `of`/value row (`SetRuleStatement.rebuild`,
    /// `LStatements.java:1653-1660`).
    pub const fn uses_team(self) -> bool {
        use LogicRule::*;
        matches!(
            self,
            BuildSpeed
                | UnitHealth
                | UnitBuildSpeed
                | UnitMineSpeed
                | UnitCost
                | UnitDamage
                | BlockHealth
                | BlockDamage
                | RtsMinSquad
                | RtsMinWeight
        )
    }
}

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

/// `lst.<key>` bundle stem for a registered statement name
/// (`LStatement.name()` = the Java class name without `Statement`; the write
/// token differs for these; `noop`/`clientdata` have no bundle row).
pub fn bundle_key(name: &str) -> String {
    match name {
        "op" => "operation".to_owned(),
        "ubind" => "unitbind".to_owned(),
        "ucontrol" => "unitcontrol".to_owned(),
        "uradar" => "unitradar".to_owned(),
        "ulocate" => "unitlocate".to_owned(),
        "spawn" => "spawnunit".to_owned(),
        "bullet" => "spawnbullet".to_owned(),
        "status" => "applystatus".to_owned(),
        "message" => "flushmessage".to_owned(),
        other => other.to_owned(),
    }
}

/// How one editor row item renders (`LStatement.build(Table)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorKind {
    /// Static label: `bundle(desc)` (`name.token.<label>`, raw fallback).
    Text,
    /// Text field (`LStatement.field`).
    Field,
    /// Enum select button (`LStatement.showSelect`).
    Select,
    /// Colour pencil (`LStatement.col`).
    Color,
    /// Alignment pencil (`LStatement.fieldAlignSelect`).
    Align,
    /// Character picker (`PrintCharStatement`'s glyph grid).
    Char,
    /// Statement target (`JumpStatement`'s `JumpButton`).
    Jump,
}

/// One item in a statement's editor row, in `build(Table)` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorItem {
    /// Item kind.
    pub kind: EditorKind,
    /// Field index in `Statement::field_values` (`0` for [`EditorKind::Text`]).
    pub index: usize,
    /// Label token: `name.token.<label>` with the token as fallback.
    pub label: &'static str,
    /// Label before the editor (`fields(desc, ...)`) or after it (`nameAfterField`).
    pub label_before: bool,
    /// Wide field (`LCanvas.getTargetWidth() - Scl.scl(20f)`).
    pub wide: bool,
}

/// `Layer` constants in `Layer.java` declaration order.
///
/// `SetMarkerStatement.rebuild` reflects every public static float field of
/// `Layer` into the draw-layer list (`LStatements.java:2415-2428`), so the
/// order and values are the display ABI.
pub const MARKER_LAYERS: &[(&str, f32)] = &[
    ("min", -11.0),
    ("background", -10.0),
    ("floor", 0.0),
    ("scorch", 10.0),
    ("debris", 20.0),
    ("blockUnder", 29.5),
    ("block", 30.0),
    ("blockCracks", 30.1),
    ("blockAfterCracks", 30.2),
    ("blockAdditive", 31.0),
    ("blockProp", 32.0),
    ("blockOver", 35.0),
    ("blockBuilding", 40.0),
    ("turret", 50.0),
    ("turretHeat", 50.1),
    ("groundUnit", 60.0),
    ("power", 70.0),
    ("legUnit", 75.0),
    ("darkness", 80.0),
    ("plans", 85.0),
    ("flyingUnitLow", 90.0),
    ("bullet", 100.0),
    ("effect", 110.0),
    ("flyingUnit", 115.0),
    ("overlayUI", 120.0),
    ("buildBeam", 122.0),
    ("shields", 125.0),
    ("weather", 130.0),
    ("light", 140.0),
    ("playerName", 150.0),
    ("fogOfWar", 155.0),
    ("space", 160.0),
    ("end", 200.0),
    ("endPixeled", 210.0),
    ("max", 220.0),
];

/// One option of an editor enum select (`LStatement.showSelect`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorOption {
    /// Serialized value (`Enum.name()`; `true`/`false` for bool toggles).
    pub name: String,
    /// Display fallback (`Enum.name()`, `LogicOp.symbol`, ...).
    pub label: String,
    /// Bundle key (`<class>.label.<name>`, `name.token.<token>`); empty = none.
    pub key: String,
}

const fn e_text(label: &'static str) -> EditorItem {
    EditorItem {
        kind: EditorKind::Text,
        index: 0,
        label,
        label_before: true,
        wide: false,
    }
}

const fn e_field(index: usize) -> EditorItem {
    EditorItem {
        kind: EditorKind::Field,
        index,
        label: "",
        label_before: true,
        wide: false,
    }
}

const fn e_labeled(index: usize, label: &'static str) -> EditorItem {
    EditorItem {
        kind: EditorKind::Field,
        index,
        label,
        label_before: true,
        wide: false,
    }
}

const fn e_wide(index: usize) -> EditorItem {
    EditorItem {
        kind: EditorKind::Field,
        index,
        label: "",
        label_before: true,
        wide: true,
    }
}

const fn e_select(index: usize) -> EditorItem {
    EditorItem {
        kind: EditorKind::Select,
        index,
        label: "",
        label_before: true,
        wide: false,
    }
}

const fn e_color(index: usize) -> EditorItem {
    EditorItem {
        kind: EditorKind::Color,
        index,
        label: "",
        label_before: true,
        wide: false,
    }
}

const fn e_align(index: usize) -> EditorItem {
    EditorItem {
        kind: EditorKind::Align,
        index,
        label: "",
        label_before: true,
        wide: false,
    }
}

const fn e_char(index: usize) -> EditorItem {
    EditorItem {
        kind: EditorKind::Char,
        index,
        label: "",
        label_before: true,
        wide: false,
    }
}

const fn e_jump(index: usize) -> EditorItem {
    EditorItem {
        kind: EditorKind::Jump,
        index,
        label: "",
        label_before: true,
        wide: false,
    }
}

/// `LStatement.build(Table)` editor row for a statement's current values.
///
/// Statements not yet transcribed fall back to one labelled text field per
/// serialized field (the generic builder).
pub fn editor_layout(name: &str, values: &[String]) -> Vec<EditorItem> {
    let generic = || -> Vec<EditorItem> {
        default_statement(name)
            .map(|statement| {
                statement
                    .fields()
                    .iter()
                    .enumerate()
                    .map(|(index, field)| e_labeled(index, field.name))
                    .collect()
            })
            .unwrap_or_default()
    };
    let value = |index: usize| values.get(index).map(String::as_str).unwrap_or("");
    match name {
        // `ReadStatement.build` (`LStatements.java:66-71`).
        "read" => vec![
            e_text("read"),
            e_field(0),
            e_text("="),
            e_field(1),
            e_text("at"),
            e_field(2),
        ],
        // `WriteStatement.build` (`:96-101`).
        "write" => vec![
            e_text("write"),
            e_field(0),
            e_text("to"),
            e_field(1),
            e_text("at"),
            e_field(2),
        ],
        // `DrawStatement.rebuild` (`:133-260`).
        "draw" => {
            let mut out = vec![e_select(0)];
            match value(0) {
                "clear" => out.extend([e_labeled(1, "r"), e_labeled(2, "g"), e_labeled(3, "b")]),
                "color" => out.extend([
                    e_labeled(1, "r"),
                    e_labeled(2, "g"),
                    e_labeled(3, "b"),
                    e_labeled(4, "a"),
                ]),
                "col" => {
                    out.push(e_labeled(1, "color"));
                    out.push(e_color(1));
                }
                "stroke" => out.push(e_field(1)),
                "line" => out.extend([
                    e_labeled(1, "x"),
                    e_labeled(2, "y"),
                    e_labeled(3, "x2"),
                    e_labeled(4, "y2"),
                ]),
                "rect" | "lineRect" => out.extend([
                    e_labeled(1, "x"),
                    e_labeled(2, "y"),
                    e_labeled(3, "width"),
                    e_labeled(4, "height"),
                ]),
                "poly" | "linePoly" => out.extend([
                    e_labeled(1, "x"),
                    e_labeled(2, "y"),
                    e_labeled(3, "sides"),
                    e_labeled(4, "radius"),
                    e_labeled(5, "rotation"),
                ]),
                "triangle" => out.extend([
                    e_labeled(1, "x"),
                    e_labeled(2, "y"),
                    e_labeled(3, "x2"),
                    e_labeled(4, "y2"),
                    e_labeled(5, "x3"),
                    e_labeled(6, "y3"),
                ]),
                "image" => out.extend([
                    e_labeled(1, "x"),
                    e_labeled(2, "y"),
                    e_labeled(3, "image"),
                    e_labeled(4, "size"),
                    e_labeled(5, "rotation"),
                ]),
                "print" => {
                    out.extend([e_labeled(1, "x"), e_labeled(2, "y"), e_labeled(3, "align")]);
                    out.push(e_align(3));
                }
                "translate" | "scale" => out.extend([e_labeled(1, "x"), e_labeled(2, "y")]),
                "rotate" => out.push(e_labeled(3, "angle")),
                _ => return generic(),
            }
            out
        }
        // `PrintStatement.build`/`FormatStatement.build` (`:267-269`, `:328-330`).
        "print" | "format" => vec![e_wide(0)],
        // `PrintCharStatement.build` (`:288-321`).
        "printchar" => vec![e_text("char"), e_field(0), e_char(0)],
        // `DrawFlushStatement.build`/`PrintFlushStatement.build` (`:349-351`, `:370-372`).
        "drawflush" | "printflush" => vec![e_text("to"), e_field(0)],
        // `GetLinkStatement.build` (`:391-402`).
        "getlink" => vec![e_field(0), e_text("="), e_text("linknum"), e_field(1)],
        // `ControlStatement.rebuild` (`:422-455`).
        "control" => {
            let mut out = vec![e_text("set"), e_select(0), e_text("of"), e_field(1)];
            let access = LAccess::from_name(value(0)).unwrap_or(LAccess::Enabled);
            for (i, &param) in access.params().iter().enumerate() {
                out.push(e_labeled(2 + i, param));
            }
            out
        }
        // `RadarStatement.build` (`:466-521`).
        "radar" => vec![
            e_field(6),
            e_text("="),
            e_labeled(4, "from"),
            e_select(0),
            e_select(1),
            e_select(2),
            e_labeled(5, "order"),
            e_text("sort"),
            e_select(3),
        ],
        // `SensorStatement.build` (`:532-648`).
        "sensor" => vec![
            e_field(0),
            e_text("="),
            e_field(2),
            e_text("in"),
            e_field(1),
        ],
        // `SetStatement.build` (`:656-661`).
        "set" => vec![e_field(0), e_text("="), e_field(1)],
        // `OperationStatement.rebuild` (`:685-737`): unary / "function" / binary.
        "op" => {
            let op = LogicOp::from_name(value(0)).unwrap_or(LogicOp::Add);
            let mut out = vec![e_field(1), e_text("=")];
            if op.is_unary() {
                out.push(e_select(0));
                out.push(e_field(2));
            } else if op.is_func() {
                out.push(e_select(0));
                out.push(e_field(2));
                out.push(e_field(3));
            } else {
                out.push(e_field(2));
                out.push(e_select(0));
                out.push(e_field(3));
            }
            out
        }
        // `SelectStatement.rebuild` (`:750-780`).
        "select" => {
            let op = ConditionOp::from_name(value(1)).unwrap_or(ConditionOp::NotEqual);
            let mut out = vec![e_field(0), e_text("="), e_text("if")];
            if op != ConditionOp::Always {
                out.push(e_field(2));
            }
            out.push(e_select(1));
            if op != ConditionOp::Always {
                out.push(e_field(3));
            }
            out.push(e_text("then"));
            out.push(e_field(4));
            out.push(e_text("else"));
            out.push(e_field(5));
            out
        }
        // `WaitStatement.build` (`:787-790`).
        "wait" => vec![e_field(0), e_text("seconds")],
        // `StopStatement.build`/`EndStatement.build`: no rows (`:807`, `:913`).
        "stop" | "end" => Vec::new(),
        // `LookupStatement.build` (`:827-853`).
        "lookup" => vec![
            e_field(1),
            e_text("-lookup"),
            e_select(0),
            e_text(" # "),
            e_field(2),
        ],
        // `PackColorStatement.build` (`:860-881`).
        "packcolor" => vec![
            e_field(0),
            e_text("="),
            e_text("pack"),
            e_field(1),
            e_field(2),
            e_field(3),
            e_field(4),
        ],
        // `UnpackColorStatement.build` (`:888-909`).
        "unpackcolor" => vec![
            e_field(0),
            e_field(1),
            e_field(2),
            e_field(3),
            e_text("="),
            e_text("unpack"),
            e_field(4),
        ],
        // `JumpStatement.build` (`:946-968`).
        "jump" => {
            let op = ConditionOp::from_name(value(1)).unwrap_or(ConditionOp::NotEqual);
            let mut out = vec![e_text("if")];
            if op != ConditionOp::Always {
                out.push(e_field(2));
            }
            out.push(e_select(1));
            if op != ConditionOp::Always {
                out.push(e_field(3));
            }
            out.push(e_jump(0));
            out
        }
        // `UnitBindStatement.build` (`:1024-1060`); the unit picker is a
        // content button in Java (not ported).
        "ubind" => vec![e_text("type"), e_field(0)],
        // `UnitControlStatement.build` (`:1068-1101`): one row per
        // `LUnitControl.params` entry.
        "ucontrol" => {
            let mut out = vec![e_select(0)];
            let control = LUnitControl::from_name(value(0)).unwrap_or(LUnitControl::Move);
            for (i, &param) in control.params().iter().enumerate() {
                out.push(e_labeled(1 + i, param));
            }
            out
        }
        // `UnitRadarStatement` = `RadarStatement.build` without the `from` field
        // (`:1103-1125`).
        "uradar" => vec![
            e_field(6),
            e_text("="),
            e_select(0),
            e_select(1),
            e_select(2),
            e_labeled(5, "order"),
            e_text("sort"),
            e_select(3),
        ],
        // `UnitLocateStatement.build` (`:1135-1215`).
        "ulocate" => {
            let locate = LLocate::from_name(value(0)).unwrap_or(LLocate::Building);
            let mut out = vec![e_text("find"), e_select(0)];
            match locate {
                LLocate::Building => {
                    out.push(e_text("group"));
                    out.push(e_select(1));
                    out.push(e_labeled(2, "enemy"));
                }
                LLocate::Ore => out.push(e_labeled(3, "ore")),
                LLocate::Spawn | LLocate::Damaged => {}
            }
            out.push(e_labeled(4, "outX"));
            out.push(e_labeled(5, "outY"));
            out.push(e_labeled(6, "found"));
            if locate != LLocate::Ore {
                out.push(e_labeled(7, "building"));
            }
            out
        }
        // `QueryStatement.build` (`:1224-1267`).
        "query" => {
            let shape = QueryShape::from_name(value(0)).unwrap_or(QueryShape::Circle);
            let mut out = vec![
                e_select(0),
                e_select(1),
                e_labeled(2, "team"),
                e_labeled(3, "x"),
                e_labeled(4, "y"),
            ];
            if shape == QueryShape::Circle {
                out.push(e_labeled(5, "radius"));
            } else {
                out.push(e_labeled(5, "width"));
                out.push(e_labeled(6, "height"));
            }
            out
        }
        // `GetBlockStatement.build` (`:1275-1303`).
        "getblock" => vec![
            e_field(1),
            e_text(" ="),
            e_select(0),
            e_labeled(2, "x"),
            e_labeled(3, "y"),
        ],
        // `SetBlockStatement.rebuild` (`:1315-1354`).
        "setblock" => {
            let layer = TileLayer::from_name(value(0)).unwrap_or(TileLayer::Block);
            let mut out = vec![
                e_text("set"),
                e_select(0),
                e_labeled(2, "x"),
                e_labeled(3, "y"),
                e_labeled(1, "block"),
            ];
            if layer == TileLayer::Block {
                out.push(e_labeled(4, "team"));
                out.push(e_labeled(5, "rotation"));
            }
            out
        }
        // `SpawnUnitStatement.build` (`:1361-1385`).
        "spawn" => vec![
            e_field(5),
            e_text("-spawn"),
            e_labeled(0, "type"),
            e_labeled(1, "x"),
            e_labeled(2, "y"),
            e_labeled(4, "team"),
            e_labeled(3, "angle"),
            e_labeled(6, "effect"),
        ],
        // `SpawnBulletStatement.build` (`:1392-1426`).
        "bullet" => vec![
            e_field(0),
            e_text("-bullet"),
            e_labeled(1, "from"),
            e_labeled(2, "index"),
            e_labeled(3, "x"),
            e_labeled(4, "y"),
            e_labeled(5, "rotation"),
            e_labeled(6, "team"),
            e_labeled(7, "owner"),
            e_labeled(8, "damage"),
            e_labeled(9, "velocityScl"),
            e_labeled(10, "lifeScl"),
            e_labeled(11, "aimX"),
            e_labeled(12, "aimY"),
        ],
        // `ApplyStatusStatement.build` (`:1435-1491`); the effect picker is a
        // content button in Java (not ported) and the `isPermanent` check needs
        // status content (duration shown for every non-clear status).
        "status" => {
            let mut out = vec![e_select(0), e_field(1), e_labeled(2, "target")];
            if value(0) != "true" {
                out.push(e_labeled(3, "seconds"));
            }
            out
        }
        // `WeatherSenseStatement.build` (`:1501-1545`).
        "weathersense" => vec![e_field(0), e_text("="), e_text("weather"), e_field(1)],
        // `WeatherSetStatement.build` (`:1554-1595`).
        "weatherset" => vec![e_field(0), e_labeled(1, "state")],
        // `SpawnWaveStatement.build` (`:1602-1622`).
        "spawnwave" => vec![
            e_labeled(2, "natural"),
            e_labeled(0, "x"),
            e_labeled(1, "y"),
        ],
        // `SetRuleStatement.rebuild` (`:1634-1690`).
        "setrule" => {
            let rule = LogicRule::from_name(value(0)).unwrap_or(LogicRule::WaveSpacing);
            let mut out = vec![e_select(0)];
            if rule == LogicRule::MapArea {
                out.extend([
                    e_text("="),
                    e_labeled(2, "x"),
                    e_labeled(3, "y"),
                    e_labeled(4, "w"),
                    e_labeled(5, "h"),
                ]);
            } else if rule.uses_team() {
                out.extend([e_labeled(2, "of"), e_text("="), e_field(1)]);
            } else if matches!(rule, LogicRule::Ban | LogicRule::Unban) {
                out.extend([e_text("block-unit"), e_field(1)]);
            } else {
                out.extend([e_text("="), e_field(1)]);
            }
            out
        }
        // `FlushMessageStatement.build` (`:1698-1733`).
        "message" => {
            let type_ = MessageType::from_name(value(0)).unwrap_or(MessageType::Announce);
            let mut out = vec![e_select(0)];
            if matches!(type_, MessageType::Announce | MessageType::Toast) {
                out.push(e_labeled(1, "duration"));
            }
            out.push(e_labeled(2, "success"));
            out
        }
        // `CutsceneStatement.rebuild` (`:1745-1794`).
        "cutscene" => {
            let action = CutsceneAction::from_name(value(0)).unwrap_or(CutsceneAction::Pan);
            let mut out = vec![e_select(0)];
            match action {
                CutsceneAction::Active | CutsceneAction::GetHud => {
                    out.push(e_labeled(1, "result"));
                }
                CutsceneAction::Pan => {
                    out.extend([e_labeled(1, "x"), e_labeled(2, "y"), e_labeled(3, "speed")]);
                }
                CutsceneAction::Zoom => out.push(e_labeled(1, "level")),
                CutsceneAction::Shake => {
                    out.extend([e_labeled(1, "amount"), e_labeled(2, "duration")]);
                }
                CutsceneAction::SetHud => out.push(e_labeled(1, "shown")),
                CutsceneAction::Stop => {}
            }
            out
        }
        // `EffectStatement.build` (`:1801-1855`); the effect picker is the
        // effects dialog in Java (a plain select here).
        "effect" => {
            let entry = crate::logic::fx::get(value(0));
            let mut out = vec![e_select(0), e_labeled(1, "x"), e_labeled(2, "y")];
            if let Some(entry) = entry {
                if entry.color {
                    out.push(e_labeled(4, "color"));
                    out.push(e_color(4));
                }
                if entry.size || entry.rotate {
                    out.push(e_labeled(3, if entry.size { "size" } else { "rotation" }));
                }
                if entry.data.is_some() {
                    out.push(e_labeled(5, "data"));
                }
            }
            out
        }
        // `ExplosionStatement.build` (`:1862-1888`).
        "explosion" => vec![
            e_labeled(0, "team"),
            e_labeled(1, "x"),
            e_labeled(2, "y"),
            e_labeled(3, "radius"),
            e_labeled(4, "damage"),
            e_labeled(5, "air"),
            e_labeled(6, "ground"),
            e_labeled(7, "pierce"),
            e_labeled(8, "effect"),
        ],
        // `SetRateStatement.build` (`:1895-1898`).
        "setrate" => vec![e_labeled(0, "ipt = ")],
        // `FetchStatement.rebuild` (`:1920-1965`).
        "fetch" => {
            let type_ = FetchType::from_name(value(0)).unwrap_or(FetchType::Unit);
            let mut out = vec![e_field(1), e_text("="), e_select(0), e_labeled(2, "team")];
            if !matches!(
                type_,
                FetchType::CoreCount
                    | FetchType::PlayerCount
                    | FetchType::UnitCount
                    | FetchType::BuildCount
            ) {
                out.push(e_labeled(3, "#"));
            }
            if matches!(type_, FetchType::Build | FetchType::BuildCount) {
                out.push(e_labeled(4, "block"));
            }
            if matches!(type_, FetchType::Unit | FetchType::UnitCount) {
                out.push(e_labeled(4, "unit"));
            }
            out
        }
        // `SyncStatement.build` (`:1972-1974`).
        "sync" => vec![e_field(0)],
        // `ClientDataStatement.build` (`:1997-2005`).
        "clientdata" => vec![
            e_text("send "),
            e_field(1),
            e_text(" on "),
            e_field(0),
            e_text(", reliable "),
            e_field(2),
        ],
        // `GetFlagStatement.build` (`:2033-2058`).
        "getflag" => vec![e_field(0), e_text("="), e_text("flag"), e_field(1)],
        // `SetFlagStatement.build` (`:2065-2085`).
        "setflag" => vec![e_field(0), e_text("="), e_field(1)],
        // `SetPropStatement.build` (`:2093-2178`).
        "setprop" => vec![
            e_text("set"),
            e_field(0),
            e_text("of"),
            e_field(1),
            e_text("to"),
            e_field(2),
        ],
        // `PlaySoundStatement.build` (`:2181-2229`); the sound picker is the
        // sounds dialog in Java (not ported).
        "playsound" => {
            let positional = value(0) == "true";
            let mut out = vec![
                e_select(0),
                e_field(1),
                e_labeled(2, "volume"),
                e_labeled(3, "pitch"),
            ];
            if positional {
                out.extend([e_labeled(5, "x"), e_labeled(6, "y")]);
            } else {
                out.push(e_labeled(4, "pan"));
            }
            out.push(e_labeled(7, "limit"));
            out
        }
        // `PlayMusicStatement.build` (`:2352-2357`).
        "playmusic" => vec![e_labeled(0, "music"), e_labeled(1, "interrupt")],
        // `SetMarkerStatement.rebuild` (`:2364-2430`); the draw-layer list and
        // marker colours map through the marker params.
        "setmarker" => {
            let control = LMarkerControl::from_name(value(0)).unwrap_or(LMarkerControl::Pos);
            let mut out = vec![e_text("set"), e_select(0), e_labeled(1, "id")];
            for (i, &param) in control.params().iter().enumerate() {
                out.push(e_labeled(2 + i, param));
                if control == LMarkerControl::Color || (control == LMarkerControl::Colori && i == 1)
                {
                    out.push(e_color(2 + i));
                }
                if matches!(
                    control,
                    LMarkerControl::TextAlign | LMarkerControl::LineAlign
                ) {
                    out.push(e_align(2));
                }
            }
            out
        }
        // `MakeMarkerStatement.build` (`:2435-2471`).
        "makemarker" => vec![
            e_select(0),
            e_labeled(1, "id"),
            e_labeled(2, "x"),
            e_labeled(3, "y"),
            e_labeled(4, "replace"),
        ],
        // `LocalePrintStatement.build` (`:2477-2479`).
        "localeprint" => vec![e_wide(0)],
        _ => generic(),
    }
}

/// `showSelect` options for a select bound to field `index` of `name`.
pub fn editor_options(name: &str, index: usize) -> Vec<EditorOption> {
    match (name, index) {
        // `DrawStatement`'s type button.
        ("draw", 0) => enum_options("graphicstype", GraphicsType::ALL.iter().map(|v| v.name())),
        // `ControlStatement` (`LAccess.controls`, `:432-434`).
        ("control", 0) => enum_options(
            "laccess",
            LAccess::controls().into_iter().map(LAccess::name),
        ),
        // `RadarStatement` targets/sort (`:475-505`).
        ("radar" | "uradar", 0..=2) => {
            enum_options("radartarget", RadarTarget::ALL.iter().map(|v| v.name()))
        }
        ("radar" | "uradar", 3) => {
            enum_options("radarsort", RadarSort::ALL.iter().map(|v| v.name()))
        }
        // `OperationStatement`'s op button (`LogicOp.symbol`/`selectTranslate`).
        ("op", 0) => symbol_options(LogicOp::ALL.iter().map(|v| (v.name(), v.symbol()))),
        // `JumpStatement.addOp` / `SelectStatement` (`ConditionOp.symbol`).
        ("select" | "jump", 1) => {
            symbol_options(ConditionOp::ALL.iter().map(|v| (v.name(), v.symbol())))
        }
        // `LookupStatement` (`GlobalVars.lookableContent`).
        ("lookup", 0) => enum_options(
            "contenttype",
            [
                ContentType::Block,
                ContentType::Unit,
                ContentType::Item,
                ContentType::Liquid,
                ContentType::Team,
            ]
            .into_iter()
            .map(ContentType::name),
        ),
        // `UnitControlStatement` (`LUnitControl.all`; build/deconstruct are
        // hidden by the live rules probe in the dialog).
        ("ucontrol", 0) => enum_options("lunitcontrol", LUnitControl::ALL.iter().map(|v| v.name())),
        // `UnitLocateStatement` (`LLocate` / `BlockFlag.allLogic`).
        ("ulocate", 0) => enum_options("llocate", LLocate::ALL.iter().map(|v| v.name())),
        ("ulocate", 1) => enum_options("blockflag", BlockFlag::ALL_LOGIC.iter().map(|v| v.name())),
        // `QueryStatement` shape toggle + `QueryType.queryable`.
        ("query", 0) => token_options(["circle", "rect"].into_iter()),
        ("query", 1) => enum_options("querytype", QueryType::QUERYABLE.iter().map(|v| v.name())),
        // `GetBlockStatement`/`SetBlockStatement` layers.
        ("getblock", 0) => enum_options("tilelayer", TileLayer::ALL.iter().map(|v| v.name())),
        ("setblock", 0) => enum_options("tilelayer", TileLayer::SETTABLE.iter().map(|v| v.name())),
        // `ApplyStatusStatement`'s apply/clear toggle.
        ("status", 0) => toggle_options(&[("false", "apply"), ("true", "clear")]),
        // `FlushMessageStatement`/`CutsceneStatement`/`FetchStatement`.
        ("message", 0) => enum_options("messagetype", MessageType::ALL.iter().map(|v| v.name())),
        ("cutscene", 0) => enum_options(
            "cutsceneaction",
            CutsceneAction::ALL.iter().map(|v| v.name()),
        ),
        ("fetch", 0) => enum_options("fetchtype", FetchType::ALL.iter().map(|v| v.name())),
        // `SetRuleStatement`/`SetMarkerStatement`.
        ("setrule", 0) => enum_options("logicrule", LogicRule::ALL.iter().map(|v| v.name())),
        ("setmarker", 0) => enum_options(
            "lmarkercontrol",
            LMarkerControl::ALL.iter().map(|v| v.name()),
        ),
        // `EffectStatement`: the effects dialog is a select of `LogicFx` names.
        ("effect", 0) => token_options(crate::logic::fx::EFFECTS.iter().map(|e| e.name)),
        // `PlaySoundStatement`'s global/positional toggle.
        ("playsound", 0) => toggle_options(&[("false", "global"), ("true", "positional")]),
        // `MakeMarkerStatement` (`MapObjectives.allMarkerTypeNames`).
        ("makemarker", 0) => token_options(
            crate::game::map_markers::ALL_MARKER_TYPE_NAMES
                .iter()
                .copied(),
        ),
        _ => {
            let Some(statement) = default_statement(name) else {
                return Vec::new();
            };
            let Some(field) = statement.fields().get(index).cloned() else {
                return Vec::new();
            };
            if field.options.is_empty() {
                return Vec::new();
            }
            enum_options(
                &field.enum_class.to_lowercase(),
                field.options.iter().copied(),
            )
        }
    }
}

/// `<class>.label.<name>` options (`LStatement.bundle(Enum)` fallback = the name).
fn enum_options<'a>(class: &str, names: impl Iterator<Item = &'a str>) -> Vec<EditorOption> {
    names
        .map(|name| EditorOption {
            name: name.to_owned(),
            label: name.to_owned(),
            key: format!("{}.label.{}", class.to_lowercase(), name.to_lowercase()),
        })
        .collect()
}

/// `selectTranslate(symbol)` options (`LogicOp`/`ConditionOp` buttons): the
/// symbol is the label, and `not`/`and`/`or`/`b-and`/`xor`/`flip`/`always`
/// resolve through `name.token.<symbol>` (`LStatement.java:199-207`).
fn symbol_options<'a, I>(pairs: I) -> Vec<EditorOption>
where
    I: Iterator<Item = (&'a str, &'a str)>,
{
    pairs
        .map(|(name, symbol)| EditorOption {
            name: name.to_owned(),
            label: symbol.to_owned(),
            key: match symbol {
                "not" | "and" | "or" | "b-and" | "xor" | "flip" | "always" => {
                    format!("name.token.{symbol}")
                }
                _ => String::new(),
            },
        })
        .collect()
}

/// `name.token.<token>` options (Java `showSelect` over plain strings).
fn token_options<'a>(names: impl Iterator<Item = &'a str>) -> Vec<EditorOption> {
    names
        .map(|name| EditorOption {
            name: name.to_owned(),
            label: name.to_owned(),
            key: format!("name.token.{name}"),
        })
        .collect()
}

/// Toggle-button options (Java builds these with `bundle(clear ? "clear" : "apply")`).
fn toggle_options(pairs: &[(&'static str, &'static str)]) -> Vec<EditorOption> {
    pairs
        .iter()
        .map(|(name, token)| EditorOption {
            name: (*name).to_owned(),
            label: (*token).to_owned(),
            key: format!("name.token.{token}"),
        })
        .collect()
}

/// Build-time field fixups: `SetRuleStatement.rebuild` defaults `p1` to
/// `@sharded` for the team rules (`LStatements.java:1653-1660`).
pub fn editor_normalize(name: &str, values: &mut [String]) -> bool {
    if name != "setrule" {
        return false;
    }
    let Some(rule) = LogicRule::from_name(values.first().map(String::as_str).unwrap_or("")) else {
        return false;
    };
    if rule.uses_team()
        && values.get(2).map(String::as_str) == Some("0")
        && let Some(slot) = values.get_mut(2)
    {
        *slot = "@sharded".to_owned();
        return true;
    }
    false
}

/// The value fixups `DrawStatement` applies when the type button changes
/// (`LStatements.java` `showSelect` callback).
pub fn editor_type_change(name: &str, values: &mut [String]) {
    if name != "draw" {
        return;
    }
    let type_ = values.first().map(String::as_str).unwrap_or("").to_owned();
    let mut set = |index: usize, value: &str| {
        if let Some(slot) = values.get_mut(index) {
            *slot = value.to_owned();
        }
    };
    match type_.as_str() {
        "color" => set(4, "255"),
        "image" => {
            set(3, "@copper");
            set(4, "32");
            set(5, "0");
        }
        "print" => set(3, "@bottomLeft"),
        _ => {}
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

    fn layout(name: &str, values: &[&str]) -> Vec<EditorItem> {
        let values: Vec<String> = values.iter().map(|v| (*v).to_owned()).collect();
        editor_layout(name, &values)
    }

    #[test]
    fn io_block_layouts_match_java() {
        // `read`: bundle("read") value = target at address (`LStatements.java:67-89`).
        let read = layout("read", &["out", "cell", "0"]);
        assert_eq!(
            read.iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Text, 0, "read"),
                (EditorKind::Field, 0, ""),
                (EditorKind::Text, 0, "="),
                (EditorKind::Field, 1, ""),
                (EditorKind::Text, 0, "at"),
                (EditorKind::Field, 2, ""),
            ]
        );
        // `write`: bundle("write") input to target at address.
        let write = layout("write", &["in", "cell", "0"]);
        assert_eq!(
            write.iter().map(|i| i.label).collect::<Vec<_>>(),
            vec!["write", "", "to", "", "at", ""]
        );
        // `getlink`: output = linknum address.
        let getlink = layout("getlink", &["result", "0"]);
        assert_eq!(
            getlink
                .iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Field, 0, ""),
                (EditorKind::Text, 0, "="),
                (EditorKind::Text, 0, "linknum"),
                (EditorKind::Field, 1, ""),
            ]
        );
        // `sensor`: to = type in from (field order to, from, type).
        let sensor = layout("sensor", &["result", "block1", "@copper"]);
        assert_eq!(
            sensor.iter().map(|i| (i.kind, i.index)).collect::<Vec<_>>(),
            vec![
                (EditorKind::Field, 0),
                (EditorKind::Text, 0),
                (EditorKind::Field, 2),
                (EditorKind::Text, 0),
                (EditorKind::Field, 1),
            ]
        );
    }

    #[test]
    fn control_layout_uses_access_params() {
        // `control shoot`: select + of + target + x/y/shoot (`:422-455`).
        let control = layout("control", &["shoot", "turret1", "0", "0", "0", "0"]);
        assert_eq!((control[1].kind, control[1].index), (EditorKind::Select, 0));
        assert_eq!((control[3].kind, control[3].index), (EditorKind::Field, 1));
        assert_eq!(
            control[4..]
                .iter()
                .map(|i| (i.index, i.label))
                .collect::<Vec<_>>(),
            vec![(2, "x"), (3, "y"), (4, "shoot")]
        );
        // `control enabled`: one param (`LAccess.params`).
        let enabled = layout("control", &["enabled", "switch1", "0", "0", "0", "0"]);
        assert_eq!(
            enabled[4..]
                .iter()
                .map(|i| (i.index, i.label))
                .collect::<Vec<_>>(),
            vec![(2, "to")]
        );
    }

    #[test]
    fn radar_layout_matches_java() {
        // `RadarStatement.build`: output = from + 3 targets + order + sort (`:466-521`).
        let radar = layout(
            "radar",
            &["enemy", "any", "any", "distance", "turret1", "1", "out"],
        );
        assert_eq!((radar[0].kind, radar[0].index), (EditorKind::Field, 6));
        assert_eq!(radar[1].label, "=");
        assert_eq!((radar[2].index, radar[2].label), (4, "from"));
        assert_eq!(
            radar[3..6].iter().map(|i| i.kind).collect::<Vec<_>>(),
            vec![EditorKind::Select; 3]
        );
        assert_eq!((radar[6].index, radar[6].label), (5, "order"));
        assert_eq!(radar[7].label, "sort");
        assert_eq!((radar[8].kind, radar[8].index), (EditorKind::Select, 3));
    }

    #[test]
    fn draw_layout_still_matches() {
        let clear = layout("draw", &["clear", "0", "0", "0", "0", "0", "0"]);
        assert_eq!(
            clear
                .iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Select, 0, ""),
                (EditorKind::Field, 1, "r"),
                (EditorKind::Field, 2, "g"),
                (EditorKind::Field, 3, "b"),
            ]
        );
        let poly = layout("draw", &["poly", "0", "0", "0", "0", "0", "0"]);
        assert_eq!(
            poly[1..]
                .iter()
                .map(|i| (i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (1, "x"),
                (2, "y"),
                (3, "sides"),
                (4, "radius"),
                (5, "rotation")
            ]
        );
        let col = layout("draw", &["col", "%ff0000", "0", "0", "0", "0", "0"]);
        assert_eq!((col[1].index, col[1].label), (1, "color"));
        assert_eq!((col[2].kind, col[2].index), (EditorKind::Color, 1));
        let print = layout("draw", &["print", "0", "0", "@center", "0", "0", "0"]);
        assert_eq!((print[3].index, print[3].label), (3, "align"));
        assert_eq!((print[4].kind, print[4].index), (EditorKind::Align, 3));
    }

    #[test]
    fn editor_select_options_match_java() {
        // `LAccess.controls` only (params > 0).
        let control = editor_options("control", 0);
        assert!(control.iter().any(|o| o.name == "shoot"));
        assert!(control.iter().any(|o| o.name == "shootp"));
        assert!(control.iter().any(|o| o.name == "enabled"));
        assert!(!control.iter().any(|o| o.name == "health"));
        assert!(control.iter().all(|o| o.key.starts_with("laccess.label.")));
        // `GraphicsType` labels come from the class bundle key.
        let draw = editor_options("draw", 0);
        assert_eq!(draw.len(), 16);
        assert!(
            draw.iter()
                .any(|o| o.name == "lineRect" && o.key == "graphicstype.label.linerect")
        );
    }

    #[test]
    fn operation_flow_layouts_match_java() {
        // `op` binary: dest = a op b (`LStatements.java:685-737`).
        let add = layout("op", &["add", "r", "a", "b"]);
        assert_eq!(
            add.iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Field, 1, ""),
                (EditorKind::Text, 0, "="),
                (EditorKind::Field, 2, ""),
                (EditorKind::Select, 0, ""),
                (EditorKind::Field, 3, ""),
            ]
        );
        // unary: op first, one operand.
        let not = layout("op", &["not", "r", "a", "b"]);
        assert_eq!((not[2].kind, not[2].index), (EditorKind::Select, 0));
        assert_eq!((not[3].kind, not[3].index), (EditorKind::Field, 2));
        assert_eq!(not.len(), 4);
        // func: op first, two operands (`max`, and `logn` from `LogicOp.func`).
        let max = layout("op", &["max", "r", "a", "b"]);
        assert_eq!((max[2].kind, max[2].index), (EditorKind::Select, 0));
        assert_eq!(max.len(), 5);
        let logn = layout("op", &["logn", "r", "a", "b"]);
        assert_eq!((logn[2].kind, logn[2].index), (EditorKind::Select, 0));
        // `select`: result = if comp0 op comp1 then a else b.
        let select = layout("select", &["r", "notEqual", "x", "false", "a", "b"]);
        assert_eq!(
            select.iter().map(|i| (i.kind, i.index)).collect::<Vec<_>>(),
            vec![
                (EditorKind::Field, 0),
                (EditorKind::Text, 0),
                (EditorKind::Text, 0),
                (EditorKind::Field, 2),
                (EditorKind::Select, 1),
                (EditorKind::Field, 3),
                (EditorKind::Text, 0),
                (EditorKind::Field, 4),
                (EditorKind::Text, 0),
                (EditorKind::Field, 5),
            ]
        );
        // `always` hides both operand fields.
        let always = layout("select", &["r", "always", "x", "false", "a", "b"]);
        assert_eq!(
            always.iter().map(|i| (i.kind, i.index)).collect::<Vec<_>>(),
            vec![
                (EditorKind::Field, 0),
                (EditorKind::Text, 0),
                (EditorKind::Text, 0),
                (EditorKind::Select, 1),
                (EditorKind::Text, 0),
                (EditorKind::Field, 4),
                (EditorKind::Text, 0),
                (EditorKind::Field, 5),
            ]
        );
        // `wait`/`stop`/`end`.
        assert_eq!(layout("wait", &["0.5"]).len(), 2);
        assert!(layout("stop", &[]).is_empty());
        assert!(layout("end", &[]).is_empty());
        // `lookup`: result = lookup type # id (field order type, result, id).
        let lookup = layout("lookup", &["item", "r", "0"]);
        assert_eq!(
            lookup.iter().map(|i| (i.kind, i.index)).collect::<Vec<_>>(),
            vec![
                (EditorKind::Field, 1),
                (EditorKind::Text, 0),
                (EditorKind::Select, 0),
                (EditorKind::Text, 0),
                (EditorKind::Field, 2),
            ]
        );
        // `packcolor`/`unpackcolor` operand order.
        let pack = layout("packcolor", &["r", "1", "0", "0", "1"]);
        assert_eq!((pack[3].kind, pack[3].index), (EditorKind::Field, 1));
        assert_eq!((pack[6].kind, pack[6].index), (EditorKind::Field, 4));
        let unpack = layout("unpackcolor", &["r", "g", "b", "a", "c"]);
        assert_eq!((unpack[6].kind, unpack[6].index), (EditorKind::Field, 4));
        // `jump`: if value op compare + the target button (dest_index).
        let jump = layout("jump", &["3", "notEqual", "x", "false"]);
        assert_eq!((jump[0].kind, jump[0].label), (EditorKind::Text, "if"));
        assert_eq!((jump[1].kind, jump[1].index), (EditorKind::Field, 2));
        assert_eq!((jump[2].kind, jump[2].index), (EditorKind::Select, 1));
        assert_eq!((jump[3].kind, jump[3].index), (EditorKind::Field, 3));
        assert_eq!((jump[4].kind, jump[4].index), (EditorKind::Jump, 0));
        // `LogicOp`/`ConditionOp` selects show symbols, not names.
        let ops = editor_options("op", 0);
        assert_eq!((ops[0].name.as_str(), ops[0].label.as_str()), ("add", "+"));
        let conds = editor_options("select", 1);
        assert!(
            conds
                .iter()
                .any(|o| o.name == "notEqual" && o.label == "not" && o.key == "name.token.not")
        );
        assert!(
            conds
                .iter()
                .any(|o| o.name == "always" && o.key == "name.token.always")
        );
        // `lookup` options are the lookable content types.
        let lookup = editor_options("lookup", 0);
        assert_eq!(lookup.len(), 5);
        assert!(lookup.iter().any(|o| o.name == "team"));
    }

    #[test]
    fn unit_layouts_match_java() {
        // `ubind`: type <field> (`LStatements.java:1024-1060`).
        let ubind = layout("ubind", &["@poly"]);
        assert_eq!((ubind[0].kind, ubind[0].label), (EditorKind::Text, "type"));
        assert_eq!((ubind[1].kind, ubind[1].index), (EditorKind::Field, 0));
        // `ucontrol move`: select + x/y (`LUnitControl.params`).
        let move_ = layout("ucontrol", &["move", "0", "0", "0", "0", "0"]);
        assert_eq!((move_[0].kind, move_[0].index), (EditorKind::Select, 0));
        assert_eq!(
            move_[1..]
                .iter()
                .map(|i| (i.index, i.label))
                .collect::<Vec<_>>(),
            vec![(1, "x"), (2, "y")]
        );
        // `ucontrol target`: x/y/shoot.
        let target = layout("ucontrol", &["target", "0", "0", "1", "0", "0"]);
        assert_eq!(
            target[1..]
                .iter()
                .map(|i| (i.index, i.label))
                .collect::<Vec<_>>(),
            vec![(1, "x"), (2, "y"), (3, "shoot")]
        );
        // `ucontrol build`: five params.
        assert_eq!(
            layout("ucontrol", &["build", "0", "0", "b", "0", "0"]).len(),
            6
        );
        // `uradar` drops the `from` field of `RadarStatement` (`:1103-1125`).
        let uradar = layout(
            "uradar",
            &["enemy", "any", "any", "distance", "0", "1", "out"],
        );
        assert_eq!((uradar[2].kind, uradar[2].index), (EditorKind::Select, 0));
        assert_eq!(uradar.len(), 8);
        // `ulocate building`: find/group rows + all outputs (`:1135-1215`).
        let building = layout(
            "ulocate",
            &["building", "core", "true", "@copper", "ox", "oy", "f", "b"],
        );
        assert_eq!(
            building
                .iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Text, 0, "find"),
                (EditorKind::Select, 0, ""),
                (EditorKind::Text, 0, "group"),
                (EditorKind::Select, 1, ""),
                (EditorKind::Field, 2, "enemy"),
                (EditorKind::Field, 4, "outX"),
                (EditorKind::Field, 5, "outY"),
                (EditorKind::Field, 6, "found"),
                (EditorKind::Field, 7, "building"),
            ]
        );
        // `ulocate ore`: the ore field replaces the group row, no building out.
        let ore = layout(
            "ulocate",
            &["ore", "core", "true", "@copper", "ox", "oy", "f", "b"],
        );
        assert_eq!(
            ore.iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Text, 0, "find"),
                (EditorKind::Select, 0, ""),
                (EditorKind::Field, 3, "ore"),
                (EditorKind::Field, 4, "outX"),
                (EditorKind::Field, 5, "outY"),
                (EditorKind::Field, 6, "found"),
            ]
        );
        // `ulocate spawn`: neither group nor ore rows.
        let spawn = layout(
            "ulocate",
            &["spawn", "core", "true", "@copper", "ox", "oy", "f", "b"],
        );
        assert_eq!(spawn.len(), 6);
        // Options: `LUnitControl`/`BlockFlag.allLogic` labels.
        let controls = editor_options("ucontrol", 0);
        assert_eq!(controls.len(), 21);
        assert!(
            controls
                .iter()
                .any(|o| o.name == "itemDrop" && o.key == "lunitcontrol.label.itemdrop")
        );
        let flags = editor_options("ulocate", 1);
        assert_eq!(flags.len(), 10);
        assert!(
            flags
                .iter()
                .any(|o| o.name == "core" && o.key == "blockflag.label.core")
        );
    }

    #[test]
    fn world_layouts_match_java() {
        // `query` circle: shape + type + team/x/y + radius (`:1224-1267`).
        let circle = layout("query", &["circle", "unit", "null", "0", "0", "10", "10"]);
        assert_eq!(
            circle
                .iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Select, 0, ""),
                (EditorKind::Select, 1, ""),
                (EditorKind::Field, 2, "team"),
                (EditorKind::Field, 3, "x"),
                (EditorKind::Field, 4, "y"),
                (EditorKind::Field, 5, "radius"),
            ]
        );
        let rect = layout("query", &["rect", "unit", "null", "0", "0", "10", "10"]);
        assert_eq!((rect[5].index, rect[5].label), (5, "width"));
        assert_eq!((rect[6].index, rect[6].label), (6, "height"));
        // `getblock`: result, " =", layer, x, y (`:1275-1303`).
        let getblock = layout("getblock", &["block", "r", "0", "0"]);
        assert_eq!(
            getblock
                .iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Field, 1, ""),
                (EditorKind::Text, 0, " ="),
                (EditorKind::Select, 0, ""),
                (EditorKind::Field, 2, "x"),
                (EditorKind::Field, 3, "y"),
            ]
        );
        // `setblock` block layer shows team/rotation; floor/ore hide them.
        let block = layout(
            "setblock",
            &["block", "@copper-wall", "1", "2", "@sharded", "0"],
        );
        assert_eq!(
            block
                .iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Text, 0, "set"),
                (EditorKind::Select, 0, ""),
                (EditorKind::Field, 2, "x"),
                (EditorKind::Field, 3, "y"),
                (EditorKind::Field, 1, "block"),
                (EditorKind::Field, 4, "team"),
                (EditorKind::Field, 5, "rotation"),
            ]
        );
        assert_eq!(
            layout("setblock", &["floor", "@sand", "1", "2", "@sharded", "0"]).len(),
            5
        );
        // `spawn`: result then "= spawn", then type/x/y/team/angle/effect.
        let spawn = layout(
            "spawn",
            &["@dagger", "10", "10", "90", "@sharded", "r", "true"],
        );
        assert_eq!((spawn[0].kind, spawn[0].index), (EditorKind::Field, 5));
        assert_eq!(
            (spawn[1].kind, spawn[1].label),
            (EditorKind::Text, "-spawn")
        );
        assert_eq!((spawn[2].index, spawn[2].label), (0, "type"));
        assert_eq!((spawn[5].index, spawn[5].label), (4, "team"));
        assert_eq!((spawn[6].index, spawn[6].label), (3, "angle"));
        // `status`: seconds hidden when clear (`:1435-1491`).
        let wet = layout("status", &["false", "@status-wet", "unit", "10"]);
        assert_eq!(wet.len(), 4);
        let clear = layout("status", &["true", "@status-wet", "unit", "10"]);
        assert_eq!(clear.len(), 3);
        // `setrule` rows per rule (`:1634-1690`).
        let area = layout("setrule", &["mapArea", "10", "0", "0", "100", "100"]);
        assert_eq!(
            area[1..]
                .iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Text, 0, "="),
                (EditorKind::Field, 2, "x"),
                (EditorKind::Field, 3, "y"),
                (EditorKind::Field, 4, "w"),
                (EditorKind::Field, 5, "h"),
            ]
        );
        let team = layout("setrule", &["unitHealth", "10", "0", "0", "100", "100"]);
        assert_eq!(
            team[1..]
                .iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Field, 2, "of"),
                (EditorKind::Text, 0, "="),
                (EditorKind::Field, 1, ""),
            ]
        );
        let ban = layout("setrule", &["ban", "10", "0", "0", "100", "100"]);
        assert_eq!((ban[1].label, ban[2].index), ("block-unit", 1));
        // `message` announce shows duration; mission does not.
        assert_eq!(layout("message", &["announce", "3", "@wait"]).len(), 3);
        assert_eq!(layout("message", &["mission", "3", "@wait"]).len(), 2);
        // `cutscene` pan/x/y/speed vs zoom/level.
        let pan = layout("cutscene", &["pan", "100", "100", "0.06", "0"]);
        assert_eq!(
            pan[1..]
                .iter()
                .map(|i| (i.index, i.label))
                .collect::<Vec<_>>(),
            vec![(1, "x"), (2, "y"), (3, "speed")]
        );
        let zoom = layout("cutscene", &["zoom", "4", "100", "0.06", "0"]);
        assert_eq!((zoom[1].index, zoom[1].label), (1, "level"));
        // `effect` follows `LogicFx` flags: warn (none), trail (size+color),
        // blockFall (data).
        let warn = layout("effect", &["warn", "0", "0", "2", "%ffaaff", ""]);
        assert_eq!(warn.len(), 3);
        let trail = layout("effect", &["trail", "0", "0", "2", "%ffaaff", ""]);
        assert_eq!((trail[3].index, trail[3].label), (4, "color"));
        assert_eq!(trail[4].kind, EditorKind::Color);
        assert_eq!((trail[5].index, trail[5].label), (3, "size"));
        let fall = layout("effect", &["blockFall", "0", "0", "2", "%ffaaff", "block"]);
        assert_eq!((fall[3].index, fall[3].label), (5, "data"));
        // `fetch` unit hides nothing, coreCount hides `#`.
        let unit = layout("fetch", &["unit", "r", "@sharded", "0", "@conveyor"]);
        assert_eq!(
            unit[4..]
                .iter()
                .map(|i| (i.index, i.label))
                .collect::<Vec<_>>(),
            vec![(3, "#"), (4, "unit")]
        );
        let count = layout("fetch", &["coreCount", "r", "@sharded", "0", "@conveyor"]);
        assert_eq!(count.len(), 4);
        // `playsound` global shows pan, positional shows x/y.
        let global = layout(
            "playsound",
            &[
                "false",
                "@sfx-shoot",
                "1",
                "1",
                "0",
                "@thisx",
                "@thisy",
                "true",
            ],
        );
        assert_eq!((global[4].index, global[4].label), (4, "pan"));
        let pos = layout(
            "playsound",
            &[
                "true",
                "@sfx-shoot",
                "1",
                "1",
                "0",
                "@thisx",
                "@thisy",
                "true",
            ],
        );
        assert_eq!(
            pos[4..6]
                .iter()
                .map(|i| (i.index, i.label))
                .collect::<Vec<_>>(),
            vec![(5, "x"), (6, "y")]
        );
        // `setmarker` color/colori rows carry the colour pencil.
        let color = layout("setmarker", &["color", "0", "%ff0000", "0", "0"]);
        assert_eq!(
            color[3..]
                .iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![(EditorKind::Field, 2, "color"), (EditorKind::Color, 2, "")]
        );
        let colori = layout("setmarker", &["colori", "0", "0", "%ff0000", "0"]);
        assert_eq!(
            colori[3..]
                .iter()
                .map(|i| (i.kind, i.index, i.label))
                .collect::<Vec<_>>(),
            vec![
                (EditorKind::Field, 2, "index"),
                (EditorKind::Field, 3, "color"),
                (EditorKind::Color, 3, ""),
            ]
        );
        // `makemarker`/`playsound`/`status` select options.
        let marker = editor_options("makemarker", 0);
        assert_eq!(marker.len(), 8);
        assert!(
            marker
                .iter()
                .any(|o| o.name == "shapeText" && o.key == "name.token.shapeText")
        );
        let sound = editor_options("playsound", 0);
        assert_eq!(
            (sound[0].label.as_str(), sound[1].label.as_str()),
            ("global", "positional")
        );
        let status = editor_options("status", 0);
        assert_eq!(
            (status[0].name.as_str(), status[0].label.as_str()),
            ("false", "apply")
        );
        // `editor_normalize`: team rules default `p1` to `@sharded`.
        let mut values: Vec<String> = ["unitHealth", "10", "0", "0", "100", "100"]
            .iter()
            .map(|v| (*v).to_owned())
            .collect();
        assert!(editor_normalize("setrule", &mut values));
        assert_eq!(values[2], "@sharded");
        let mut values: Vec<String> = ["unitHealth", "10", "@crux", "0", "100", "100"]
            .iter()
            .map(|v| (*v).to_owned())
            .collect();
        assert!(!editor_normalize("setrule", &mut values));
    }
}
