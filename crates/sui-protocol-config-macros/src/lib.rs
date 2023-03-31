// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, Type};

/// This macro derives getters for each field in ProtocolConfig at a specific version
/// If the field is empty, None is returned, otherwise Some(value) is returned
#[proc_macro_derive(ProtocolConfigGetters)]
pub fn getters_macro(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    let struct_name = &ast.ident;
    let data = &ast.data;

    let getters = match data {
        // For each field, get the name and type
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields_named) => fields_named.named.iter().filter_map(|field| {
                let field_name = field.ident.as_ref().expect("Field must be named");
                let field_type = &field.ty;
                match field_type {
                    Type::Path(type_path)
                        // Only derive for fields of type Option<T>
                        if type_path
                            .path
                            .segments
                            .last()
                            .map_or(false, |segment| segment.ident == "Option") =>
                    {
                        // Prepend the function name with "get_for_current_version_"
                        let getter_name = format_ident!("get_for_version_{}", field_name);
                        let getter_name_curr =
                            format_ident!("get_for_current_version_{}", field_name);
                        Some(quote! {
                            pub fn #getter_name(version: ProtocolVersion) -> #field_type {
                                ProtocolConfig::get_for_version_impl(version).#field_name
                            }
                            pub fn #getter_name_curr(&self) -> #field_type {
                                Self::#getter_name(self.version)
                            }
                        })
                    }
                    _ => None,
                }
            }),
            _ => panic!("Only named fields are supported."),
        },
        _ => panic!("Only structs supported."),
    };

    // Generate for each
    let output = quote! {
        impl #struct_name {
            #(#getters)*
        }
    };

    TokenStream::from(output)
}
