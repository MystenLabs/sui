// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Error, Expr, LitStr, Result, Token, Type,
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
/// Bound values following in the order matching their binders in the string:
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
        tokens.extend(quote! {
            .bind::<::diesel::sql_types::#ty, _>(#expr)
            .sql(#suffix)
        });
    }

    tokens.into()
}
