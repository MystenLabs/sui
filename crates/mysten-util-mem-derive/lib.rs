// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright 2020 Parity Technologies
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A crate for deriving the MallocSizeOf trait.
//!
//! This is a copy of Servo malloc_size_of_derive code, modified to work with
//! our `mysten_util_mem` library

#![allow(clippy::all)]

extern crate proc_macro2;
#[macro_use]
extern crate syn;
#[macro_use]
extern crate synstructure;

decl_derive!([MallocSizeOf, attributes(ignore_malloc_size_of)] => malloc_size_of_derive);

fn malloc_size_of_derive(s: synstructure::Structure) -> proc_macro2::TokenStream {
    let match_body = s.each(|binding| {
        let ignore = binding
            .ast()
            .attrs
            .iter()
            .any(|attr| match attr.parse_meta().unwrap() {
                syn::Meta::Path(ref path) | syn::Meta::List(syn::MetaList { ref path, .. })
                    if path.is_ident("ignore_malloc_size_of") =>
                {
                    panic!(
                        "#[ignore_malloc_size_of] should have an explanation, \
					 e.g. #[ignore_malloc_size_of = \"because reasons\"]"
                    );
                }
                syn::Meta::NameValue(syn::MetaNameValue { ref path, .. })
                    if path.is_ident("ignore_malloc_size_of") =>
                {
                    true
                }
                _ => false,
            });
        if ignore {
            None
        } else if let syn::Type::Array(..) = binding.ast().ty {
            Some(quote! {
                for item in #binding.iter() {
                    sum += mysten_util_mem::MallocSizeOf::size_of(item, ops);
                }
            })
        } else {
            Some(quote! {
                sum += mysten_util_mem::MallocSizeOf::size_of(#binding, ops);
            })
        }
    });

    let ast = s.ast();
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    let mut where_clause = where_clause.unwrap_or(&parse_quote!(where)).clone();
    for param in ast.generics.type_params() {
        let ident = &param.ident;
        where_clause
            .predicates
            .push(parse_quote!(#ident: mysten_util_mem::MallocSizeOf));
    }

    let tokens = quote! {
        impl #impl_generics mysten_util_mem::MallocSizeOf for #name #ty_generics #where_clause {
            #[inline]
            #[allow(unused_variables, unused_mut, unreachable_code)]
            fn size_of(&self, ops: &mut mysten_util_mem::MallocSizeOfOps) -> usize {
                let mut sum = 0;
                match *self {
                    #match_body
                }
                sum
            }
        }
    };

    tokens
}
