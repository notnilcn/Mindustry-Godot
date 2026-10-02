// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `mind-macros` — proc-macro crate for sim component/entity metadata.
//!
//! Replaces the build-time Java annotation processor in
//! `annotations/src/main/java/mindustry/annotations/` with Rust derive/function
//! macros that emit *metadata only* (HLP plan 05 §3.5). No method bodies are
//! merged: composition is data + explicitly ordered systems.
//!
//! Public API (stable for downstream lanes):
//! - `#[derive(SimComponent)]` with `#[sim(...)]` field attributes emits an impl
//!   of `::mind_core::entities::meta::SimComponentMeta`.
//! - `entity_def! { Name = [Comp, ...]; ... }` emits
//!   `::mind_core::entities::meta::ENTITY_DEF_SPECS`.
//!
//! Adding a new derive (e.g. plan 03's `LoadRegions`) is purely additive: add a
//! `#[proc_macro_derive(Name, attributes(...))]` fn below that emits an impl of a
//! trait owned by the consuming crate (see the hand-off note in
//! `05_SIM_CORE_IMPLEMENTATION_PLAN.md` Changelog).

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DeriveInput, Fields, GenericArgument, Ident, PathArguments, Token, Type,
    parse_macro_input,
};

/// Parsed `#[sim(...)]` options for one struct or field.
#[derive(Default)]
struct SimOptions {
    name: Option<String>,
    base: bool,
    methods: Vec<(String, i32)>,
    kind: FieldKindOpt,
    since: u16,
}

/// Field-level `FieldKind` selection.
#[derive(Default, Clone)]
enum FieldKindOpt {
    #[default]
    Plain,
    SyncFloat {
        clamped: bool,
        interp: bool,
    },
    SyncLocal,
    NoSync,
    NoSerialize,
    Transient,
    ReadOnly,
}

impl SimOptions {
    fn merge(&mut self, attr: &Attribute) -> syn::Result<()> {
        if !attr.path().is_ident("sim") {
            return Ok(());
        }
        attr.parse_nested_meta(|meta| {
            let ident = meta
                .path
                .get_ident()
                .map(Ident::to_string)
                .unwrap_or_default();
            match ident.as_str() {
                "component" => {}
                "base" => self.base = true,
                "name" => {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    self.name = Some(value.value());
                }
                "since" => {
                    let value: syn::LitInt = meta.value()?.parse()?;
                    self.since = value.base10_parse()?;
                }
                "sync_float" => {
                    let mut clamped = false;
                    let mut interp = false;
                    if meta.input.peek(syn::token::Paren) {
                        meta.parse_nested_meta(|inner| {
                            match inner
                                .path
                                .get_ident()
                                .map(Ident::to_string)
                                .unwrap_or_default()
                                .as_str()
                            {
                                "clamped" => clamped = true,
                                "interp" => interp = true,
                                _ => {}
                            }
                            Ok(())
                        })?;
                    }
                    self.kind = FieldKindOpt::SyncFloat { clamped, interp };
                }
                "sync_local" => self.kind = FieldKindOpt::SyncLocal,
                "no_sync" => self.kind = FieldKindOpt::NoSync,
                "no_serialize" => self.kind = FieldKindOpt::NoSerialize,
                "transient" => self.kind = FieldKindOpt::Transient,
                "read_only" => self.kind = FieldKindOpt::ReadOnly,
                "methods" => {
                    meta.parse_nested_meta(|inner| {
                        let method = inner
                            .path
                            .get_ident()
                            .map(Ident::to_string)
                            .unwrap_or_default();
                        let value: syn::LitInt = inner.value()?.parse()?;
                        self.methods.push((method, value.base10_parse()?));
                        Ok(())
                    })?;
                }
                _ => {}
            }
            Ok(())
        })
    }

    fn kind_tokens(&self) -> proc_macro2::TokenStream {
        match &self.kind {
            FieldKindOpt::Plain => quote!(::mind_core::entities::meta::FieldKind::Plain),
            FieldKindOpt::SyncFloat { clamped, interp } => quote!(
                ::mind_core::entities::meta::FieldKind::SyncFloat {
                    clamped: #clamped,
                    interp: #interp,
                }
            ),
            FieldKindOpt::SyncLocal => quote!(::mind_core::entities::meta::FieldKind::SyncLocal),
            FieldKindOpt::NoSync => quote!(::mind_core::entities::meta::FieldKind::NoSync),
            FieldKindOpt::NoSerialize => {
                quote!(::mind_core::entities::meta::FieldKind::NoSerialize)
            }
            FieldKindOpt::Transient => quote!(::mind_core::entities::meta::FieldKind::Transient),
            FieldKindOpt::ReadOnly => quote!(::mind_core::entities::meta::FieldKind::ReadOnly),
        }
    }
}

