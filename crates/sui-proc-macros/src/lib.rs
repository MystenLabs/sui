// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;
use quote::{ToTokens, quote, quote_spanned};
use syn::{
    Attribute, BinOp, Data, DataEnum, DeriveInput, Expr, ExprBinary, ExprMacro, Item, ItemMacro,
    Stmt, StmtMacro, Token, UnOp,
    fold::{Fold, fold_expr, fold_item_macro, fold_stmt},
    parse::Parser,
    parse_macro_input, parse2,
    punctuated::Punctuated,
    spanned::Spanned,
};

/// The sui_test macro will invoke either `#[msim::test]` or `#[tokio::test]`,
/// depending on whether the simulator config var is enabled.
///
/// This should be used for tests that can meaningfully run in either environment.
#[proc_macro_attribute]
pub fn sui_test(args: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as syn::ItemFn);
    let arg_parser = Punctuated::<syn::Meta, Token![,]>::parse_terminated;
    let args = arg_parser.parse(args).unwrap().into_iter();

    let header = if cfg!(msim) {
        quote! {
            #[::sui_simulator::sim_test(crate = "sui_simulator", #(#args)* )]
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

/// The sim_test macro will invoke `#[msim::test]` if the simulator config var is enabled.
///
/// Otherwise, it will emit an ignored test - if forcibly run, the ignored test will panic.
///
/// This macro must be used in order to pass any simulator-specific arguments (e.g. a
/// custom `config`), which are not understood by tokio.
#[proc_macro_attribute]
pub fn sim_test(args: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as syn::ItemFn);
    let arg_parser = Punctuated::<syn::Meta, Token![,]>::parse_terminated;
    let args = arg_parser.parse(args).unwrap().into_iter();

    let ignore = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("ignore"))
        .map_or(quote! {}, |_| quote! { #[ignore] });

    let result = if cfg!(msim) {
        let sig = &input.sig;
        let return_type = &sig.output;
        let body = &input.block;
        quote! {
            #[::sui_simulator::sim_test(crate = "sui_simulator", #(#args),*)]
            #ignore
            #sig {
                async fn body_fn() #return_type { #body }

                let timeout_secs: u64 = std::env::var("SUI_SIM_TEST_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1000);
                let timeout_duration = tokio::time::Duration::from_secs(timeout_secs);

                let ret = tokio::time::timeout(timeout_duration, body_fn())
                    .await
                    .expect("sim_test timed out");

                ::sui_simulator::task::shutdown_all_nodes();

                // all node handles should have been dropped after the above block exits, but task
                // shutdown is asynchronous, so we need a brief delay before checking for leaks.
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

                assert_eq!(
                    sui_simulator::NodeLeakDetector::get_current_node_count(),
                    0,
                    "SuiNode leak detected"
                );

                ret
            }
        }
    } else {
        let fn_name = &input.sig.ident;
        let sig = &input.sig;
        let body = &input.block;
        quote! {
            #[allow(clippy::needless_return)]
            #[tokio::test]
            #ignore
            #sig {
                if std::env::var("SUI_SKIP_SIMTESTS").is_ok() {
                    println!("not running test {} in `cargo test`: SUI_SKIP_SIMTESTS is set", stringify!(#fn_name));

                    struct Ret;

                    impl From<Ret> for () {
                        fn from(_ret: Ret) -> Self {
                        }
                    }

                    impl<E> From<Ret> for Result<(), E> {
                        fn from(_ret: Ret) -> Self {
                            Ok(())
                        }
                    }

                    return Ret.into();
                }

                #body
            }
        }
    };

    result.into()
}

#[proc_macro]
pub fn checked_arithmetic(input: TokenStream) -> TokenStream {
    let input_file = CheckArithmetic.fold_file(parse_macro_input!(input));

    let output_items = input_file.items;

    let output = quote! {
        #(#output_items)*
    };

    TokenStream::from(output)
}

#[proc_macro_attribute]
pub fn with_checked_arithmetic(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_item = parse_macro_input!(item as Item);
    match input_item {
        Item::Fn(input_fn) => {
            let transformed_fn = CheckArithmetic.fold_item_fn(input_fn);
            TokenStream::from(quote! { #transformed_fn })
        }
        Item::Impl(input_impl) => {
            let transformed_impl = CheckArithmetic.fold_item_impl(input_impl);
            TokenStream::from(quote! { #transformed_impl })
        }
        item => {
            let transformed_impl = CheckArithmetic.fold_item(item);
            TokenStream::from(quote! { #transformed_impl })
        }
    }
}

struct CheckArithmetic;

impl CheckArithmetic {
    fn maybe_skip_macro(&self, attrs: &mut Vec<Attribute>) -> bool {
        if let Some(idx) = attrs
            .iter()
            .position(|attr| attr.path().is_ident("skip_checked_arithmetic"))
        {
            // Skip processing macro because it is annotated with
            // #[skip_checked_arithmetic]
            attrs.remove(idx);
            true
        } else {
            false
        }
    }

    fn process_macro_contents(
        &mut self,
        tokens: proc_macro2::TokenStream,
    ) -> syn::Result<proc_macro2::TokenStream> {
        // Parse the macro's contents as a comma-separated list of expressions.
        let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
        let Ok(exprs) = parser.parse(tokens.clone().into()) else {
            return Err(syn::Error::new_spanned(
                tokens,
                "could not process macro contents - use #[skip_checked_arithmetic] to skip this macro",
            ));
        };

        // Fold each sub expression.
        let folded_exprs = exprs
            .into_iter()
            .map(|expr| self.fold_expr(expr))
            .collect::<Vec<_>>();

        // Convert the folded expressions back into tokens and reconstruct the macro.
        let mut folded_tokens = proc_macro2::TokenStream::new();
        for (i, folded_expr) in folded_exprs.into_iter().enumerate() {
            if i > 0 {
                folded_tokens.extend(std::iter::once::<proc_macro2::TokenTree>(
                    proc_macro2::Punct::new(',', proc_macro2::Spacing::Alone).into(),
                ));
            }
            folded_expr.to_tokens(&mut folded_tokens);
        }

        Ok(folded_tokens)
    }
}

impl Fold for CheckArithmetic {
    fn fold_stmt(&mut self, stmt: Stmt) -> Stmt {
        let stmt = fold_stmt(self, stmt);
        if let Stmt::Macro(stmt_macro) = stmt {
            let StmtMacro {
                mut attrs,
                mut mac,
                semi_token,
            } = stmt_macro;

            if self.maybe_skip_macro(&mut attrs) {
                Stmt::Macro(StmtMacro {
                    attrs,
                    mac,
                    semi_token,
                })
            } else {
                match self.process_macro_contents(mac.tokens.clone()) {
                    Ok(folded_tokens) => {
                        mac.tokens = folded_tokens;
                        Stmt::Macro(StmtMacro {
                            attrs,
                            mac,
                            semi_token,
                        })
                    }
                    Err(error) => parse2(error.to_compile_error()).unwrap(),
                }
            }
        } else {
            stmt
        }
    }

    fn fold_item_macro(&mut self, mut item_macro: ItemMacro) -> ItemMacro {
        if !self.maybe_skip_macro(&mut item_macro.attrs) {
            let err = syn::Error::new_spanned(
                item_macro.to_token_stream(),
                "cannot process macros - use #[skip_checked_arithmetic] to skip \
                    processing this macro",
            );

            return parse2(err.to_compile_error()).unwrap();
        }
        fold_item_macro(self, item_macro)
    }

    fn fold_expr(&mut self, expr: Expr) -> Expr {
        let span = expr.span();
        let expr = fold_expr(self, expr);
        let expr = match expr {
            Expr::Macro(expr_macro) => {
                let ExprMacro { mut attrs, mut mac } = expr_macro;

                if self.maybe_skip_macro(&mut attrs) {
                    return Expr::Macro(ExprMacro { attrs, mac });
                } else {
                    match self.process_macro_contents(mac.tokens.clone()) {
                        Ok(folded_tokens) => {
                            mac.tokens = folded_tokens;
                            let expr_macro = Expr::Macro(ExprMacro { attrs, mac });
                            quote!(#expr_macro)
                        }
                        Err(error) => {
                            return Expr::Verbatim(error.to_compile_error());
                        }
                    }
                }
            }

            Expr::Binary(expr_binary) => {
                let ExprBinary {
                    attrs,
                    mut left,
                    op,
                    mut right,
                } = expr_binary;

                fn remove_parens(expr: &mut Expr) {
                    if let Expr::Paren(paren) = expr {
                        // i don't even think rust allows this, but just in case
                        assert!(paren.attrs.is_empty(), "TODO: attrs on parenthesized");
                        *expr = *paren.expr.clone();
                    }
                }

                macro_rules! wrap_op {
                    ($left: expr, $right: expr, $method: ident, $span: expr) => {{
                        // Remove parens from exprs since both sides get assigned to tmp variables.
                        // otherwise we get lint errors
                        remove_parens(&mut $left);
                        remove_parens(&mut $right);

                        quote_spanned!($span => {
                            // assign in one stmt in case either #left or #right contains
                            // references to `left` or `right` symbols.
                            let (left, right) = (#left, #right);
                            left.$method(right)
                                .unwrap_or_else(||
                                    panic!(
                                        "Overflow or underflow in {} {} + {}",
                                        stringify!($method),
                                        left,
                                        right,
                                    )
                                )
                        })
                    }};
                }

                macro_rules! wrap_op_assign {
                    ($left: expr, $right: expr, $method: ident, $span: expr) => {{
                        // Remove parens from exprs since both sides get assigned to tmp variables.
                        // otherwise we get lint errors
                        remove_parens(&mut $left);
                        remove_parens(&mut $right);

                        quote_spanned!($span => {
                            // assign in one stmt in case either #left or #right contains
                            // references to `left` or `right` symbols.
                            let (left, right) = (&mut #left, #right);
                            *left = (*left).$method(right)
                                .unwrap_or_else(||
                                    panic!(
                                        "Overflow or underflow in {} {} + {}",
                                        stringify!($method),
                                        *left,
                                        right
                                    )
                                )
                        })
                    }};
                }

                match op {
                    BinOp::Add(_) => {
                        wrap_op!(left, right, checked_add, span)
                    }
                    BinOp::Sub(_) => {
                        wrap_op!(left, right, checked_sub, span)
                    }
                    BinOp::Mul(_) => {
                        wrap_op!(left, right, checked_mul, span)
                    }
                    BinOp::Div(_) => {
                        wrap_op!(left, right, checked_div, span)
                    }
                    BinOp::Rem(_) => {
                        wrap_op!(left, right, checked_rem, span)
                    }
                    BinOp::AddAssign(_) => {
                        wrap_op_assign!(left, right, checked_add, span)
                    }
                    BinOp::SubAssign(_) => {
                        wrap_op_assign!(left, right, checked_sub, span)
                    }
                    BinOp::MulAssign(_) => {
                        wrap_op_assign!(left, right, checked_mul, span)
                    }
                    BinOp::DivAssign(_) => {
                        wrap_op_assign!(left, right, checked_div, span)
                    }
                    BinOp::RemAssign(_) => {
                        wrap_op_assign!(left, right, checked_rem, span)
                    }
                    _ => {
                        let expr_binary = ExprBinary {
                            attrs,
                            left,
                            op,
                            right,
                        };
                        quote_spanned!(span => #expr_binary)
                    }
                }
            }
            Expr::Unary(expr_unary) => {
                let op = &expr_unary.op;
                let operand = &expr_unary.expr;
                match op {
                    UnOp::Neg(_) => {
                        quote_spanned!(span => #operand.checked_neg().expect("Overflow or underflow in negation"))
                    }
                    _ => quote_spanned!(span => #expr_unary),
                }
            }
            _ => quote_spanned!(span => #expr),
        };

        parse2(expr).unwrap()
    }
}

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
            impl sui_enum_compat_util::EnumOrderMap for #name {
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
