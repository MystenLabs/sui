// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Type};

/// This proc macro generates getters, attribute lookup, etc for protocol config fields of type `Option<T>`
/// and for the feature flags
/// Example for a field: `new_constant: Option<u64>`, and for feature flags `feature: bool`, we derive
/// ```rust,ignore
///     /// Returns the value of the field if exists at the given version, otherise panic
///     pub fn new_constant(&self) -> u64 {
///         self.new_constant.expect(Self::CONSTANT_ERR_MSG)
///     }
///
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
#[proc_macro_derive(ProtocolConfigGetters)]
pub fn getters_macro(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    let struct_name = &ast.ident;
    let data = &ast.data;
    let mut seen_types = std::collections::HashSet::new();

    let tokens = match data {
        Data::Struct(data_struct) => match &data_struct.fields {
            // Operate on each field of the ProtocolConfig struct
            Fields::Named(fields_named) => fields_named.named.iter().filter_map(|field| {
                // Extract field name and type
                let field_name = field.ident.as_ref().expect("Field must be named");
                let field_type = &field.ty;
                // Check if field is of type Option<T>
                match field_type {
                    Type::Path(type_path)
                        if type_path
                            .path
                            .segments
                            .last()
                            .map_or(false, |segment| segment.ident == "Option") =>
                    {
                        // Extract inner type T from Option<T>
                        let inner_type = if let syn::PathArguments::AngleBracketed(
                            angle_bracketed_generic_arguments,
                        ) = &type_path.path.segments.last().unwrap().arguments
                        {
                            if let Some(syn::GenericArgument::Type(ty)) =
                                angle_bracketed_generic_arguments.args.first()
                            {
                                ty.clone()
                            } else {
                                panic!("Expected a type argument.");
                            }
                        } else {
                            panic!("Expected angle bracketed arguments.");
                        };

                        let getter = quote! {
                            // Derive the getter
                            pub fn #field_name(&self) -> #inner_type {
                                self.#field_name.expect(Self::CONSTANT_ERR_MSG)
                            }
                        };

                        let value_lookup = quote! {
                            stringify!(#field_name) => self.#field_name.map(|v| ProtocolConfigValue::#inner_type(v)),
                        };

                        let field_name_str = quote! {
                            stringify!(#field_name)
                        };

                        // Track all the types seen
                        if seen_types.contains(&inner_type) {
                            None
                        } else {
                            seen_types.insert(inner_type.clone());
                            Some(quote! {
                               #inner_type
                            })
                        };

                        Some((getter, (value_lookup, field_name_str)))
                    }
                    _ => None,
                }
            }),
            _ => panic!("Only named fields are supported."),
        },
        _ => panic!("Only structs supported."),
    };
    let (getters, (value_lookup, field_names_str)): (Vec<_>, (Vec<_>, Vec<_>)) = tokens.unzip();
    let inner_types1 = Vec::from_iter(seen_types);
    let inner_types2: Vec<_> = inner_types1.clone();
    let output = quote! {
        // For each getter, expand it out into a function in the impl block
        impl #struct_name {
            const CONSTANT_ERR_MSG: &str = "protocol constant not present in current protocol version";
            #(#getters)*

            /// Lookup a config attribute by its string representation
            pub fn lookup_attr(&self, value: String) -> Option<ProtocolConfigValue> {
                match value.as_str() {
                    #(#value_lookup)*
                    _ => None,
                }
            }

            /// Get a map of all config attribute from string representations
            pub fn attr_map(&self) -> std::collections::BTreeMap<String, Option<ProtocolConfigValue>> {
                vec![
                    #(((#field_names_str).to_owned(), self.lookup_attr((#field_names_str).to_owned())),)*
                    ].into_iter().collect()
            }

            /// Get the feature flags
            pub fn lookup_feature(&self, value: String) -> Option<bool> {
                self.feature_flags.lookup_attr(value)
            }

            pub fn feature_map(&self) -> std::collections::BTreeMap<String, bool> {
                self.feature_flags.attr_map()
            }
        }

        #[allow(non_camel_case_types)]
        #[derive(Clone, Serialize, Debug, PartialEq, Deserialize, schemars::JsonSchema)]
        pub enum ProtocolConfigValue {
            #(#inner_types1(#inner_types1),)*
        }

        impl std::fmt::Display for ProtocolConfigValue {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                use std::fmt::Write;
                let mut writer = String::new();
                match self {
                    #(
                        ProtocolConfigValue::#inner_types2(x) => {
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

#[proc_macro_derive(ProtocolConfigFeatureFlagsGetters)]
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
                // Check if field is of type bool
                match field_type {
                    Type::Path(type_path)
                        if type_path
                            .path
                            .segments
                            .last()
                            .map_or(false, |segment| segment.ident == "bool") =>
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
