// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Type, parse_macro_input};

/// This proc macro generates getters, attribute lookup, etc for protocol config fields of type `Option<T>`
/// and for the feature flags
/// Example for a field: `new_constant: Option<u64>`, and for feature flags `feature: bool`, we derive
/// ```rust,ignore
///     /// Returns the value of the field if exists at the given version, otherise panic
///     pub fn new_constant(&self) -> u64 {
///         self.new_constant.expect(Self::CONSTANT_ERR_MSG)
///     }
///     /// Returns the value of the field if exists at the given version, otherise None.
///     pub fn new_constant_as_option(&self) -> Option<u64> {
///         self.new_constant
///     }
///     // We auto derive an enum such that the variants are all the types of the fields
///     pub enum ProtocolConfigValue {
///        u32(u32),
///        u64(u64),
///        ..............
///     }
///     // This enum is used to return field values so that the type is also encoded in the response
///
///     /// Returns the value of the field if exists at the given version, otherise None
///     pub fn lookup_attr(&self, value: String) -> Option<ProtocolConfigValue>;
///
///     /// Returns a map of all configs to values
///     pub fn attr_map(&self) -> std::collections::BTreeMap<String, Option<ProtocolConfigValue>>;
///
///     /// Returns a feature by the string name or None if it doesn't exist
///     pub fn lookup_feature(&self, value: String) -> Option<bool>;
///
///     /// Returns a map of all features to values
///     pub fn feature_map(&self) -> std::collections::BTreeMap<String, bool>;
/// ```
///
/// Every field (scalar and non-scalar) is also emitted into a typed
/// `render<F: Format>(&self, meter: &mut impl Meter) -> Result<BTreeMap<String, Option<F>>, MeterError>`
/// method, where each value is produced via `mysten_common::rpc_format::ToFormat`. This is the
/// path RPC code should use to expose protocol config to clients; the same call site can target
/// `serde_json::Value`, `prost_types::Value`, or any other `Format` impl by choosing `F`.
///
/// Scalar (`u16`/`u32`/`u64`/`bool`) fields continue to feed `ProtocolConfigValue` / `attr_map`
/// for back-compat with existing consumers. Non-scalar fields appear only in `render`. Add
/// `#[skip_accessor]` to keep a field internal and out of every generated surface.
#[proc_macro_derive(ProtocolConfigAccessors, attributes(skip_accessor))]
pub fn accessors_macro(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    let struct_name = &ast.ident;
    let data = &ast.data;

    let fields: Vec<AccessorField> = match data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields_named) => fields_named
                .named
                .iter()
                .filter_map(parse_accessor_field)
                .collect(),
            _ => panic!("Only named fields are supported."),
        },
        _ => panic!("Only structs supported."),
    };

    let expanded: Vec<ExpandedField> = fields.iter().map(expand_field).collect();

    let accessors = expanded.iter().map(|e| &e.accessor);
    let setters = expanded.iter().map(|e| &e.setter);
    let render_arms = expanded.iter().map(|e| &e.render_arm);

    // Scalar-only collections — driven by the optional `ScalarExtras` extension on each field.
    let scalar_extras: Vec<&ScalarExtras> = expanded
        .iter()
        .filter_map(|e| e.scalar_extras.as_ref())
        .collect();
    let scalar_value_setter_arms = scalar_extras.iter().map(|s| &s.value_setter_arm);
    let scalar_lookup_arms = scalar_extras.iter().map(|s| &s.lookup_arm);
    let scalar_field_names = scalar_extras.iter().map(|s| &s.field_name_str);

    // Multiple scalar fields of the same primitive type all share a single
    // `ProtocolConfigValue` variant (e.g. every `u64` field maps to `ProtocolConfigValue::u64`).
    let mut variant_decls = Vec::new();
    let mut display_variants = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for s in &scalar_extras {
        if !seen.insert(s.variant_ident.to_string()) {
            continue;
        }
        let ident = &s.variant_ident;
        let inner = &s.inner_type;
        variant_decls.push(quote! { #ident(#inner) });
        display_variants.push(s.variant_ident.clone());
    }

    let output = quote! {
        impl #struct_name {
            const CONSTANT_ERR_MSG: &'static str = "protocol constant not present in current protocol version";
            #(#accessors)*

            /// Lookup a scalar config attribute by its string representation.
            pub fn lookup_attr(&self, value: String) -> Option<ProtocolConfigValue> {
                match value.as_str() {
                    #(#scalar_lookup_arms)*
                    _ => None,
                }
            }

            /// Get a map of all scalar config attributes from string representations.
            ///
            /// Non-scalar (e.g. list-typed) fields aren't represented here — use
            /// `Self::render` for a typed view that includes every field.
            pub fn attr_map(&self) -> std::collections::BTreeMap<String, Option<ProtocolConfigValue>> {
                vec![
                    #(((#scalar_field_names).to_owned(), self.lookup_attr((#scalar_field_names).to_owned())),)*
                    ].into_iter().collect()
            }

            /// Render every protocol-config attribute into the chosen `Format`.
            ///
            /// The value is `Some(...)` when the field is set in this protocol version and
            /// `None` when the field isn't configured at this version (matching `attr_map`'s
            /// existing semantics for scalars).
            pub fn render<F>(
                &self,
                meter: &mut impl ::mysten_common::rpc_format::Meter,
            ) -> ::std::result::Result<
                std::collections::BTreeMap<String, Option<F>>,
                ::mysten_common::rpc_format::MeterError,
            >
            where
                F: ::mysten_common::rpc_format::Format,
            {
                let mut map = std::collections::BTreeMap::new();
                #(#render_arms)*
                Ok(map)
            }

            /// Get the feature flags
            pub fn lookup_feature(&self, value: String) -> Option<bool> {
                self.feature_flags.lookup_attr(value)
            }

            pub fn feature_map(&self) -> std::collections::BTreeMap<String, bool> {
                self.feature_flags.attr_map()
            }
        }

        impl #struct_name {
            #(#setters)*

            pub fn set_attr_for_testing(&mut self, attr: String, val: String) {
                match attr.as_str() {
                    #(#scalar_value_setter_arms)*
                    _ => panic!(
                        "Attempting to set unknown or non-string-settable attribute: {}",
                        attr,
                    ),
                }
            }
        }

        #[allow(non_camel_case_types)]
        #[derive(Clone, Serialize, Debug, PartialEq, Deserialize, schemars::JsonSchema)]
        pub enum ProtocolConfigValue {
            #(#variant_decls,)*
        }

        impl std::fmt::Display for ProtocolConfigValue {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                use std::fmt::Write;
                let mut writer = String::new();
                match self {
                    #(
                        ProtocolConfigValue::#display_variants(x) => {
                            write!(writer, "{}", x)?;
                        }
                    )*
                }
                write!(f, "{}", writer)
            }
        }
    };

    TokenStream::from(output)
}

