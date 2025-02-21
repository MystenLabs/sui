// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    #[error("Domain not found: {0}")]
    NotFound(String),

    #[error(transparent)]
    NameService(sui_name_service::NameServiceError),
}
