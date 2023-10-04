// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{ErrorExtensionValues, ErrorExtensions, Response, ServerError};
use async_graphql_axum::GraphQLResponse;
use sui_indexer::errors::IndexerError;

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
pub(crate) fn graphql_error_response(code: &str, message: impl Into<String>) -> GraphQLResponse {
    let error = graphql_error(code, message);
    Response::from_errors(error.into()).into()
}

/// Create a generic GraphQL Server Error.
///
/// This error has no path, source, or locations, just a message and an error code.
pub(crate) fn graphql_error(code: &str, message: impl Into<String>) -> ServerError {
    let mut ext = ErrorExtensionValues::default();
    ext.set("code", code);

    ServerError {
        message: message.into(),
        source: None,
        locations: vec![],
        path: vec![],
        extensions: Some(ext),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Provide one of digest or sequence_number, not both")]
    InvalidCheckpointQuery,
    #[error("String is not valid base58: {0}")]
    InvalidBase58(String),
    #[error("Invalid digest length: expected {expected}, actual {actual}")]
    InvalidDigestLength { expected: usize, actual: usize },
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
    #[error("Internal error occurred while processing request: {0}")]
    Internal(String),
}

impl ErrorExtensions for Error {
    fn extend(&self) -> async_graphql::Error {
        async_graphql::Error::new(format!("{}", self)).extend_with(|_err, e| match self {
            Error::InvalidCheckpointQuery
            | Error::CursorNoBeforeAfter
            | Error::CursorNoFirstLast
            | Error::CursorNoReversePagination
            | Error::InvalidCursor(_)
            | Error::CursorConnectionFetchFailed(_)
            | Error::MultiGet(_)
            | Error::InvalidBase58(_)
            | Error::InvalidDigestLength { .. } => {
                e.set("code", code::BAD_USER_INPUT);
            }
            Error::Internal(_) => {
                e.set("code", code::INTERNAL_SERVER_ERROR);
            }
        })
    }
}

impl From<IndexerError> for Error {
    fn from(e: IndexerError) -> Self {
        Error::Internal(e.to_string())
    }
}
