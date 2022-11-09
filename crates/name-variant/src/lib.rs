// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//! Generates methods to print the name of the enum variant.
//!
//! # Example
//!
//! ```rust
//! use name_variant::NamedVariant;
//!
//! # macro_rules! dont_test { () => {
//! #[derive(NamedVariant)]
//! enum TestEnum {
//!     A,
//!     B(),
//!     C(i32, i32),
//!     D { _name: String, _age: i32 },
//!     VariantTest,
//! }
//!
//! let x = TestEnum::C(1, 2);
//! assert_eq!(x.variant_name(), "C");
//!
//! let x = TestEnum::A;
//! assert_eq!(x.variant_name(), "A");
//!
//! let x = TestEnum::B();
//! assert_eq!(x.variant_name(), "B");
//!
//! let x = TestEnum::D {_name: "Jane Doe".into(), _age: 30 };
//! assert_eq!(x.variant_name(), "D");
//!
//! let x = TestEnum::VariantTest;
//! assert_eq!(x.variant_name(), "VariantTest");
//!
//! # }};
//! ```

extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};

use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{parse_macro_input, Data, DataEnum, DeriveInput, Error, Fields};

macro_rules! derive_error {
    ($string: tt) => {
        Error::new(Span::call_site(), $string)
            .to_compile_error()
            .into()
    };
}

fn match_enum_to_string(name: &Ident, variants: &DataEnum) -> proc_macro2::TokenStream {
    // the variant dispatch proper
    let mut match_arms = quote! {};
    for variant in variants.variants.iter() {
        let variant_ident = &variant.ident;
        let fields_in_variant = match &variant.fields {
            Fields::Unnamed(_) => quote_spanned! {variant.span() => (..) },
            Fields::Unit => quote_spanned! { variant.span() => },
            Fields::Named(_) => quote_spanned! {variant.span() => {..} },
        };
        let variant_string = variant_ident.to_string();

        match_arms.extend(quote! {
            #name::#variant_ident #fields_in_variant => #variant_string,
        });
    }
    match_arms
}

#[proc_macro_derive(NamedVariant)]
pub fn derive_named_variant(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let data = &input.data;

    let mut variant_checker_functions;

    match data {
        Data::Enum(data_enum) => {
            variant_checker_functions = TokenStream2::new();

            let variant_arms = match_enum_to_string(name, data_enum);

            variant_checker_functions.extend(quote_spanned! { name.span() =>
                const fn variant_name(&self) -> &'static str {
                    match self {
                        #variant_arms
                    }
                }
            });
        }
        _ => return derive_error!("NamedVariant is only implemented for enums"),
    };

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            #variant_checker_functions
        }
    };

    TokenStream::from(expanded)
}
