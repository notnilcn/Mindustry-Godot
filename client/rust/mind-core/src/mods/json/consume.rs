// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Sounds, consumes and stacks.
//!
//! Upstream: core/src/mindustry/mod/ContentParser.java

use super::*;

impl<'a> ContentJsonParser<'a> {
    /// Parses a sound name (`SoundId::by_name`; `Sounds.<field>` upstream).
    pub(super) fn parse_sound(&mut self, file: &str, value: Option<&Value>) -> Option<SoundId> {
        let raw = value.and_then(Value::as_str)?;
        match SoundId::by_name(raw) {
            Some(sound) => Some(sound),
            None => {
                self.warn(file, format!("unknown sound `{raw}`"));
                None
            }
        }
    }

    /// `ContentParser` `consumes` array (`mindustry/world/consumers/*`): each
    /// entry's `type` resolves through the `ClassMap` against the `Consume` base
    /// class and its fields are read with Java's defaults (`Consume.java:16`
    /// `update = true`; `ConsumeCoolant.java:6` `allowLiquid = true`,
    /// `allowGas = false`). Classes whose runtime half the port does not have
    /// are ignored with an explicit warning — never a silent drop.
    pub(super) fn parse_consumes(
        &mut self,
        registry: &ContentRegistry,
        file: &str,
        value: &Value,
    ) -> Result<Vec<ConsumeDef>, ContentParseError> {
        let Value::Array(entries) = value else {
            return Err(ContentParseError::new(format!(
                "{file}: `consumes` must be an array"
            )));
        };
        let mut out = Vec::new();
        for entry in entries {
            let Value::Object(map) = entry else {
                return Err(ContentParseError::new(format!(
                    "{file}: `consumes` entries must be objects"
                )));
            };
            let Some(class) = map.get("type").and_then(Value::as_str) else {
                return Err(ContentParseError::new(format!(
                    "{file}: `consumes` entry missing `type`"
                )));
            };
            let tag = self
                .classmap
                .resolve(class, ClassScope::Consume)
                .unwrap_or(class);
            let update = field_bool(map, "update").unwrap_or(true);
            let optional = field_bool(map, "optional").unwrap_or(false);
            let ignore = field_bool(map, "ignore").unwrap_or(false);
            match tag {
                // `ConsumeItems` (`ConsumeItems.java:11`, `ItemStack[] items`).
                "ConsumeItems" => {
                    let stacks = match map.get("items") {
                        Some(items) => self.parse_stacks(registry, file, items)?,
                        None => Vec::new(),
                    };
                    out.push(ConsumeDef::Items {
                        stacks,
                        optional,
                        ignore,
                        update,
                    });
                }
                // `ConsumeLiquid` (`ConsumeLiquid.java:14` `final Liquid liquid`,
                // `ConsumeLiquidBase.java:8` `float amount`).
                "ConsumeLiquid" => {
                    let Some(raw) = map.get("liquid").and_then(Value::as_str) else {
                        return Err(ContentParseError::new(format!(
                            "{file}: ConsumeLiquid requires `liquid`"
                        )));
                    };
                    let liquid = resolve_name(registry, raw).ok_or_else(|| {
                        ContentParseError::new(format!("{file}: unknown liquid `{raw}`"))
                    })?;
                    out.push(ConsumeDef::Liquid {
                        liquid: Box::leak(liquid.into_boxed_str()),
                        amount: field_f32(map, "amount").unwrap_or(0.0),
                        optional,
                        update,
                    });
                }
                // `ConsumeLiquids` (`ConsumeLiquids.java:12` `LiquidStack[]`).
                "ConsumeLiquids" => {
                    let stacks = match map.get("liquids") {
                        Some(liquids) => self.parse_liquid_stacks(registry, file, liquids)?,
                        None => Vec::new(),
                    };
                    out.push(ConsumeDef::Liquids {
                        stacks,
                        optional,
                        update,
                    });
                }
                // `ConsumePower` (`ConsumePower.java:10-14`: `usage`, `capacity`,
                // `buffered`); `consumePowerBuffered` sets `buffered`.
                "ConsumePower" => {
                    let buffered = field_bool(map, "buffered").unwrap_or(false);
                    out.push(ConsumeDef::Power {
                        usage: field_f32(map, "usage").unwrap_or(0.0),
                        buffered: if buffered {
                            field_f32(map, "capacity").unwrap_or(0.0)
                        } else {
                            0.0
                        },
                        update,
                    });
                }
                // `ConsumeCoolant` (`ConsumeCoolant.java:4-6`).
                "ConsumeCoolant" => {
                    out.push(ConsumeDef::Coolant {
                        amount: field_f32(map, "amount").unwrap_or(1.0),
                        allow_liquid: field_bool(map, "allowLiquid").unwrap_or(true),
                        allow_gas: field_bool(map, "allowGas").unwrap_or(false),
                        optional,
                        update,
                    });
                }
                // `ConsumeItemExplode` (`ConsumeItemExplode.java:17-19`: damage,
                // threshold, baseChance) takes an explosive-item *filter*; the
                // explode runtime lands with J04 T2, so the entry is recognized
                // and warned about instead of being dropped silently.
                "ConsumeItemExplode" => {
                    self.warn(
                        file,
                        "consumer `ConsumeItemExplode` recognized; its runtime lands with J04 T2",
                    );
                }
                // Filter/dynamic/payload/power-condition consumers: recognized
                // aliases without a port runtime half yet.
                other => {
                    self.warn(
                        file,
                        format!("consumer `{other}` is unsupported in the port; ignored"),
                    );
                }
            }
        }
        Ok(out)
    }

