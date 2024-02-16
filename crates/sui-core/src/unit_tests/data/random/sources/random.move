// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module random::random {
    use sui::tx_context::TxContext;
    use sui::random::Random;

    entry fun no_op(_: &Random, _: &mut TxContext) {
    }

}