/// Maps a Rust field type to the metadata `FieldType` vocabulary.
fn field_type_tokens(ty: &Type) -> proc_macro2::TokenStream {
    let meta = quote!(::mind_core::entities::meta);
    match ty {
        Type::Path(path) => {
            let Some(segment) = path.path.segments.last() else {
                return quote!(#meta::FieldType::Struct("?"));
            };
            let name = segment.ident.to_string();
            match name.as_str() {
                "f32" => quote!(#meta::FieldType::F32),
                "f64" => quote!(#meta::FieldType::F64),
                "i8" => quote!(#meta::FieldType::I8),
                "i16" => quote!(#meta::FieldType::I16),
                "i32" => quote!(#meta::FieldType::I32),
                "i64" => quote!(#meta::FieldType::I64),
                "u8" => quote!(#meta::FieldType::U8),
                "u16" => quote!(#meta::FieldType::U16),
                "u32" => quote!(#meta::FieldType::U32),
                "u64" => quote!(#meta::FieldType::U64),
                "bool" => quote!(#meta::FieldType::Bool),
                "Entity" => quote!(#meta::FieldType::Entity),
                "BlockId" | "ItemId" | "LiquidId" | "UnitTypeId" | "ContentId" => {
                    quote!(#meta::FieldType::Content)
                }
                "Option" | "Vec" => {
                    let inner = generic_inner(segment)
                        .map_or_else(|| quote!(#meta::FieldType::Struct("?")), field_type_tokens);
                    let variant = format_ident!("{}", name);
                    quote!(#meta::FieldType::#variant(&#inner))
                }
                other => {
                    let other = syn::LitStr::new(other, segment.ident.span());
                    quote!(#meta::FieldType::Struct(#other))
                }
            }
        }
        Type::Reference(reference) => field_type_tokens(&reference.elem),
        _ => quote!(#meta::FieldType::Struct("?")),
    }
}

fn generic_inner(segment: &syn::PathSegment) -> Option<&Type> {
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    args.args.iter().find_map(|arg| match arg {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    })
}

/// Derives the `SimComponentMeta` metadata impl for a component struct.
///
/// ```ignore
/// #[derive(Component, SimComponent)]
/// #[sim(component, base, methods(update_priority = 0))]
/// pub struct Health {
///     pub health: f32,
///     #[sim(sync_local)] pub elevation: f32,
/// }
/// ```
#[proc_macro_derive(SimComponent, attributes(sim))]
pub fn derive_sim_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_sim_component(&input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_sim_component(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let mut options = SimOptions::default();
    for attr in &input.attrs {
        options.merge(attr)?;
    }
    let name = options.name.clone().unwrap_or_else(|| ident.to_string());
    let base = options.base;

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "SimComponent can only be derived for structs",
        ));
    };
    // Unit structs (marker components) and unnamed fields contribute no
    // serialized metadata.
    let named = match &data.fields {
        Fields::Named(fields) => Some(&fields.named),
        Fields::Unit => None,
        Fields::Unnamed(_) => None,
    };

    let mut field_metas = Vec::new();
    for field in named.into_iter().flatten() {
        let Some(field_ident) = &field.ident else {
            continue;
        };
        let mut field_options = SimOptions::default();
        for attr in &field.attrs {
            field_options.merge(attr)?;
        }
        let field_name = field_options
            .name
            .clone()
            .unwrap_or_else(|| field_ident.to_string());
        let field_name = syn::LitStr::new(&field_name, field_ident.span());
        let kind = field_options.kind_tokens();
        let ty = field_type_tokens(&field.ty);
        let since = field_options.since;
        field_metas.push(quote! {
            ::mind_core::entities::meta::FieldMeta {
                name: #field_name,
                kind: #kind,
                ty: #ty,
                revision_added: #since,
            }
        });
    }

    let method_metas: Vec<_> = options
        .methods
        .iter()
        .map(|(method, priority)| {
            let method = syn::LitStr::new(method, ident.span());
            quote! {
                ::mind_core::entities::meta::SystemOrder {
                    method: #method,
                    priority: #priority,
                }
            }
        })
        .collect();

    let name_lit = syn::LitStr::new(&name, ident.span());
    Ok(quote! {
        impl ::mind_core::entities::meta::SimComponentMeta for #ident {
            fn component_meta() -> &'static ::mind_core::entities::meta::ComponentMeta {
                static META: ::mind_core::entities::meta::ComponentMeta =
                    ::mind_core::entities::meta::ComponentMeta {
                        name: #name_lit,
                        base: #base,
                        fields: &[#(#field_metas),*],
                        methods: &[#(#method_metas),*],
                    };
                &META
            }
        }
    })
}

/// One `Name = [Component, ...];` entry.
struct EntityDefEntry {
    name: Ident,
    components: Vec<syn::Path>,
}

impl Parse for EntityDefEntry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let content;
        syn::bracketed!(content in input);
        let components = Punctuated::<syn::Path, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        input.parse::<Token![;]>()?;
        Ok(Self { name, components })
    }
}

struct EntityDefInput {
    entries: Vec<EntityDefEntry>,
}

impl Parse for EntityDefInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut entries = Vec::new();
        while !input.is_empty() {
            entries.push(input.parse()?);
        }
        Ok(Self { entries })
    }
}

/// Declares entity archetypes and emits `mind_core::entities::meta::ENTITY_DEF_SPECS`.
///
/// ```ignore
/// entity_def! {
///     Building = [BaseEntity, SimId, DefId, Pos, Vel, TeamComp];
///     Unit = [BaseEntity, SimId, DefId, Pos, Vel, TeamComp, Health];
/// }
/// ```
///
/// Component names are captured as strings; the `EntityRegistry` validates them
/// against registered `SimComponentMeta` names and computes each def's
/// `GroupMask` using the exact `EntityProcess.java:273` rule (plan 05 §3.6).
#[proc_macro]
pub fn entity_def(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as EntityDefInput);
    let specs = input.entries.iter().map(|entry| {
        let name = syn::LitStr::new(&entry.name.to_string(), entry.name.span());
        let components = entry.components.iter().map(|component| {
            let name = component
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
                .unwrap_or_default();
            syn::LitStr::new(&name, component.span())
        });
        quote! {
            ::mind_core::entities::meta::EntityDefSpec {
                name: #name,
                components: &[#(#components),*],
            }
        }
    });
    let output = quote! {
        /// Entity archetype specs emitted by `entity_def!` (plan 05 §3.5.1).
        pub static ENTITY_DEF_SPECS: &[::mind_core::entities::meta::EntityDefSpec] =
            &[#(#specs),*];
    };
    output.into()
}
