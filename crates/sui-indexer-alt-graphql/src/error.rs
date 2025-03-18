// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::Infallible;

use async_graphql::ErrorExtensions;

/// Error codes for the `extensions.code` field of a GraphQL error that originates from outside
/// GraphQL.
///
/// <https://www.apollographql.com/docs/apollo-server/data/errors/#built-in-error-codes>
pub(crate) mod code {
    pub const BAD_USER_INPUT: &str = "BAD_USER_INPUT";
    pub const INTERNAL_SERVER_ERROR: &str = "INTERNAL_SERVER_ERROR";
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum RpcError<E: std::error::Error = Infallible> {
    BadUserInput(E),
    InternalError(#[from] anyhow::Error),
}

impl<E: std::error::Error> From<RpcError<E>> for async_graphql::Error {
    fn from(err: RpcError<E>) -> Self {
        match err {
            RpcError::BadUserInput(err) => err.to_string().extend_with(|_, ext| {
                ext.set("code", code::BAD_USER_INPUT);
            }),

            RpcError::InternalError(err) => {
                // Discard the root cause (which will be the main error message), and then capture
                // the rest as a context chain.
                let mut chain = err.chain();
                let Some(top) = chain.next() else {
                    return "Unknown error".extend_with(|_, ext| {
                        ext.set("code", code::INTERNAL_SERVER_ERROR);
                    });
                };

                let chain: Vec<_> = chain.map(|e| e.to_string()).collect();
                top.to_string().extend_with(|_, ext| {
                    ext.set("code", code::INTERNAL_SERVER_ERROR);
                    ext.set("chain", chain);
                })
            }
        }
    }
}

impl<E: std::error::Error> From<RpcError<E>> for async_graphql::ServerError {
    fn from(err: RpcError<E>) -> Self {
        let async_graphql::Error {
            message,
            source,
            extensions,
        } = async_graphql::Error::from(err);

        async_graphql::ServerError {
            message,
            source,
            locations: vec![],
            path: vec![],
            extensions,
        }
    }
}

/// Signal an error that is the user's fault.
pub(crate) fn bad_user_input<E: std::error::Error>(err: E) -> RpcError<E> {
    RpcError::BadUserInput(err)
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    use super::*;

    #[derive(thiserror::Error, Debug)]
    #[error("Boom!")]
    struct Error;

    #[test]
    fn test_bad_user_input() {
        let err: async_graphql::Error = bad_user_input(Error).into();

        assert_eq!(err.message, "Boom!");

        let ext = err.extensions.as_ref().expect("No extensions");
        assert_eq!(ext.get("code"), Some(&code::BAD_USER_INPUT.into()));
    }

    #[test]
    fn test_internal_error() {
        let err: async_graphql::Error = RpcError::<Infallible>::from(
            anyhow!("Root cause")
                .context("Immediate predecessor")
                .context("Main message"),
        )
        .into();

        assert_eq!(err.message, "Main message");

        let ext = err.extensions.as_ref().expect("No extensions");
        assert_eq!(ext.get("code"), Some(&code::INTERNAL_SERVER_ERROR.into()));
        assert_eq!(
            ext.get("chain"),
            Some(&vec!["Immediate predecessor", "Root cause"].into())
        );
    }
}
