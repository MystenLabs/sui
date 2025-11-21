// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Error, Expr, LitStr, Result, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

use crate::{
    lexer::Lexer,
    parser::{Format, Parser},
};

mod lexer;
mod parser;

/// Rust syntax for `sql!(as T, "format", binds,*)`
struct SqlInput {
    return_: Type,
    format_: LitStr,
    binds: Punctuated<Expr, Token![,]>,
}

/// Rust syntax for `query!("format", binds,*)`.
struct QueryInput {
    format_: LitStr,
    binds: Punctuated<Expr, Token![,]>,
}

impl Parse for SqlInput {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<Token![as]>()?;
        let return_ = input.parse()?;
        input.parse::<Token![,]>()?;
        let format_ = input.parse()?;

        if input.is_empty() {
            return Ok(Self {
                return_,
                format_,
                binds: Punctuated::new(),
            });
        }

        input.parse::<Token![,]>()?;
        let binds = Punctuated::parse_terminated(input)?;

        Ok(Self {
            return_,
            format_,
            binds,
        })
    }
}

impl Parse for QueryInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let format_ = input.parse()?;

        if input.is_empty() {
            return Ok(Self {
                format_,
                binds: Punctuated::new(),
            });
        }

        input.parse::<Token![,]>()?;
        let binds = Punctuated::parse_terminated(input)?;

        Ok(Self { format_, binds })
    }
}

/// The `sql!` macro is used to construct a `diesel::SqlLiteral<T>` using a format string to
/// describe the SQL snippet with the following syntax:
///
/// ```rust,ignore
/// sql!(as T, "format", binds,*)
/// ```
///
/// `T` is the `SqlType` that the literal will be interpreted as, as a Rust expression. The format
/// string introduces binders with curly braces, surrounding the `SqlType` of the bound value. This
/// type is given as a string which must correspond to a type in the `diesel::sql_types` module.
/// Bound values follow in the order matching their binders in the string:
///
/// ```rust,ignore
/// sql!(as Bool, "{BigInt} <= foo AND foo < {BigInt}", 5, 10)
/// ```
///
/// The above macro invocation will generate the following code:
///
/// ```rust,ignore
/// sql::<Bool>("")
///    .bind::<BigInt, _>(5)
///    .sql(" <= foo AND foo < ")
///    .bind::<BigInt, _>(10)
///    .sql("")
/// ```
#[proc_macro]
pub fn sql(input: TokenStream) -> TokenStream {
    let SqlInput {
        return_,
        format_,
        binds,
        ..
    } = parse_macro_input!(input as SqlInput);

    let format_str = format_.value();
    let lexemes: Vec<_> = Lexer::new(&format_str).collect();
    let Format { head, tail } = match Parser::new(&lexemes).format() {
        Ok(format) => format,
        Err(err) => {
            return Error::new(format_.span(), err).into_compile_error().into();
        }
    };

    let mut tokens = quote! {
        ::diesel::dsl::sql::<#return_>(#head)
    };

    for (expr, (ty, suffix)) in binds.iter().zip(tail.into_iter()) {
        tokens.extend(if let Some(ty) = ty {
            quote! {
                .bind::<::diesel::sql_types::#ty, _>(#expr)
                .sql(#suffix)
            }
        } else {
            // No type was provided for the bind parameter, so we use `Untyped` which will report
            // an error because it doesn't implement `SqlType`.
            quote! {
                .bind::<::diesel::sql_types::Untyped, _>(#expr)
                .sql(#suffix)
            }
        });
    }

    tokens.into()
}

/// The `query!` macro constructs a value that implements `diesel::query_builder::Query` -- a full
/// SQL query, defined by a format string and binds with the following syntax:
///
/// ```rust,ignore
/// query!("format", binds,*)
/// ```
///
/// The format string introduces binders with curly braces. An empty binder interpolates another
/// query at that position, otherwise the binder is expected to contain a `SqlType` for a value
/// that will be bound into the query, given a string which must correspond to a type in the
/// `diesel::sql_types` module. Bound values or queries to interpolate follow in the order matching
/// their binders in the string:
///
/// ```rust,ignore
/// query!("SELECT * FROM foo WHERE {BigInt} <= cursor AND {}", 5, query!("cursor < {BigInt}", 10))
/// ```
///
/// The above macro invocation will generate the following SQL query:
///
/// ```sql
/// SELECT * FROM foo WHERE $1 <= cursor AND cursor < $2 -- binds [5, 10]
/// ```
#[proc_macro]
pub fn query(input: TokenStream) -> TokenStream {
    let QueryInput { format_, binds } = parse_macro_input!(input as QueryInput);

    let format_str = format_.value();
    let lexemes: Vec<_> = Lexer::new(&format_str).collect();
    let Format { head, tail } = match Parser::new(&lexemes).format() {
        Ok(format) => format,
        Err(err) => {
            return Error::new(format_.span(), err).into_compile_error().into();
        }
    };

    let mut tokens = quote! {
        ::sui_pg_db::query::Query::new(#head)
    };

    for (expr, (ty, suffix)) in binds.iter().zip(tail.into_iter()) {
        tokens.extend(if let Some(ty) = ty {
            // If there is a type, this interpolation is for a bind.
            quote! {
                .bind::<::diesel::sql_types::#ty, _>(#expr)
                .sql(#suffix)
            }
        } else {
            // Otherwise, we are interpolating another query.
            quote! {
                .query(#expr)
                .sql(#suffix)
            }
        });
    }

    tokens.into()
}
