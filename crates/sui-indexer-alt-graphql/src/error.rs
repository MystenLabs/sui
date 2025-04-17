// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)] // TODO: Remove once we have a user error.

use std::{convert::Infallible, sync::Arc, time::Duration};

use async_graphql::{ErrorExtensionValues, ErrorExtensions, Response, Value};

use crate::pagination;

/// Error codes for the `extensions.code` field of a GraphQL error that originates from outside
/// GraphQL.
///
/// <https://www.apollographql.com/docs/apollo-server/data/errors/#built-in-error-codes>
pub(crate) mod code {
    pub const BAD_USER_INPUT: &str = "BAD_USER_INPUT";
    pub const GRAPHQL_PARSE_FAILED: &str = "GRAPHQL_PARSE_FAILED";
    pub const GRAPHQL_VALIDATION_FAILED: &str = "GRAPHQL_VALIDATION_FAILED";
    pub const INTERNAL_SERVER_ERROR: &str = "INTERNAL_SERVER_ERROR";
    pub const REQUEST_TIMEOUT: &str = "REQUEST_TIMEOUT";
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum RpcError<E: std::error::Error = Infallible> {
    /// An error that is the user's fault.
    BadUserInput(Arc<E>),

    /// A user error related to pagination and cursors.
    Pagination(#[from] pagination::Error),

    /// An error that is produced by the framework, it gets wrapped so that we can add an error
    /// extension to it.
    GraphQlError(async_graphql::Error),

    /// An error produced by the internal workings of the service (our fault).
    InternalError(Arc<anyhow::Error>),

    /// The request took too long to process.
    RequestTimeout { kind: &'static str, limit: Duration },
}

impl<E: std::error::Error> From<RpcError<E>> for async_graphql::Error {
    fn from(err: RpcError<E>) -> Self {
        match err {
            RpcError::BadUserInput(err) => err.to_string().extend_with(|_, ext| {
                ext.set("code", code::BAD_USER_INPUT);
            }),

            RpcError::Pagination(err) => err.to_string().extend_with(|_, ext| {
                ext.set("code", code::BAD_USER_INPUT);
            }),

            RpcError::GraphQlError(mut err) => {
                fill_error_code(&mut err.extensions, code::INTERNAL_SERVER_ERROR);
                err
            }

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

            RpcError::RequestTimeout { kind, limit } => {
                format!("{kind} timed out after {:.2}s", limit.as_secs_f64()).extend_with(
                    |_, ext| {
                        ext.set("code", code::REQUEST_TIMEOUT);
                    },
                )
            }
        }
    }
}

// Cannot use `#[from]` for this conversion because [`async_graphql::Error`] does not implement
// `std::error::Error`, so it cannot participate in the source/chaining APIs.
impl From<async_graphql::Error> for RpcError {
    fn from(err: async_graphql::Error) -> Self {
        RpcError::GraphQlError(err)
    }
}

// Cannot use `#[from]` for this conversion because [`anyhow::Error`] does not implement `Clone`,
// so it needs to be wrapped in an [`Arc`].
impl From<anyhow::Error> for RpcError {
    fn from(err: anyhow::Error) -> Self {
        RpcError::InternalError(Arc::new(err))
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
    RpcError::BadUserInput(Arc::new(err))
}

/// Signal a timeout. `kind` specifies what operation timed out and is included in the error
/// message.
pub(crate) fn request_timeout(kind: &'static str, limit: Duration) -> RpcError {
    RpcError::RequestTimeout { kind, limit }
}

/// Add a code to the error, if one does not exist already in the error extensions.
pub(crate) fn fill_error_code(ext: &mut Option<ErrorExtensionValues>, code: &str) {
    match ext {
        Some(ref ext) if ext.get("code").is_some() => {}
        Some(ref mut ext) => ext.set("code", code),
        None => {
            let mut singleton = ErrorExtensionValues::default();
            singleton.set("code", code);
            *ext = Some(singleton);
        }
    }
}

/// Get a list of error codes from a GraphQL response. We use these to figure out whether we should
/// log the query at the `debug` or `info` level.
pub(crate) fn error_codes(response: &Response) -> Vec<&str> {
    response
        .errors
        .iter()
        .flat_map(|err| &err.extensions)
        .flat_map(|ext| ext.get("code"))
        .filter_map(|code| {
            if let Value::String(code) = code {
                Some(code.as_str())
            } else {
                None
            }
        })
        .collect()
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

    /// If the GraphQL error does not have a code, it should be set to `INTERNAL_SERVER_ERROR`.
    #[test]
    fn test_graphql_error() {
        let err: async_graphql::Error =
            RpcError::<Infallible>::from(async_graphql::Error::new("Boom!")).into();

        assert_eq!(err.message, "Boom!");

        let ext = err.extensions.as_ref().expect("No extensions");
        assert_eq!(ext.get("code"), Some(&code::INTERNAL_SERVER_ERROR.into()));
    }

    /// If the GraphQL error does already have a code, it should be left as is.
    #[test]
    fn test_graphql_error_existing_code() {
        let err: async_graphql::Error = RpcError::<Infallible>::from(
            async_graphql::Error::new("Boom!")
                .extend_with(|_, ext| ext.set("code", code::BAD_USER_INPUT)),
        )
        .into();

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

    #[test]
    fn test_request_timeout() {
        let err: async_graphql::Error = request_timeout("Kind", Duration::from_secs(5)).into();

        assert_eq!(err.message, "Kind timed out after 5.00s");

        let ext = err.extensions.as_ref().expect("No extensions");
        assert_eq!(ext.get("code"), Some(&code::REQUEST_TIMEOUT.into()));
    }
}
