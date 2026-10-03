// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `mind-derive` — proc macros replacing the upstream `annotations/` entity IO
//! codegen (plan 04 §2.3 deviation 4 / R1).
//!
//! `#[derive(EntityIo)]` mirrors `annotations/.../entity/EntityIO.java`:
//! `write` emits the newest revision (`u16`) then save fields in declaration
//! order; `read` is a `match revision` chain assigning only fields that exist
//! in that revision; unknown revisions error. Field/struct attributes map 1:1
//! to upstream `@NoSerialize`/`@NoSync`/`@SyncLocal`/`@SyncField`:
//!
//! - struct: `#[entity(name = "BuildingComp", no_serialize, no_sync)]`
//! - field: `#[entity(since = N)]`, `#[entity(removed_in = N)]`,
//!   `#[entity(no_serialize)]`, `#[entity(no_sync)]`,
//!   `#[entity(sync_local)]`, `#[entity(transient)]`,
//!   `#[entity(sync_field(interp = "linear"|"angle", clamped))]`
//!
//! Field *removals* keep the struct field as a tombstone with
//! `removed_in = N` so old revision read chains still compile (the manifest
//! check in `io::entity::revisions` audits the result — renames require a bump
//! + alias entry, plan 04 deviation 3).

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// Derives `mind_core::io::entity::EntityCodec` for one entity def struct.
#[proc_macro_derive(EntityIo, attributes(entity))]
pub fn derive_entity_io(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(&input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[derive(Default)]
struct StructAttrs {
    name: Option<String>,
    no_serialize: bool,
    no_sync: bool,
    version: u8,
}

#[derive(Default)]
struct FieldAttrs {
    since: u16,
    removed_in: Option<u16>,
    no_serialize: bool,
    no_sync: bool,
    sync_local: bool,
    transient: bool,
    sync_field: bool,
    interp: InterpKind,
    clamped: bool,
}

#[derive(Default, Clone, Copy, PartialEq)]
enum InterpKind {
    #[default]
    None,
    Linear,
    Angle,
}

struct FieldInfo {
    ident: syn::Ident,
    ty: syn::Type,
    attrs: FieldAttrs,
}

fn expand(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let struct_attrs = parse_struct_attrs(input)?;
    let ident = &input.ident;
    let def_name = struct_attrs
        .name
        .clone()
        .unwrap_or_else(|| ident.to_string());
    let class_const = syn::Ident::new(&screaming_snake(&def_name), ident.span());

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => named
                .named
                .iter()
                .map(|field| {
                    let attrs = parse_field_attrs(field)?;
                    let ident = field
                        .ident
                        .clone()
                        .ok_or_else(|| syn::Error::new_spanned(field, "expected a named field"))?;
                    Ok(FieldInfo {
                        ident,
                        ty: field.ty.clone(),
                        attrs,
                    })
                })
                .collect::<syn::Result<Vec<FieldInfo>>>()?,
            _ => {
                return Err(syn::Error::new_spanned(
                    input,
                    "EntityIo requires named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "EntityIo can only be derived for structs",
            ));
        }
    };

    // Newest revision = max(since, removed_in) across fields (upstream's
    // newest manifest number).
    let newest: u16 = fields
        .iter()
        .map(|field| field.attrs.since.max(field.attrs.removed_in.unwrap_or(0)))
        .max()
        .unwrap_or(0);

    let serialize = !struct_attrs.no_serialize;
    let sync = !struct_attrs.no_sync;
    let tile_version = struct_attrs.version;

    // Save fields at the newest revision (declaration order, plan 04 §3.5).
    let save_fields: Vec<&FieldInfo> = fields
        .iter()
        .filter(|field| {
            !field.attrs.no_serialize && !field.attrs.transient && field.attrs.removed_in.is_none()
        })
        .collect();
    let sync_fields: Vec<&FieldInfo> = fields
        .iter()
        .filter(|field| {
            !field.attrs.no_sync && !field.attrs.transient && field.attrs.removed_in.is_none()
        })
        .collect();

    let field_descs = save_fields.iter().map(|field| {
        let name = field.ident.to_string();
        let ty = &field.ty;
        let flags = field_flag_tokens(&field.attrs);
        quote! {
            ::mind_core::io::entity::FieldDesc {
                name: #name,
                type_: <#ty as ::mind_core::io::entity::IoField>::TYPE_NAME,
                size: <#ty as ::mind_core::io::entity::IoField>::SIZE,
                flags: #flags,
            }
        }
    });

    let sync_field_metas = sync_fields.iter().map(|field| {
        let name = field.ident.to_string();
        let interp = match field.attrs.interp {
            InterpKind::Linear => quote!(::mind_core::io::entity::Interp::Linear),
            InterpKind::Angle => quote!(::mind_core::io::entity::Interp::Angle),
            InterpKind::None => quote!(::mind_core::io::entity::Interp::None),
        };
        let clamped = field.attrs.clamped;
        quote! {
            ::mind_core::io::entity::SyncFieldMeta {
                name: #name,
                interp: #interp,
                clamped: #clamped,
            }
        }
    });

    let save_writes = save_fields.iter().map(|field| {
        let fname = &field.ident;
        quote! {
            ::mind_core::io::entity::IoField::write_field(&self.#fname, w)?;
        }
    });
    let sync_writes = sync_fields.iter().map(|field| {
        let fname = &field.ident;
        quote! {
            ::mind_core::io::entity::IoField::write_field(&self.#fname, w)?;
        }
    });

    // One read arm per revision: fields visible at that revision.
    let mut read_arms = Vec::new();
    let mut sync_read_arms = Vec::new();
    for revision in 0..=newest {
        let reads: Vec<proc_macro2::TokenStream> = fields
            .iter()
            .filter(|field| {
                !field.attrs.no_serialize
                    && !field.attrs.transient
                    && field.attrs.since <= revision
                    && field
                        .attrs
                        .removed_in
                        .is_none_or(|removed| removed > revision)
            })
            .map(|field| {
                let fname = &field.ident;
                quote! {
                    ::mind_core::io::entity::IoField::read_field(&mut self.#fname, r)?;
                }
            })
            .collect();
        read_arms.push(quote! {
            #revision => {
                #(#reads)*
                Ok(())
            }
        });
        let sync_reads: Vec<proc_macro2::TokenStream> = fields
            .iter()
            .filter(|field| {
                !field.attrs.no_sync
                    && !field.attrs.transient
                    && field.attrs.since <= revision
                    && field
                        .attrs
                        .removed_in
                        .is_none_or(|removed| removed > revision)
            })
            .map(|field| {
                let fname = &field.ident;
                quote! {
                    ::mind_core::io::entity::IoField::read_field(&mut self.#fname, r)?;
                }
            })
            .collect();
        sync_read_arms.push(quote! {
            #revision => {
                #(#sync_reads)*
                Ok(())
            }
        });
    }

    Ok(quote! {
        impl ::mind_core::io::entity::EntityCodec for #ident {
            const NAME: &'static str = #def_name;
            const CLASS_ID: u8 = ::mind_core::io::entity::class_ids::#class_const;
            const SERIALIZE: bool = #serialize;
            const SYNC: bool = #sync;
            const NEWEST_REVISION: u16 = #newest;
            const TILE_VERSION: u8 = #tile_version;

            fn fields() -> &'static [::mind_core::io::entity::FieldDesc] {
                static FIELDS: &[::mind_core::io::entity::FieldDesc] = &[
                    #(#field_descs),*
                ];
                FIELDS
            }

            fn sync_fields() -> &'static [::mind_core::io::entity::SyncFieldMeta] {
                static SYNC_FIELDS: &[::mind_core::io::entity::SyncFieldMeta] = &[
                    #(#sync_field_metas),*
                ];
                SYNC_FIELDS
            }

            fn write(
                &self,
                w: &mut ::mind_core::io::entity::EntityWriter,
            ) -> ::mind_core::io::IoResult<()> {
                w.us(Self::NEWEST_REVISION);
                #(#save_writes)*
                Ok(())
            }

            fn read(
                &mut self,
                r: &mut ::mind_core::io::entity::EntityReader,
                revision: u16,
            ) -> ::mind_core::io::IoResult<()> {
                match revision {
                    #(#read_arms)*
                    _ => Err(::mind_core::io::IoError::UnknownRevision {
                        name: Self::NAME.to_owned(),
                        revision,
                    }),
                }
            }

            fn write_sync(
                &self,
                w: &mut ::mind_core::io::entity::EntityWriter,
            ) -> ::mind_core::io::IoResult<()> {
                w.us(Self::NEWEST_REVISION);
                #(#sync_writes)*
                Ok(())
            }

            fn read_sync(
                &mut self,
                r: &mut ::mind_core::io::entity::EntityReader,
                revision: u16,
            ) -> ::mind_core::io::IoResult<()> {
                match revision {
                    #(#sync_read_arms)*
                    _ => Err(::mind_core::io::IoError::UnknownRevision {
                        name: Self::NAME.to_owned(),
                        revision,
                    }),
                }
            }
        }
    })
}

