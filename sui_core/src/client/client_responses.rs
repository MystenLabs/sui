// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::messages::CertifiedOrder;
use sui_types::object::Object;

pub struct SplitCoinResponse {
    /// Certificate of the order
    pub certificate: CertifiedOrder,
    /// The updated original coin object after split
    pub updated_coin: Object,
    /// All the newly created coin objects generated from the split
    pub new_coins: Vec<Object>,
    /// The updated gas payment object after deducting payment
    pub updated_gas: Object,
}

pub struct MergeCoinResponse {
    /// Certificate of the order
    pub certificate: CertifiedOrder,
    /// The updated original coin object after merge
    pub updated_coin: Object,
    /// The updated gas payment object after deducting payment
    pub updated_gas: Object,
}
