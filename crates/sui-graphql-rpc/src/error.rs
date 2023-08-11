// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{ErrorExtensionValues, Response, ServerError};
use async_graphql_axum::GraphQLResponse;

/// Error codes for the `extensions.code` field of a GraphQL error that originates from outside
/// GraphQL.
pub mod code {
    pub const BAD_REQUEST: &str = "BAD_REQUEST";
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
