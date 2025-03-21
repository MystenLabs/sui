// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{ErrorExtensions, PathSegment, Pos, ServerError};

use crate::error::code;

pub(super) struct Error {
    kind: ErrorKind,
    path: Vec<PathSegment>,
    pos: Pos,
}

#[derive(thiserror::Error, Debug)]
pub(super) enum ErrorKind {
    #[error("Query nesting is over {0}")]
    InputNesting(u32),

    #[error("Query has over {0} nodes")]
    InputNodes(u32),

    #[error("Fragment {0} referred to but not found in document")]
    UnknownFragment(String),
}

impl Error {
    pub(super) fn new(kind: ErrorKind, path: Vec<PathSegment>, pos: Pos) -> Self {
        Self { kind, path, pos }
    }
}

impl From<Error> for ServerError {
    fn from(err: Error) -> Self {
        let Error { kind, pos, path } = err;
        kind.extend_with(|_, ext| ext.set("code", code::GRAPHQL_VALIDATION_FAILED))
            .into_server_error(pos)
            .with_path(path)
    }
}
