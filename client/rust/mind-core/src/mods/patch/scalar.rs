// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Typed scalar setters and the shared parsing helpers.
//!
//! Upstream: core/src/mindustry/mod/DataPatcher.java

use super::*;

impl DataPatcher {
    /// Records a reverse action that [`DataPatcher::unapply`] runs in reverse order.
    pub(super) fn push_reset(&mut self, reset: impl FnOnce(&mut ContentRegistry) + 'static) {
        self.resetters.push(Box::new(reset));
    }

    // ---- generic scalar helpers ----

    /// Shared body of the typed setters: mode check, parse, record the original,
    /// push a resetter, then write the new value.
    #[allow(clippy::too_many_arguments)]
    fn set_scalar<T: Copy + 'static>(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        field: &str,
        value: &Value,
        mode: FieldMode,
        parse: fn(&Value) -> Option<T>,
        get: impl FnOnce(&ContentRegistry) -> Option<T>,
        set: impl Fn(&mut ContentRegistry, T) + Clone + 'static,
    ) {
        if mode != FieldMode::Set {
            self.warn(format!("array suffix is invalid on `{field}`"));
            return;
        }
        let Some(new) = parse(value) else {
            return;
        };
        if self.mark_used(reference, field) {
            let Some(original) = get(registry) else {
                return;
            };
            let reset = set.clone();
            self.push_reset(move |r: &mut ContentRegistry| reset(r, original));
        }
        set(registry, new);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn set_i32(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        field: &str,
        value: &Value,
        mode: FieldMode,
        get: impl FnOnce(&ContentRegistry) -> Option<i32>,
        set: impl Fn(&mut ContentRegistry, i32) + Clone + 'static,
    ) {
        self.set_scalar(
            registry,
            reference,
            field,
            value,
            mode,
            |v| v.as_i64().map(|v| v as i32),
            get,
            set,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn set_f32(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        field: &str,
        value: &Value,
        mode: FieldMode,
        get: impl FnOnce(&ContentRegistry) -> Option<f32>,
        set: impl Fn(&mut ContentRegistry, f32) + Clone + 'static,
    ) {
        self.set_scalar(
            registry,
            reference,
            field,
            value,
            mode,
            |v| v.as_f64().map(|v| v as f32),
            get,
            set,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn set_bool(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        field: &str,
        value: &Value,
        mode: FieldMode,
        get: impl FnOnce(&ContentRegistry) -> Option<bool>,
        set: impl Fn(&mut ContentRegistry, bool) + Clone + 'static,
    ) {
        self.set_scalar(
            registry,
            reference,
            field,
            value,
            mode,
            Value::as_bool,
            get,
            set,
        );
    }

    // ---- shared parsing ----

    pub(super) fn parse_item_stacks(
        &mut self,
        registry: &ContentRegistry,
        value: &Value,
    ) -> Result<Vec<ItemStack>, String> {
        let entries: Vec<&Value> = match value {
            Value::Array(items) => items.iter().collect(),
            Value::String(_) => vec![value],
            Value::Object(map) => {
                // `{ "copper": 10 }` form.
                let mut out = Vec::new();
                for (name, amount) in map {
                    let item = registry
                        .item_id(name)
                        .or_else(|| registry.item_id(&registry.transform_name(name)))
                        .ok_or_else(|| format!("unknown item `{name}`"))?;
                    out.push(ItemStack::new(item, amount.as_i64().unwrap_or(1) as i32));
                }
                return Ok(out);
            }
            _ => return Err("requirements value must be an array/object/string".to_owned()),
        };
        let mut out = Vec::with_capacity(entries.len());
        for entry in entries {
            let (raw, amount) = match entry {
                Value::String(text) => crate::mods::json::parse_stack(text)
                    .ok_or_else(|| format!("invalid stack `{text}`"))?,
                Value::Object(map) => {
                    let item = map
                        .get("item")
                        .and_then(Value::as_str)
                        .ok_or("stack missing item")?;
                    (
                        item.to_owned(),
                        map.get("amount").and_then(Value::as_i64).unwrap_or(1) as i32,
                    )
                }
                _ => return Err("invalid requirement entry".to_owned()),
            };
            let item = registry
                .item_id(&raw)
                .or_else(|| registry.item_id(&registry.transform_name(&raw)))
                .ok_or_else(|| format!("unknown item `{raw}`"))?;
            out.push(ItemStack::new(item, amount));
        }
        Ok(out)
    }

    pub(super) fn resolve_status_id(
        &self,
        registry: &ContentRegistry,
        raw: &str,
    ) -> Option<StatusId> {
        registry
            .status_id(raw)
            .or_else(|| registry.status_id(&registry.transform_name(raw)))
    }

    /// Records `(content, field)` once; returns whether a reset is needed.
    pub(super) fn mark_used(&mut self, reference: ContentRef, field: &str) -> bool {
        self.used.insert((reference, field.to_owned()))
    }

    pub(super) fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
}
