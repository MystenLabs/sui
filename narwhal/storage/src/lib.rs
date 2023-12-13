// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod certificate_store;
mod consensus_store;
mod node_store;
mod payload_store;
mod proposer_store;
mod randomness_store;
mod vote_digest_store;

pub use certificate_store::*;
pub use consensus_store::*;
pub use node_store::*;
pub use payload_store::*;
pub use proposer_store::*;
pub use randomness_store::*;
use store::TypedStoreError;
pub use vote_digest_store::*;

/// Convenience type to propagate store errors.
pub type StoreResult<T> = Result<T, TypedStoreError>;
