// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::validator_wrapper {
    use sui::versioned::Versioned;

    friend sui_system::sui_system_state_inner;

    struct ValidatorWrapper has store {
        inner: Versioned
    }
}
