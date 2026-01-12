// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;

use quote::quote;
use syn::Data;
use syn::DeriveInput;
use syn::Fields;
use syn::parse_macro_input;

#[proc_macro_derive(SerializeRow)]
pub fn schema_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let (schema, getter_implementation, field_count) = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => {
                let field_count = fields.named.len();
                let (schema_iter, getter_iter): (Vec<_>, Vec<_>) = fields
                    .named
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| {
                        let field_name = field.ident.as_ref().unwrap().to_string();
                        (
                            format!("\"{}\"", field_name),
                            format!(
                                "if idx == {} {{ return Ok((&self.{}).into()); }}",
                                idx, field_name
                            ),
                        )
                    })
                    .unzip();
                (schema_iter.join(", "), getter_iter.join("\n"), field_count)
            }
            _ => panic!("not supported struct for row serialization"),
        },
        _ => panic!("not supported struct for row serialization"),
    };
    let schema_tokens: proc_macro2::TokenStream = schema.parse().unwrap();
    let getter_implementation_tokens: proc_macro2::TokenStream =
        getter_implementation.parse().unwrap();

    quote! {
        impl crate::schema::RowSchema for #struct_name {
            fn schema() -> &'static [&'static str] {
                &[#schema_tokens]
            }

            fn column_count(&self) -> usize {
                #field_count
            }

            fn get_column(&self, idx: usize) -> Result<crate::schema::ColumnValue<'_>, crate::schema::ColumnError> {
                #getter_implementation_tokens
                Err(crate::schema::ColumnError::InvalidIndex(idx))
            }
        }
    }
    .into()
}
