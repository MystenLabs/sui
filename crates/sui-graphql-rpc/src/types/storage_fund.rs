// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use async_graphql::*;

/// SUI set aside to account for objects stored on-chain.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct StorageFund {
    /// Sum of storage rebates of live objects on chain.
    pub total_object_storage_rebates: Option<BigInt>,

    /// The portion of the storage fund that will never be refunded through storage rebates.
    ///
    /// The system maintains an invariant that the sum of all storage fees into the storage fund is
    /// equal to the sum of of all storage rebates out, the total storage rebates remaining, and the
    /// non-refundable balance.
    pub non_refundable_balance: Option<BigInt>,
}
