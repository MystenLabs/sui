// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DataEnum, DeriveInput};

/// This proc macro generates a function `order_to_variant_map` which returns a map
/// of the position of each variant to the name of the variant.
/// It is intended to catch changes in enum order when backward compat is required.
/// ```rust,ignore
///    /// Example for this enum
///    #[derive(EnumVariantOrder)]
///    pub enum MyEnum {
///         A,
///         B(u64),
///         C{x: bool, y: i8},
///     }
///     let order_map = MyEnum::order_to_variant_map();
///     assert!(order_map.get(0).unwrap() == "A");
///     assert!(order_map.get(1).unwrap() == "B");
///     assert!(order_map.get(2).unwrap() == "C");
/// ```
#[proc_macro_derive(EnumVariantOrder)]
pub fn enum_variant_order_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    if let Data::Enum(DataEnum { variants, .. }) = ast.data {
        let variant_entries = variants
            .iter()
            .enumerate()
            .map(|(index, variant)| {
                let variant_name = variant.ident.to_string();
                quote! {
                    map.insert( #index as u64, (#variant_name).to_string());
                }
            })
            .collect::<Vec<_>>();

        let deriv = quote! {
            impl enum_compat_util::EnumOrderMap for #name {
                fn order_to_variant_map() -> std::collections::BTreeMap<u64, String > {
                    let mut map = std::collections::BTreeMap::new();
                    #(#variant_entries)*
                    map
                }
            }
        };

        deriv.into()
    } else {
        panic!("EnumVariantOrder can only be used with enums.");
    }
}
