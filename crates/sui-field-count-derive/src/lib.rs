// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(FieldCount)]
pub fn field_count_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let generics = input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let fields_count = if let syn::Data::Struct(data_struct) = input.data {
        data_struct.fields.len()
    } else {
        panic!("FieldCount can only be derived for structs");
    };

    let expanded = quote! {
        impl #impl_generics FieldCount for #name #ty_generics #where_clause {
            const FIELD_COUNT: usize = #fields_count;
        }
    };

    TokenStream::from(expanded)
}