fn field_flag_tokens(attrs: &FieldAttrs) -> proc_macro2::TokenStream {
    let mut flags = Vec::new();
    if attrs.transient {
        flags.push(quote!(::mind_core::io::entity::FieldFlag::Transient));
    } else {
        if !attrs.no_serialize {
            flags.push(quote!(::mind_core::io::entity::FieldFlag::Save));
        }
        if !attrs.no_sync {
            flags.push(quote!(::mind_core::io::entity::FieldFlag::Sync));
        }
    }
    if attrs.sync_local {
        flags.push(quote!(::mind_core::io::entity::FieldFlag::SyncLocal));
    }
    match attrs.interp {
        InterpKind::Linear => flags.push(quote!(::mind_core::io::entity::FieldFlag::InterpLinear)),
        InterpKind::Angle => flags.push(quote!(::mind_core::io::entity::FieldFlag::InterpAngle)),
        InterpKind::None => {}
    }
    if attrs.clamped {
        flags.push(quote!(::mind_core::io::entity::FieldFlag::Clamped));
    }
    quote! { &[#(#flags),*] }
}

fn parse_struct_attrs(input: &DeriveInput) -> syn::Result<StructAttrs> {
    let mut out = StructAttrs::default();
    for attr in &input.attrs {
        if !attr.path().is_ident("entity") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let value: syn::LitStr = meta.value()?.parse()?;
                out.name = Some(value.value());
                return Ok(());
            }
            if meta.path.is_ident("no_serialize") {
                out.no_serialize = true;
                return Ok(());
            }
            if meta.path.is_ident("no_sync") {
                out.no_sync = true;
                return Ok(());
            }
            if meta.path.is_ident("version") {
                let value: syn::LitInt = meta.value()?.parse()?;
                out.version = value.base10_parse()?;
                return Ok(());
            }
            Err(meta.error("unsupported entity attribute"))
        })?;
    }
    Ok(out)
}

