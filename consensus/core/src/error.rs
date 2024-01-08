// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

/// Errors that can occur when processing blocks, reading from storage, or encountering shutdown.
#[allow(unused)]
#[derive(Clone, Debug, Error)]
pub enum ConsensusError {
    #[error("Error deserializing block")]
    MalformattedBlock,
}

#[allow(unused)]
pub type ConsensusResult<T> = Result<T, ConsensusError>;