/// Token streams emitted for a single `ProtocolConfig` field. Every field contributes an
/// accessor, a setter, and a `render` arm; scalar fields additionally populate
/// [`ScalarExtras`] for the `ProtocolConfigValue` / `attr_map` / `set_attr_for_testing` paths.
struct ExpandedField {
    /// `fn field_name(&self) -> T` (scalars only) + `fn field_name_as_option(&self) -> Option<T>`.
    accessor: proc_macro2::TokenStream,
    /// `set_x_for_testing` + `disable_x_for_testing` (always) and the `from_str` variant
    /// (scalars only — non-scalars don't generally implement `FromStr`).
    setter: proc_macro2::TokenStream,
    /// One per-field block of `render` that calls `ToFormat::to_format` and inserts the result.
    render_arm: proc_macro2::TokenStream,
    /// Populated only when the field is a scalar (i.e. inner type is a single bare identifier
    /// usable as a `ProtocolConfigValue` variant ident — `u16`/`u32`/`u64`/`bool`).
    scalar_extras: Option<ScalarExtras>,
}

/// The extra tokens scalar fields contribute on top of the always-emitted accessor/setter/render
/// pieces. Non-scalar fields don't participate in `ProtocolConfigValue` at all.
struct ScalarExtras {
    /// `stringify!(field_name) => self.set_x_from_str_for_testing(val),` — match arm for
    /// `set_attr_for_testing`.
    value_setter_arm: proc_macro2::TokenStream,
    /// `stringify!(field_name) => self.field_name.map(ProtocolConfigValue::Variant),` — match
    /// arm for `lookup_attr`.
    lookup_arm: proc_macro2::TokenStream,
    /// `stringify!(field_name)` — used to assemble `attr_map`.
    field_name_str: proc_macro2::TokenStream,
    /// Variant identifier used in `ProtocolConfigValue`.
    variant_ident: syn::Ident,
    /// Inner `T` of `Option<T>` — pairs with `variant_ident` when declaring the enum variant.
    inner_type: syn::Type,
}

