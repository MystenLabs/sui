// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::result::Error as DieselError;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    PgCreate(anyhow::Error),

    #[error(transparent)]
    PgConnect(anyhow::Error),

    #[error(transparent)]
    PgRunQuery(#[from] DieselError),

    #[error(transparent)]
    BigtableCreate(anyhow::Error),

    #[error(transparent)]
    BigtableRead(anyhow::Error),

    #[error(transparent)]
    Serde(anyhow::Error),
}