fn parse_field_attrs(field: &syn::Field) -> syn::Result<FieldAttrs> {
    let mut out = FieldAttrs::default();
    for attr in &field.attrs {
        if !attr.path().is_ident("entity") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("since") {
                let value: syn::LitInt = meta.value()?.parse()?;
                out.since = value.base10_parse()?;
                return Ok(());
            }
            if meta.path.is_ident("removed_in") {
                let value: syn::LitInt = meta.value()?.parse()?;
                out.removed_in = Some(value.base10_parse()?);
                return Ok(());
            }
            if meta.path.is_ident("no_serialize") {
                out.no_serialize = true;
                return Ok(());
            }
            if meta.path.is_ident("no_sync") {
                out.no_sync = true;
                return Ok(());
            }
            if meta.path.is_ident("sync_local") {
                out.sync_local = true;
                return Ok(());
            }
            if meta.path.is_ident("transient") {
                out.transient = true;
                return Ok(());
            }
            if meta.path.is_ident("clamped") {
                out.clamped = true;
                return Ok(());
            }
            if meta.path.is_ident("sync_field") {
                out.sync_field = true;
                meta.parse_nested_meta(|nested| {
                    if nested.path.is_ident("interp") {
                        let value: syn::LitStr = nested.value()?.parse()?;
                        out.interp = match value.value().as_str() {
                            "linear" => InterpKind::Linear,
                            "angle" => InterpKind::Angle,
                            other => {
                                return Err(
                                    nested.error(format!("unknown interpolation kind: {other}"))
                                );
                            }
                        };
                        return Ok(());
                    }
                    if nested.path.is_ident("clamped") {
                        out.clamped = true;
                        return Ok(());
                    }
                    Err(nested.error("unsupported sync_field attribute"))
                })?;
                return Ok(());
            }
            Err(meta.error("unsupported entity attribute"))
        })?;
    }
    Ok(out)
}

