// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{ErrorExtensionValues, ErrorExtensions, Pos, Response, ServerError};
use async_graphql_axum::GraphQLResponse;
use sui_indexer::errors::IndexerError;
use sui_json_rpc::name_service::NameServiceError;

/// Error codes for the `extensions.code` field of a GraphQL error that originates from outside
/// GraphQL.
/// `<https://www.apollographql.com/docs/apollo-server/data/errors/#built-in-error-codes>`
pub(crate) mod code {
    pub const BAD_REQUEST: &str = "BAD_REQUEST";
    pub const BAD_USER_INPUT: &str = "BAD_USER_INPUT";
    pub const INTERNAL_SERVER_ERROR: &str = "INTERNAL_SERVER_ERROR";
    pub const REQUEST_TIMEOUT: &str = "REQUEST_TIMEOUT";
    pub const UNKNOWN: &str = "UNKNOWN";
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

pub(crate) fn graphql_error_at_pos(
    code: &str,
    message: impl Into<String>,
    pos: Pos,
) -> ServerError {
    let mut ext = ErrorExtensionValues::default();
    ext.set("code", code);

    ServerError {
        message: message.into(),
        source: None,
        locations: vec![pos],
        path: vec![],
        extensions: Some(ext),
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("Unsupported protocol version requested. Min supported: {0}, max supported: {1}")]
    ProtocolVersionUnsupported(u64, u64),
    #[error(transparent)]
    NameService(#[from] NameServiceError),
    #[error("'first' and 'last' must not be used together")]
    CursorNoFirstLast,
    #[error("Connection's page size of {0} exceeds max of {1}")]
    PageTooLarge(u64, u64),
    // Catch-all for client-fault errors
    #[error("{0}")]
    Client(String),
    #[error("Internal error occurred while processing request: {0}")]
    Internal(String),
}

impl ErrorExtensions for Error {
    fn extend(&self) -> async_graphql::Error {
        async_graphql::Error::new(format!("{}", self)).extend_with(|_err, e| match self {
            Error::NameService(_)
            | Error::CursorNoFirstLast
            | Error::PageTooLarge(_, _)
            | Error::ProtocolVersionUnsupported(_, _)
            | Error::Client(_) => {
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