    /// `ConsumeLiquids.liquids` (`LiquidStack[]`): `{liquid, amount}` entries.
    pub(super) fn parse_liquid_stacks(
        &mut self,
        registry: &ContentRegistry,
        file: &str,
        value: &Value,
    ) -> Result<Vec<LiquidStackSpec>, ContentParseError> {
        let entries: Vec<&Value> = match value {
            Value::Array(items) => items.iter().collect(),
            Value::Object(map) => map.values().collect(),
            _ => {
                return Err(ContentParseError::new(format!(
                    "{file}: `liquids` must be an array or object"
                )));
            }
        };
        let mut out = Vec::new();
        for entry in entries {
            let Value::Object(map) = entry else {
                return Err(ContentParseError::new(format!(
                    "{file}: invalid liquid entry"
                )));
            };
            let raw = map.get("liquid").and_then(Value::as_str).ok_or_else(|| {
                ContentParseError::new(format!("{file}: liquid stack missing `liquid`"))
            })?;
            let resolved = resolve_name(registry, raw)
                .ok_or_else(|| ContentParseError::new(format!("{file}: unknown liquid `{raw}`")))?;
            out.push(liquid_stack(
                Box::leak(resolved.into_boxed_str()),
                field_f32(map, "amount").unwrap_or(0.0),
            ));
        }
        Ok(out)
    }

    pub(super) fn parse_stacks(
        &mut self,
        registry: &ContentRegistry,
        file: &str,
        value: &Value,
    ) -> Result<Vec<StackSpec>, ContentParseError> {
        let mut out = Vec::new();
        let entries: Vec<&Value> = match value {
            Value::Array(items) => items.iter().collect(),
            Value::Object(map) => map.values().collect(),
            _ => {
                return Err(ContentParseError::new(format!(
                    "{file}: `requirements` must be an array or object"
                )));
            }
        };
        for entry in entries {
            // `ItemStack`/`LiquidStack`/… aliases are recognized on stack
            // entries (`data` scope); an unknown one is warned about.
            if let Value::Object(map) = entry
                && let Some(class) = map.get("type").and_then(Value::as_str)
                && self.classmap.resolve(class, ClassScope::Data).is_none()
            {
                self.warn(
                    file,
                    format!("stack `type` `{class}` is unknown; entry parsed by fields"),
                );
            }
            let (raw_name, amount) = match entry {
                Value::String(text) => parse_stack(text).ok_or_else(|| {
                    ContentParseError::new(format!("{file}: invalid stack `{text}`"))
                })?,
                Value::Object(map) => {
                    let item = map.get("item").and_then(Value::as_str).ok_or_else(|| {
                        ContentParseError::new(format!("{file}: stack missing item"))
                    })?;
                    let amount = map.get("amount").and_then(Value::as_i64).unwrap_or(1);
                    (item.to_owned(), amount as i32)
                }
                _ => {
                    return Err(ContentParseError::new(format!(
                        "{file}: invalid requirement entry"
                    )));
                }
            };
            let resolved = resolve_name(registry, &raw_name).ok_or_else(|| {
                ContentParseError::new(format!("{file}: unknown item `{raw_name}`"))
            })?;
            out.push(stack(Box::leak(resolved.into_boxed_str()), amount));
        }
        Ok(out)
    }
}