/// Derives `mind_core::editor::objectives::ObjectiveFields` from
/// `#[objective(name = "...", kind = "...", flags = "...")]` field attributes
/// (plan 19 §2.3.2/§3.9, OD19-F: no Java reflection, descriptors are data).
///
/// Supported `kind` values: `string`, `bool`, `byte`, `int`, `float`, `team`,
/// `color`, `vec2f`, `vec2i`, `content:<item|block|unit>`, `seq:<kind>`,
/// `map:<kind>`. `flags` is a `|`/`,`-separated list of `second`, `tilepos`,
/// `multiline`, `logiccode`, `researchable`, `synthetic`, `hidden`.
#[proc_macro_derive(ObjectiveFields, attributes(objective))]
pub fn derive_objective_fields(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_objective_fields(&input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_objective_fields(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => named.named.iter().collect::<Vec<_>>(),
            _ => {
                return Err(syn::Error::new_spanned(
                    input,
                    "ObjectiveFields requires named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "ObjectiveFields can only be derived for structs",
            ));
        }
    };

    let mut descriptors = Vec::new();
    for field in fields {
        let mut name: Option<String> = None;
        let mut kind: Option<proc_macro2::TokenStream> = None;
        let mut flags: Option<proc_macro2::TokenStream> = None;
        for attr in &field.attrs {
            if !attr.path().is_ident("objective") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    name = Some(value.value());
                    return Ok(());
                }
                if meta.path.is_ident("kind") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    kind = Some(
                        parse_field_kind(&value.value())
                            .map_err(|error| syn::Error::new(value.span(), error))?,
                    );
                    return Ok(());
                }
                if meta.path.is_ident("flags") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    flags = Some(parse_field_flags(&value.value()));
                    return Ok(());
                }
                Err(meta.error("unsupported objective attribute"))
            })?;
        }
        let Some(name) = name else { continue };
        let kind = kind.ok_or_else(|| syn::Error::new_spanned(field, "missing kind"))?;
        let flags =
            flags.unwrap_or_else(|| quote!(::mind_core::editor::objectives::FieldFlags::empty()));
        descriptors.push(quote! {
            ::mind_core::editor::objectives::ObjectiveField::new(#name, #kind, #flags)
        });
    }

    Ok(quote! {
        impl ::mind_core::editor::objectives::ObjectiveFields for #ident {
            fn fields(&self) -> Vec<::mind_core::editor::objectives::ObjectiveField> {
                vec![#(#descriptors),*]
            }
        }
    })
}

fn parse_field_kind(kind: &str) -> Result<proc_macro2::TokenStream, String> {
    let path = quote!(::mind_core::editor::objectives::FieldKind);
    let simple = match kind {
        "string" => quote!(#path::String),
        "bool" => quote!(#path::Bool),
        "byte" => quote!(#path::Byte),
        "int" => quote!(#path::Int),
        "float" => quote!(#path::Float),
        "team" => quote!(#path::Team),
        "color" => quote!(#path::Color),
        "vec2f" => quote!(#path::Vec2F),
        "vec2i" => quote!(#path::Vec2I),
        "content:item" => quote!(#path::Content(::mind_core::content::ContentType::Item)),
        "content:block" => quote!(#path::Content(::mind_core::content::ContentType::Block)),
        "content:unit" => quote!(#path::Content(::mind_core::content::ContentType::Unit)),
        other => {
            if let Some(inner) = other.strip_prefix("seq:") {
                let inner = parse_field_kind(inner)?;
                quote!(#path::Seq(::std::boxed::Box::new(#inner)))
            } else if let Some(inner) = other.strip_prefix("map:") {
                let inner = parse_field_kind(inner)?;
                quote!(#path::Map(::std::boxed::Box::new(#inner)))
            } else {
                return Err(format!("unknown objective field kind `{other}`"));
            }
        }
    };
    Ok(simple)
}

fn parse_field_flags(flags: &str) -> proc_macro2::TokenStream {
    let path = quote!(::mind_core::editor::objectives::FieldFlags);
    let mut out = quote!(#path::empty());
    for flag in flags
        .split(['|', ','])
        .map(str::trim)
        .filter(|f| !f.is_empty())
    {
        let constant = match flag {
            "second" => quote!(#path::SECOND),
            "tilepos" => quote!(#path::TILE_POS),
            "multiline" => quote!(#path::MULTILINE),
            "logiccode" => quote!(#path::LOGIC_CODE),
            "researchable" => quote!(#path::RESEARCHABLE),
            "synthetic" => quote!(#path::SYNTHETIC),
            "hidden" => quote!(#path::HIDDEN),
            other => {
                let message = format!("unknown objective flag `{other}`");
                quote!(compile_error!(#message))
            }
        };
        out = quote!(#out.union(#constant));
    }
    out
}

/// `BuildingComp` → `BUILDING_COMP`, `alpha` → `ALPHA` (class-const names,
/// must match `mind_core::io::entity::class_ids` and the check-class-ids tool).
fn screaming_snake(name: &str) -> String {
    let mut out = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() && index > 0 {
            out.push('_');
        }
        out.push(ch);
    }
    out.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screaming_snake_conversion() {
        assert_eq!(screaming_snake("BuildingComp"), "BUILDING_COMP");
        assert_eq!(screaming_snake("alpha"), "ALPHA");
        assert_eq!(screaming_snake("PosTeamDef"), "POS_TEAM_DEF");
        assert_eq!(screaming_snake("WeatherStateComp"), "WEATHER_STATE_COMP");
    }
}