fn expand_field(f: &AccessorField) -> ExpandedField {
    let field_name = &f.field_name;
    let field_type = &f.field_type;
    let inner_type = &f.inner_type;
    let as_option_name: proc_macro2::TokenStream =
        format!("{field_name}_as_option").parse().unwrap();
    let test_setter_name: proc_macro2::TokenStream =
        format!("set_{field_name}_for_testing").parse().unwrap();
    let test_un_setter_name: proc_macro2::TokenStream =
        format!("disable_{field_name}_for_testing").parse().unwrap();

    let render_arm = quote! {
        {
            let value = self
                .#field_name
                .as_ref()
                .map(|v| <_ as ::mysten_common::rpc_format::ToFormat>::to_format::<F, _>(v, meter))
                .transpose()?;
            map.insert(stringify!(#field_name).to_owned(), value);
        }
    };

    // `_as_option` is always emitted. The plain getter and the string-based setter only make
    // sense for scalars (the plain getter unwraps to a `Copy` primitive; the `from_str` setter
    // requires `FromStr` on the inner type). Non-scalars provide custom getters next to the
    // field definition when they want one (e.g. borrowed-slice ergonomics).
    let as_option_emit = quote! {
        pub fn #as_option_name(&self) -> #field_type {
            self.#field_name.clone()
        }
    };
    let common_setters = quote! {
        pub fn #test_setter_name(&mut self, val: #inner_type) {
            self.#field_name = Some(val);
        }

        pub fn #test_un_setter_name(&mut self) {
            self.#field_name = None;
        }
    };

    match &f.scalar_variant {
        Some(variant_ident) => {
            let test_setter_from_str_name: proc_macro2::TokenStream =
                format!("set_{field_name}_from_str_for_testing")
                    .parse()
                    .unwrap();
            ExpandedField {
                accessor: quote! {
                    pub fn #field_name(&self) -> #inner_type {
                        self.#field_name.expect(Self::CONSTANT_ERR_MSG)
                    }

                    pub fn #as_option_name(&self) -> #field_type {
                        self.#field_name
                    }
                },
                setter: quote! {
                    #common_setters

                    pub fn #test_setter_from_str_name(&mut self, val: String) {
                        use std::str::FromStr;
                        self.#test_setter_name(#inner_type::from_str(&val).unwrap());
                    }
                },
                render_arm,
                scalar_extras: Some(ScalarExtras {
                    value_setter_arm: quote! {
                        stringify!(#field_name) => self.#test_setter_from_str_name(val),
                    },
                    lookup_arm: quote! {
                        stringify!(#field_name) => self
                            .#field_name
                            .map(ProtocolConfigValue::#variant_ident),
                    },
                    field_name_str: quote! { stringify!(#field_name) },
                    variant_ident: variant_ident.clone(),
                    inner_type: inner_type.clone(),
                }),
            }
        }
        None => ExpandedField {
            accessor: as_option_emit,
            setter: common_setters,
            render_arm,
            scalar_extras: None,
        },
    }
}

/// Per-field metadata extracted from a `ProtocolConfig` field while expanding the
/// `ProtocolConfigAccessors` derive.
struct AccessorField {
    /// The `#field_name` identifier — used both for accessor method names and as the string key
    /// in the generated maps.
    field_name: syn::Ident,
    /// The full `Option<T>` type as written in the struct.
    field_type: syn::Type,
    /// The inner `T` extracted from `Option<T>`.
    inner_type: syn::Type,
    /// `Some(ident)` when the inner type is a single bare identifier usable directly as a
    /// `ProtocolConfigValue` variant ident (`u16`/`u32`/`u64`/`bool`). `None` for non-scalar
    /// fields, which never appear in `ProtocolConfigValue`.
    scalar_variant: Option<syn::Ident>,
}

fn parse_accessor_field(field: &syn::Field) -> Option<AccessorField> {
    let field_name = field.ident.clone().expect("Field must be named");

    let skip_accessor = field
        .attrs
        .iter()
        .any(|attr| attr.path.is_ident("skip_accessor"));
    if skip_accessor {
        return None;
    }

    let field_type = &field.ty;
    let type_path = match field_type {
        Type::Path(p) => p,
        _ => return None,
    };
    let last_segment = type_path.path.segments.last()?;
    if last_segment.ident != "Option" {
        return None;
    }
    let inner_type = match &last_segment.arguments {
        syn::PathArguments::AngleBracketed(args) => match args.args.first()? {
            syn::GenericArgument::Type(ty) => ty.clone(),
            _ => panic!("Expected a type argument inside Option<...> for `{field_name}`"),
        },
        _ => panic!("Expected angle bracketed arguments inside Option<...> for `{field_name}`"),
    };

    let scalar_variant = inferred_scalar_variant_ident(&inner_type);

    Some(AccessorField {
        field_name,
        field_type: field_type.clone(),
        inner_type,
        scalar_variant,
    })
}

fn inferred_scalar_variant_ident(ty: &syn::Type) -> Option<syn::Ident> {
    const SCALAR_PRIMITIVES: &[&str] = &["bool", "u8", "u16", "u32", "u64", "u128", "usize"];

    let Type::Path(path) = ty else { return None };
    if path.qself.is_some() {
        return None;
    }
    if path.path.segments.len() != 1 {
        return None;
    }
    let segment = path.path.segments.first()?;
    if !matches!(segment.arguments, syn::PathArguments::None) {
        return None;
    }
    if !SCALAR_PRIMITIVES.iter().any(|p| segment.ident == *p) {
        return None;
    }
    Some(segment.ident.clone())
}

#[proc_macro_derive(ProtocolConfigOverride)]
pub fn protocol_config_override_macro(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    // Create a new struct name by appending "Optional".
    let struct_name = &ast.ident;
    let optional_struct_name =
        syn::Ident::new(&format!("{}Optional", struct_name), struct_name.span());

    // Extract the fields from the struct
    let fields = match &ast.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields_named) => &fields_named.named,
            _ => panic!("ProtocolConfig must have named fields"),
        },
        _ => panic!("ProtocolConfig must be a struct"),
    };

    // Create new fields with types wrapped in Option.
    let optional_fields = fields.iter().map(|field| {
        let field_name = &field.ident;
        let field_type = &field.ty;
        quote! {
            #field_name: Option<#field_type>
        }
    });

    // Generate the function to update the original struct.
    let update_fields = fields.iter().map(|field| {
        let field_name = &field.ident;
        quote! {
            if let Some(value) = self.#field_name {
                tracing::warn!(
                    "ProtocolConfig field \"{}\" has been overridden with the value: {value:?}",
                    stringify!(#field_name),
                );
                config.#field_name = value;
            }
        }
    });

    // Generate the new struct definition.
    let output = quote! {
        #[derive(serde::Deserialize, Debug)]
        pub struct #optional_struct_name {
            #(#optional_fields,)*
        }

        impl #optional_struct_name {
            pub fn apply_to(self, config: &mut #struct_name) {
                #(#update_fields)*
            }
        }
    };

    TokenStream::from(output)
}

