// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Attribute, Data, DataStruct, DeriveInput, Fields, FieldsNamed, Ident, Meta,
    NestedMeta,
};

/// Attribute macro to be applied to config-based structs. It ensures that the struct derives serde
/// traits, and `Debug`, that all fields are renamed with "kebab case", and adds a `#[serde(default
/// = ...)]` implementation for each field that ensures that if the field is not present during
/// deserialization, it is replaced with its default value, from the `Default` implementation for
/// the config struct.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn GraphQLConfig(_attr: TokenStream, input: TokenStream) -> TokenStream {
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
        panic!("GraphQL configs must be structs.");
    };

    let Fields::Named(FieldsNamed {
        brace_token: _,
        named,
    }) = fields
    else {
        panic!("GraphQL configs must have named fields.");
    };

    // Figure out which derives need to be added to meet the criteria of a config struct.
    let core_derives = core_derives(&attrs);

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
                Self::default().#name
            }
        }
    });

    TokenStream::from(quote! {
        #[derive(#(#core_derives),*)]
        #[serde(rename_all = "kebab-case")]
        #(#attrs)* #vis #struct_token #ident #generics {
            #(#fields),*
        } #semi_token

        impl #ident {
            #(#defaults)*
        }
    })
}

/// Return a set of derives that should be added to the struct to make sure it derives all the
/// things we expect from a config, namely `Serialize`, `Deserialize`, and `Debug`.
///
/// We cannot add core derives unconditionally, because they will conflict with existing ones.
fn core_derives(attrs: &[Attribute]) -> BTreeSet<Ident> {
    let mut derives = BTreeSet::from_iter([
        format_ident!("Serialize"),
        format_ident!("Deserialize"),
        format_ident!("Debug"),
        format_ident!("Clone"),
        format_ident!("Eq"),
        format_ident!("PartialEq"),
    ]);

    for attr in attrs {
        let Ok(Meta::List(list)) = attr.parse_meta() else {
            continue;
        };

        let Some(ident) = list.path.get_ident() else {
            continue;
        };

        if ident != "derive" {
            continue;
        }

        for nested in list.nested {
            let NestedMeta::Meta(Meta::Path(path)) = nested else {
                continue;
            };

            let Some(ident) = path.get_ident() else {
                continue;
            };

            derives.remove(ident);
        }
    }

    derives
}

/// Find the attribute that corresponds to a `#[cfg(...)]` annotation, if it exists.
fn extract_cfg(attrs: &[Attribute]) -> Option<&Attribute> {
    attrs.iter().find(|attr| {
        let meta = attr.parse_meta().ok();
        meta.is_some_and(|m| m.path().is_ident("cfg"))
    })
}
