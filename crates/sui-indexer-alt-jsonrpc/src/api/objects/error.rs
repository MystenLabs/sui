// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Object filter contains more than the maximum {max} type filters")]
    FilterTooBig { max: usize },

    #[error("Object filter nested deeper than maximum of {max}")]
    FilterTooDeep { max: usize },

    #[error("Pagination issue: {0}")]
    Pagination(#[from] crate::paginate::Error),

    #[error("Requested {requested} keys, exceeding maximum {max}")]
    TooManyKeys { requested: usize, max: usize },
}
