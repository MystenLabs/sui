// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Attribute, Data, DataStruct, DeriveInput, Fields, FieldsNamed, Meta,
    MetaList, MetaNameValue, NestedMeta,
};

/// Attribute macro to be applied to config-based structs. It ensures that the struct derives serde
/// traits, and `Debug`, that all fields are renamed with "kebab case", and adds a `#[serde(default
/// = ...)]` implementation for each field that ensures that if the field is not present during
/// deserialization, it is replaced with its default value, from the `Default` implementation for
/// the config struct.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn DefaultConfig(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        vis,
        ident,
        generics,
        data,
    } = parse_macro_input!(input as DeriveInput);

    let Data::Struct(DataStruct {
        struct_token,
        fields,
        semi_token,
    }) = data
    else {
        panic!("Default configs must be structs.");
    };

    let Fields::Named(FieldsNamed {
        brace_token: _,
        named,
    }) = fields
    else {
        panic!("Default configs must have named fields.");
    };

    // Extract field names once to avoid having to check for their existence multiple times.
    let fields_with_names: Vec<_> = named
        .iter()
        .map(|field| {
            let Some(ident) = &field.ident else {
                panic!("All fields must have an identifier.");
            };

            (ident, field)
        })
        .collect();

    // Generate the fields with the `#[serde(default = ...)]` attribute.
    let fields = fields_with_names.iter().map(|(name, field)| {
        let default = format!("{ident}::__default_{name}");
        quote! { #[serde(default = #default)] #field }
    });

    // Generate the default implementations for each field.
    let defaults = fields_with_names.iter().map(|(name, field)| {
        let ty = &field.ty;
        let fn_name = format_ident!("__default_{}", name);
        let cfg = extract_cfg(&field.attrs);

        quote! {
            #[doc(hidden)] #cfg
            fn #fn_name() -> #ty {
                <Self as std::default::Default>::default().#name
            }
        }
    });

    // Check if there's already a serde rename_all attribute
    let has_rename_all = attrs.iter().any(|attr| {
        if !attr.path.is_ident("serde") {
            return false;
        };

        let Ok(Meta::List(MetaList { nested, .. })) = attr.parse_meta() else {
            return false;
        };

        nested.iter().any(|nested| {
            if let NestedMeta::Meta(Meta::NameValue(MetaNameValue { path, .. })) = nested {
                path.is_ident("rename_all")
            } else {
                false
            }
        })
    });

    // Only include the default rename_all if none exists
    let rename_all = if !has_rename_all {
        quote! { #[serde(rename_all = "kebab-case")] }
    } else {
        quote! {}
    };

    TokenStream::from(quote! {
        #[derive(serde::Serialize, serde::Deserialize)]
        #rename_all
        #(#attrs)* #vis #struct_token #ident #generics {
            #(#fields),*
        } #semi_token

        impl #ident {
            #(#defaults)*
        }
    })
}

/// Find the attribute that corresponds to a `#[cfg(...)]` annotation, if it exists.
fn extract_cfg(attrs: &[Attribute]) -> Option<&Attribute> {
    attrs.iter().find(|attr| {
        let meta = attr.parse_meta().ok();
        meta.is_some_and(|m| m.path().is_ident("cfg"))
    })
}
