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
#[proc_macro_derive(ProtocolConfigAccessors)]
pub fn accessors_macro(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    let struct_name = &ast.ident;
    let data = &ast.data;
    let mut inner_types = vec![];

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

                        let as_option_name = format!("{field_name}_as_option");
                        let as_option_name: proc_macro2::TokenStream =
                        as_option_name.parse().unwrap();
                        let test_setter_name: proc_macro2::TokenStream =
                            format!("set_{field_name}_for_testing").parse().unwrap();
                        let test_un_setter_name: proc_macro2::TokenStream =
                            format!("disable_{field_name}_for_testing").parse().unwrap();
                        let test_setter_from_str_name: proc_macro2::TokenStream =
                            format!("set_{field_name}_from_str_for_testing").parse().unwrap();

                        let getter = quote! {
                            // Derive the getter
                            pub fn #field_name(&self) -> #inner_type {
                                self.#field_name.expect(Self::CONSTANT_ERR_MSG)
                            }

                            pub fn #as_option_name(&self) -> #field_type {
                                self.#field_name
                            }
                        };

                        let test_setter = quote! {
                            // Derive the setter
                            pub fn #test_setter_name(&mut self, val: #inner_type) {
                                self.#field_name = Some(val);
                            }

                            // Derive the setter from String
                            pub fn #test_setter_from_str_name(&mut self, val: String) {
                                use std::str::FromStr;
                                self.#test_setter_name(#inner_type::from_str(&val).unwrap());
                            }

                            // Derive the un-setter
                            pub fn #test_un_setter_name(&mut self) {
                                self.#field_name = None;
                            }
                        };

                        let value_setter = quote! {
                            stringify!(#field_name) => self.#test_setter_from_str_name(val),
                        };


                        let value_lookup = quote! {
                            stringify!(#field_name) => self.#field_name.map(|v| ProtocolConfigValue::#inner_type(v)),
                        };

                        let field_name_str = quote! {
                            stringify!(#field_name)
                        };

                        // Track all the types seen
                        if inner_types.contains(&inner_type) {
                            None
                        } else {
                            inner_types.push(inner_type.clone());
                            Some(quote! {
                               #inner_type
                            })
                        };

                        Some(((getter, (test_setter, value_setter)), (value_lookup, field_name_str)))
                    }
                    _ => None,
                }
            }),
            _ => panic!("Only named fields are supported."),
        },
        _ => panic!("Only structs supported."),
    };

    #[allow(clippy::type_complexity)]
    let ((getters, (test_setters, value_setters)), (value_lookup, field_names_str)): (
        (Vec<_>, (Vec<_>, Vec<_>)),
        (Vec<_>, Vec<_>),
    ) = tokens.unzip();
    let output = quote! {
        // For each getter, expand it out into a function in the impl block
        impl #struct_name {
            const CONSTANT_ERR_MSG: &'static str = "protocol constant not present in current protocol version";
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

        // For each attr, derive a setter from the raw value and from string repr
        impl #struct_name {
            #(#test_setters)*

            pub fn set_attr_for_testing(&mut self, attr: String, val: String) {
                match attr.as_str() {
                    #(#value_setters)*
                    _ => panic!("Attempting to set unknown attribute: {}", attr),
                }
            }
        }

        #[allow(non_camel_case_types)]
        #[derive(Clone, Serialize, Debug, PartialEq, Deserialize, schemars::JsonSchema)]
        pub enum ProtocolConfigValue {
            #(#inner_types(#inner_types),)*
        }

        impl std::fmt::Display for ProtocolConfigValue {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                use std::fmt::Write;
                let mut writer = String::new();
                match self {
                    #(
                        ProtocolConfigValue::#inner_types(x) => {
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
