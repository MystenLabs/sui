// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct StorageFund {
    pub total_object_storage_rebates: Option<BigInt>,
    pub non_refundable_balance: Option<BigInt>,
}
