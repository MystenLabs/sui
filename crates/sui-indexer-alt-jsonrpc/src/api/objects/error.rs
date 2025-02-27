// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    #[error("Pagination issue: {0}")]
    Pagination(#[from] crate::paginate::Error),

    #[error("Requested {requested} keys, exceeding maximum {max}")]
    TooManyKeys { requested: usize, max: usize },
}