#[proc_macro_derive(ProtocolConfigFeatureFlagsGetters, attributes(skip_accessor))]
pub fn feature_flag_getters_macro(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    let struct_name = &ast.ident;
    let data = &ast.data;

    let getters = match data {
        Data::Struct(data_struct) => match &data_struct.fields {
            // Operate on each field of the ProtocolConfig struct
            Fields::Named(fields_named) => fields_named.named.iter().filter_map(|field| {
                // Extract field name and type
                let field_name = field.ident.as_ref().expect("Field must be named");
                let field_type = &field.ty;
                let skip_accessor = field
                    .attrs
                    .iter()
                    .any(|attr| attr.path.is_ident("skip_accessor"));
                if skip_accessor {
                    return None;
                }
                // Check if field is of type bool
                match field_type {
                    Type::Path(type_path)
                        if type_path
                            .path
                            .segments
                            .last()
                            .is_some_and(|segment| segment.ident == "bool") =>
                    {
                        Some((
                            quote! {
                                // Derive the getter
                                pub fn #field_name(&self) -> #field_type {
                                    self.#field_name
                                }
                            },
                            (
                                quote! {
                                    stringify!(#field_name) => Some(self.#field_name),
                                },
                                quote! {
                                    stringify!(#field_name)
                                },
                            ),
                        ))
                    }
                    _ => None,
                }
            }),
            _ => panic!("Only named fields are supported."),
        },
        _ => panic!("Only structs supported."),
    };

    let (by_fn_getters, (string_name_getters, field_names)): (Vec<_>, (Vec<_>, Vec<_>)) =
        getters.unzip();

    let output = quote! {
        // For each getter, expand it out into a function in the impl block
        impl #struct_name {
            #(#by_fn_getters)*

            /// Lookup a feature flag by its string representation
            pub fn lookup_attr(&self, value: String) -> Option<bool> {
                match value.as_str() {
                    #(#string_name_getters)*
                    _ => None,
                }
            }

            /// Get a map of all feature flags from string representations
            pub fn attr_map(&self) -> std::collections::BTreeMap<String, bool> {
                vec![
                    // Okay to unwrap since we added all above
                    #(((#field_names).to_owned(), self.lookup_attr((#field_names).to_owned()).unwrap()),)*
                    ].into_iter().collect()
            }
        }
    };

    TokenStream::from(output)
}
