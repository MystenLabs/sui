// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;

use quote::quote;
use syn::FnArg;
use syn::ImplItem;
use syn::ItemImpl;
use syn::Pat;
use syn::PatType;
use syn::Signature;
use syn::Type;
use syn::parse_macro_input;

/// Wraps `impl Type { .. }` the same way `#[Object]` does, additionally injecting a pipeline-
/// availability guard as the first statement of every method that takes `ctx: &Context<'_>` as
/// the parameter immediately after the receiver. Methods without it are left unchanged.
///
/// The guard calls back into `crate::api::types::gated_object::check_pipeline_available` and
/// `crate::api::types::gated_object::GatedResolverResult`, so this macro is only usable from
/// within the `sui-indexer-alt-graphql` crate.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn GatedObject(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as ItemImpl);
    let ty_name = type_name(&input);

    for item in &mut input.items {
        let ImplItem::Method(method) = item else {
            continue;
        };

        let Some(ctx_ident) = ctx_param_ident(&method.sig) else {
            continue;
        };

        let field_name = graphql_field_name(&method.sig.ident.to_string());
        let guard: syn::Stmt = syn::parse2(quote! {
            if let ::std::result::Result::Err(__e) = crate::api::types::gated_object::check_pipeline_available(
                #ctx_ident,
                #ty_name,
                #field_name,
            ) {
                return crate::api::types::gated_object::GatedResolverResult::from_pipeline_error(__e);
            }
        })
        .expect("gated_object guard should parse as a valid statement");

        method.block.stmts.insert(0, guard);
    }

    quote! {
        #[async_graphql::Object]
        #input
    }
    .into()
}

/// Returns the identifier of the `ctx: &Context<'_>` parameter, if `sig` has one as the second
/// parameter (immediately after the receiver).
fn ctx_param_ident(sig: &Signature) -> Option<syn::Ident> {
    let mut inputs = sig.inputs.iter();

    let Some(FnArg::Receiver(_)) = inputs.next() else {
        return None;
    };

    let Some(FnArg::Typed(PatType { pat, ty, .. })) = inputs.next() else {
        return None;
    };

    if !is_context_ref(ty) {
        return None;
    }

    let Pat::Ident(pat_ident) = &**pat else {
        return None;
    };

    Some(pat_ident.ident.clone())
}

/// Whether `ty` is (syntactically) a `&Context<'_>` reference.
fn is_context_ref(ty: &Type) -> bool {
    let Type::Reference(reference) = ty else {
        return false;
    };
    let Type::Path(path) = &*reference.elem else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Context")
}

fn type_name(input: &ItemImpl) -> String {
    let Type::Path(path) = &*input.self_ty else {
        panic!("#[GatedObject] only supports `impl Type {{ .. }}` for a plain named type");
    };
    path.path
        .segments
        .last()
        .expect("empty type path")
        .ident
        .to_string()
}

/// Converts a `snake_case` Rust method name into the `camelCase` field name async-graphql
/// registers by default. Does not account for an explicit `#[graphql(name = ...)]` override.
fn graphql_field_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for (i, word) in name.split('_').enumerate() {
        if i == 0 {
            out.push_str(word);
        } else if let Some(first) = word.chars().next() {
            out.extend(first.to_uppercase());
            out.push_str(&word[first.len_utf8()..]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphql_field_name() {
        assert_eq!(graphql_field_name("id"), "id");
        assert_eq!(graphql_field_name("epoch_id"), "epochId");
        assert_eq!(graphql_field_name("coin_deny_list"), "coinDenyList");
        assert_eq!(
            graphql_field_name("total_stake_rewards"),
            "totalStakeRewards"
        );
    }
}
