// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use super::sui_address::SuiAddress;

use async_graphql::*;

/// The extra data required to turn a `TransactionKind` into a
/// `TransactionData` in a dry-run.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct TransactionMeta {
    pub sender: Option<SuiAddress>,

    pub gas_price: Option<BigInt>,

    pub gas_objects: Option<Vec<SuiAddress>>,
}

impl Default for TransactionMeta {
    fn default() -> Self {
        Self {
            sender: Some(SuiAddress::from_array([0; SuiAddress::LENGTH])),
            gas_price: None,
            gas_objects: None,
        }
    }
}
