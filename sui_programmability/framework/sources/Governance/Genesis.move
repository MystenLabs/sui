// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::Genesis {
    use Std::Vector;

    use Sui::SuiSystem;
    use Sui::TxContext::TxContext;

    fun init(ctx: &mut TxContext) {
        let validators = Vector::empty();
        SuiSystem::create(
            validators,
            ctx
        );
    }
}