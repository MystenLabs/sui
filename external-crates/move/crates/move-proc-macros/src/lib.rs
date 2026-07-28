// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Data, DataEnum, DeriveInput, Ident, ItemFn, LitStr, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

/// This macro generates a function `order_to_variant_map` which returns a map
/// of the position of each variant to the name of the variant.
/// It is intended to catch changes in enum order when backward compat is required.
/// A test is also generated which enforces the enum order.
///
/// ```rust,ignore
///    /// Example for this enum
///    #[test_variant_order(src/unit_tests/staged_enum_variant_order/my_enum.yaml)]
///    pub enum MyEnum {
///         A,
///         B(u64),
///         C{x: bool, y: i8},
///     }
///     let order_map = MyEnum::order_to_variant_map();
///     assert!(order_map.get(0).unwrap() == "A");
///     assert!(order_map.get(1).unwrap() == "B");
///     assert!(order_map.get(2).unwrap() == "C");
///
///     // A test called `enforce_enum_order_test_MyEnum` is generated which enforces the enum order.
///     // The snapshot file will be at `src/unit_tests/staged_enum_variant_order/my_enum.yaml`
/// ```
#[proc_macro_attribute]
pub fn test_variant_order(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Remove whitespace between the slashes
    let path = attr
        .to_string()
        .split('/')
        .map(|x| x.trim().to_string())
        .collect::<Vec<String>>()
        .join("/");

    let item_orig = item.clone();
    let ast_orig = parse_macro_input!(item_orig as DeriveInput);

    let ast = parse_macro_input!(item as DeriveInput);
    let name = &ast.ident;
    let test_fn_name = syn::Ident::new(&format!("enforce_enum_order_test_{}", name), name.span());
    let Data::Enum(DataEnum { variants, .. }) = ast.data else {
        panic!("`test_variant_order` macro can only be used with enums.");
    };

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
        #[cfg(test)]
        impl enum_compat_util::EnumOrderMap for #name {
            fn order_to_variant_map() -> std::collections::BTreeMap<u64, String> {
                let mut map = std::collections::BTreeMap::new();
                #(#variant_entries)*
                map
            }
        }

        #[allow(non_snake_case)]
        #[test]
        fn #test_fn_name() {
            let mut base_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            base_path.extend([
                #path,
            ]);
            enum_compat_util::check_enum_compat_order::<#name>(base_path);
        }

        #ast_orig
    };

    deriv.into()
}

const RED_ZONE: usize = 1024 * 1024; // 1MB
const STACK_PER_CALL: usize = 1024 * 1024 * 8; // 8MB

/// This macro uses `stacker` to grow the stack of any function annotated with it. It does this by
/// rewriting the function body to bump the stack pointer up by 1MB per call. The intent it to use
/// this in the compiler to avoid stack overflows in many places that Rust was previously
/// destroying the stack.
///
/// The `grow_stack` call takes two arguments, `RED_ZONE` and `STACK_SIZE`. It then checks to see
/// if we're within `RED_ZONE` bytes of the end of the stack, and will allocate a new stack of at
/// least `STACK_SIZE` bytes if so.
#[proc_macro_attribute]
pub fn growing_stack(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = input_fn;

    let output = quote! {
        #(#attrs)* #vis #sig {
            stacker::maybe_grow(#RED_ZONE, #STACK_PER_CALL, || #block)
        }
    };

    output.into()
}

/// A segment of the path given to `optional_include_str!`: either a string literal or an
/// identifier (so `macro_rules!` callers can splice in captured idents).
enum PathPart {
    Lit(LitStr),
    Ident(Ident),
}

impl Parse for PathPart {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(LitStr) {
            Ok(Self::Lit(input.parse()?))
        } else {
            Ok(Self::Ident(input.parse()?))
        }
    }
}

/// Like `include_str!`, but expands to `Some(include_str!(...))` if the file exists and `None`
/// if it does not. The path is the concatenation of the comma-separated arguments (string
/// literals and identifiers), resolved relative to the `CARGO_MANIFEST_DIR` of the crate
/// invoking the macro.
///
/// ```rust,ignore
/// // Some(...) if `<crate root>/docs/Foo_Bar.md` exists, None otherwise
/// let doc: Option<&'static str> = optional_include_str!("docs/", Foo, "_", Bar, ".md");
/// ```
///
/// Note: rustc only records a dependency on the file when it is actually included, so a crate
/// using this macro should also have a build script with `cargo::rerun-if-changed` on the
/// relevant directory to pick up newly added files.
#[proc_macro]
pub fn optional_include_str(input: TokenStream) -> TokenStream {
    let parts = parse_macro_input!(input with Punctuated::<PathPart, Token![,]>::parse_terminated);
    let relative_path: String = parts
        .iter()
        .map(|part| match part {
            PathPart::Lit(lit) => lit.value(),
            PathPart::Ident(ident) => ident.to_string(),
        })
        .collect();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set when expanding optional_include_str!");
    let full_path = std::path::Path::new(&manifest_dir).join(&relative_path);
    let output = if full_path.is_file() {
        let full_path = full_path
            .to_str()
            .expect("non-UTF-8 path in optional_include_str!");
        quote! { Some(include_str!(#full_path)) }
    } else {
        quote! { None }
    };
    output.into()
}

/// Procedural macro to parse an identifier and return it with the first letter capitalized.
#[proc_macro]
pub fn capitalize(input: TokenStream) -> TokenStream {
    // Parse the input as an identifier
    let ident = parse_macro_input!(input as Ident);

    // Convert the identifier to a string
    let mut ident_str = ident.to_string();

    // Capitalize the first letter
    ident_str[..1].make_ascii_uppercase();

    // Create a new identifier with the capitalized string
    let new_ident = Ident::new(&ident_str, ident.span());

    // Generate and return the output TokenStream
    let output = quote! {
        #new_ident
    };

    output.into()
}
