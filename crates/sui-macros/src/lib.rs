// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;
use quote::quote;

/// The sui_test macro will invoke either #[madsim::test] or #[tokio::test],
/// depending on whether the simulator config var is enabled.
///
/// This should be used for tests that can meaningfully run in either environment.
#[proc_macro_attribute]
pub fn sui_test(args: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);

    let header = if cfg!(madsim) {
        quote! {
            #[::sui_simulator::sim_test(crate = "sui_simulator", #(#args)*)]
        }
    } else {
        quote! {
            #[::tokio::test(#(#args)*)]
        }
    };

    let result = quote! {
        #header
        #input
    };

    result.into()
}

/// The sim_test macro will invoke #[madsim::test] if the simulator config var is enabled.
///
/// Otherwise, it will emit an ignored test - if forcibly run, the ignored test will panic.
///
/// This macro must be used in order to pass any simulator-specific arguments, such as
/// `check_determinism`, which is not understood by tokio.
#[proc_macro_attribute]
pub fn sim_test(args: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);

    let result = if cfg!(madsim) {
        quote! {
            #[::sui_simulator::sim_test(crate = "sui_simulator", #(#args)*)]
            #input
        }
    } else {
        let fn_name = input.sig.ident.clone();
        quote! {
            #[::core::prelude::v1::test]
            #[ignore = "simulator-only test"]
            fn #fn_name () {
                unimplemented!("this test cannot run outside the simulator");

                // paste original function to silence un-used import errors.
                #[allow(dead_code)]
                #input
            }
        }
    };

    result.into()
}
