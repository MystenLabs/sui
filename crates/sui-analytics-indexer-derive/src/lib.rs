// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, PathSegment};

#[proc_macro_derive(SerializeParquet)]
pub fn schema_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let (schema, getter_implementation) = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => {
                let mut idx = 0;
                let (schema_iter, getter_iter): (Vec<_>, Vec<_>) = fields
                    .named
                    .iter()
                    .filter_map(|field| {
                        let field_name = field.ident.as_ref().unwrap().to_string();
                        if is_supported_field_type(&field.ty) {
                            idx += 1;
                            Some((
                                format!("\"{}\".to_string()", field_name),
                                format!(
                                    "if idx == {} {{ return self.{}.clone().into(); }}",
                                    idx, field_name
                                ),
                            ))
                        } else {
                            None
                        }
                    })
                    .unzip();
                (schema_iter.join(", "), getter_iter.join("\n"))
            }
            _ => panic!("not supported struct for parquet serialization"),
        },
        _ => panic!("not supported struct for parquet serialization"),
    };
    let schema_tokens: proc_macro2::TokenStream = schema.parse().unwrap();
    let getter_implementation_tokens: proc_macro2::TokenStream =
        getter_implementation.parse().unwrap();
    quote! {
        impl ParquetSchema for #struct_name {
            fn schema() -> Vec<String> {
                vec![#schema_tokens]
            }

            fn get_column(&self, idx: usize) -> ParquetValue {
                #getter_implementation_tokens
                panic!("not supported column {:?}", idx);
            }
        }
    }
    .into()
}

fn is_supported_field_type(type_: &syn::Type) -> bool {
    // TODO: cover all used types and panic if not found
    let supported_types = ["u64", "String"];
    if let syn::Type::Path(path) = type_ {
        if path.path.segments.len() == 1 {
            let PathSegment { ident, .. } = &path.path.segments[0];
            return supported_types.contains(&&ident.to_string()[..]);
        }
    }
    false
}
