// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Type};

/// This proc macro generates getters for protocol config fields of type `Option<T>`.
/// Example for a field: `new_constant: Option<u64>`, we derive
/// ```rust,ignore
///      pub fn new_constant(&self) -> u64 {
///         self.new_constant.expect(Self::CONSTANT_ERR_MSG)
///     }
/// ```
#[proc_macro_derive(ProtocolConfigGetters)]
pub fn getters_macro(input: TokenStream) -> TokenStream {
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
                        Some(quote! {
                            // Derive the getter
                            pub fn #field_name(&self) -> #inner_type {
                                self.#field_name.expect(Self::CONSTANT_ERR_MSG)
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

    let output = quote! {
        // For each getter, expand it out into a function in the impl block
        impl #struct_name {
            const CONSTANT_ERR_MSG: &str = "protocol constant not present in current protocol version";
            #(#getters)*
        }
    };

    TokenStream::from(output)
}
