// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{ErrorExtensionValues, ErrorExtensions, Response, ServerError};
use async_graphql_axum::GraphQLResponse;

/// Error codes for the `extensions.code` field of a GraphQL error that originates from outside
/// GraphQL.
/// `<https://www.apollographql.com/docs/apollo-server/data/errors/#built-in-error-codes>`
pub(crate) mod code {
    pub const BAD_REQUEST: &str = "BAD_REQUEST";
    pub const BAD_USER_INPUT: &str = "BAD_USER_INPUT";
    pub const INTERNAL_SERVER_ERROR: &str = "INTERNAL_SERVER_ERROR";
}

/// Create a GraphQL Response containing an Error.
///
/// Most errors produced by the service will automatically be wrapped in a `GraphQLResponse`,
/// because they will originate from within the GraphQL implementation.  This function is intended
/// for errors that originated from outside of GraphQL (such as in middleware), but that need to be
/// ingested by GraphQL clients.
pub(crate) fn graphql_error(code: &str, message: String) -> GraphQLResponse {
    let mut ext = ErrorExtensionValues::default();
    ext.set("code", code);

    let error = ServerError {
        message,
        source: None,
        locations: vec![],
        path: vec![],
        extensions: Some(ext),
    };

    Response::from_errors(error.into()).into()
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("'before' and 'after' must not be used together")]
    CursorNoBeforeAfter,
    #[error("'first' and 'last' must not be used together")]
    CursorNoFirstLast,
    #[error("reverse pagination is not supported")]
    CursorNoReversePagination,
    #[error("Invalid cursor: {0}")]
    InvalidCursor(String),
    #[error("Data has changed since cursor was generated: {0}")]
    CursorConnectionFetchFailed(String),
    #[error("Error received in multi-get query: {0}")]
    MultiGet(String),
    #[error("Internal error occurred while processing request.")]
    Internal(String),
}

impl ErrorExtensions for Error {
    fn extend(&self) -> async_graphql::Error {
        async_graphql::Error::new(format!("{}", self)).extend_with(|_err, e| match self {
            Error::CursorNoBeforeAfter
            | Error::CursorNoFirstLast
            | Error::CursorNoReversePagination
            | Error::InvalidCursor(_)
            | Error::CursorConnectionFetchFailed(_)
            | Error::MultiGet(_) => {
                e.set("code", code::BAD_USER_INPUT);
            }
            Error::Internal(_) => {
                e.set("code", code::INTERNAL_SERVER_ERROR);
            }
        })
    }
}
