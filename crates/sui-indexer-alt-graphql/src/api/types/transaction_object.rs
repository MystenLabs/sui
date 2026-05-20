// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Union;

use crate::api::types::object_change::ObjectChange;
use crate::api::types::unchanged_consensus_object::ConsensusObjectRead;

/// An object as it appears in a specific transaction. The variant discriminates whether the object was changed by the transaction or read as an unchanged consensus (shared) input.
#[derive(Union)]
pub(crate) enum TransactionObject {
    Changed(ObjectChange),
    ConsensusRead(ConsensusObjectRead),
}
