// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    parser::types::OperationType, ErrorExtensions, Name, PathSegment, Pos, ServerError,
};

use crate::error::code;

pub(super) struct Error {
    kind: ErrorKind,
    path: Vec<PathSegment>,
    pos: Option<Pos>,
}

#[derive(thiserror::Error, Debug)]
pub(super) enum ErrorKind {
    #[error("Query nesting is over {0}")]
    InputNesting(u32),

    #[error("Query has over {0} nodes")]
    InputNodes(u32),

    #[error(transparent)]
    InternalError(#[from] anyhow::Error),

    #[error(
        "Query is estimated to produce over {0} output nodes. Try fetching fewer fields or \
         fetching fewer items per page in paginated or multi-get fields."
    )]
    OutputNodes(u32),

    #[error("Page size is too large: {actual} > {limit}")]
    PageSizeTooLarge { limit: u32, actual: u32 },

    #[error("Request too large {actual}B > {limit}B")]
    PayloadSizeOverall { limit: u32, actual: u64 },

    #[error("Query payload too large: {actual}B > {limit}B")]
    PayloadSizeQuery { limit: u32, actual: u32 },

    #[error("Transaction payload exceeded limit of {limit}B")]
    PayloadSizeTx { limit: u32 },

    #[error("{0} not supported")]
    SchemaNotSupported(OperationType),

    #[error("Fragment {0} referred to but not found in document")]
    UnknownFragment(String),

    #[error("Variable {0} is not provided in the query")]
    VariableNotFound(Name),
}

impl Error {
    /// An error that occurred at a specific position in the query.
    pub(super) fn new(kind: ErrorKind, path: Vec<PathSegment>, pos: Pos) -> Self {
        Self {
            kind,
            path,
            pos: Some(pos),
        }
    }

    /// An error that applies to the query as a whole.
    pub(super) fn new_global(kind: ErrorKind) -> Self {
        Self {
            kind,
            path: vec![],
            pos: None,
        }
    }
}

impl From<Error> for ServerError {
    fn from(err: Error) -> Self {
        let Error { kind, pos, path } = err;

        let code = if matches!(kind, ErrorKind::InternalError(_)) {
            code::INTERNAL_SERVER_ERROR
        } else {
            code::GRAPHQL_VALIDATION_FAILED
        };

        let async_graphql::Error {
            message,
            source,
            extensions,
        } = kind.extend_with(|_, ext| ext.set("code", code));

        ServerError {
            message,
            source,
            locations: pos.into_iter().collect(),
            path,
            extensions,
        }
    }
}
