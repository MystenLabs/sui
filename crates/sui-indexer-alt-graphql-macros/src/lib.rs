// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;
use quote::quote;
use syn::Expr;
use syn::ExprCall;
use syn::FnArg;
use syn::ImplItem;
use syn::ImplItemFn;
use syn::ItemImpl;
use syn::Pat;
use syn::PatType;
use syn::Stmt;
use syn::Type;
use syn::parse_macro_input;
use syn::visit::Visit;

/// Replaces `#[Object]` on a GraphQL resolver `impl` block. Does everything `#[Object]` does
/// (this macro re-emits the transformed block with `#[Object]` reattached), and additionally
/// makes every field method responsible for its own pipeline-availability gating: for each
/// method that takes a `ctx: &Context<'_>` parameter and doesn't already call
/// `available_range::gate`/`gate_filtered` itself, this injects an unconditional check --
/// `type_`/`field` are derived from the impl's `Self` type and the method's name, matching the
/// keys `collect_pipelines!` (available_range.rs) already expects.
///
/// This only covers fields whose pipeline requirement doesn't depend on a filter synthesized at
/// runtime from `self`/scope (see available_range.rs's module docs for why that class of field
/// needs hand-written gating instead). Fields that already gate themselves explicitly are left
/// untouched (see the escape hatch below).
#[proc_macro_attribute]
#[allow(non_snake_case)]
pub fn GatedObject(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut impl_block = parse_macro_input!(item as ItemImpl);

    let type_name = self_type_name(&impl_block.self_ty);

    for item in &mut impl_block.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };

        let Some(ctx_ident) = ctx_param_ident(method) else {
            continue;
        };

        if calls_gate_directly(method) {
            continue;
        }

        let field_name = graphql_field_name(method);
        let check: Stmt = syn::parse_quote! {
            if let ::core::result::Result::Err(__gate_err) =
                crate::api::types::available_range::gate(#ctx_ident, #type_name, #field_name)
            {
                return crate::api::types::available_range::Gateable::gate_err(__gate_err);
            }
        };
        method.block.stmts.insert(0, check);
    }

    quote! {
        #[Object]
        #impl_block
    }
    .into()
}

/// The impl's `Self` type, stringified (e.g. `Object`, `MoveObject`) -- matches the `type_` key
/// `collect_pipelines!` expects.
fn self_type_name(self_ty: &Type) -> String {
    let Type::Path(type_path) = self_ty else {
        panic!("#[GatedObject] expects a plain `impl Type {{ .. }}` block");
    };
    let Some(segment) = type_path.path.segments.last() else {
        panic!("#[GatedObject] expects a plain `impl Type {{ .. }}` block");
    };
    segment.ident.to_string()
}

/// The identifier bound to this method's `ctx: &Context<'_>` parameter, if it has one.
fn ctx_param_ident(method: &ImplItemFn) -> Option<syn::Ident> {
    method.sig.inputs.iter().find_map(|arg| {
        let FnArg::Typed(PatType { pat, ty, .. }) = arg else {
            return None;
        };
        let Pat::Ident(pat_ident) = pat.as_ref() else {
            return None;
        };
        let Type::Reference(reference) = ty.as_ref() else {
            return None;
        };
        let Type::Path(type_path) = reference.elem.as_ref() else {
            return None;
        };
        let last = type_path.path.segments.last()?;
        (last.ident == "Context").then(|| pat_ident.ident.clone())
    })
}

/// This method's GraphQL field name: honors an explicit `#[graphql(name = "...")]` override,
/// otherwise converts the Rust (snake_case) method name to camelCase, matching async-graphql's
/// own field-naming convention.
fn graphql_field_name(method: &ImplItemFn) -> String {
    for attr in &method.attrs {
        if !attr.path().is_ident("graphql") {
            continue;
        }
        let mut name = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                name = Some(lit.value());
            }
            Ok(())
        });
        if let Some(name) = name {
            return name;
        }
    }

    snake_to_camel(&method.sig.ident.to_string())
}

fn snake_to_camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut capitalize_next = false;
    for c in name.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            out.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether `method`'s body already contains a direct call to `gate`/`gate_filtered` (however
/// qualified) -- the escape hatch letting a method opt out of automatic injection because it
/// gates itself explicitly (e.g. filter-conditional fields like `Query::object`).
fn calls_gate_directly(method: &ImplItemFn) -> bool {
    struct FindGateCall(bool);

    impl<'ast> Visit<'ast> for FindGateCall {
        fn visit_expr_call(&mut self, call: &'ast ExprCall) {
            if let Expr::Path(path) = call.func.as_ref()
                && let Some(last) = path.path.segments.last()
                && (last.ident == "gate" || last.ident == "gate_filtered")
            {
                self.0 = true;
            }
            syn::visit::visit_expr_call(self, call);
        }
    }

    let mut finder = FindGateCall(false);
    finder.visit_block(&method.block);
    finder.0
}
