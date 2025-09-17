// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module package_resolver_test::resolver_test {
    /// Custom token type for testing type resolution
    public struct NestedObject has store {}

    public struct SimpleObject has key, store {
        id: UID,
        value: NestedObject,
    }

    fun init(ctx: &mut TxContext) {
        let simple_object = SimpleObject {
            id: object::new(ctx),
            value: NestedObject {}
        };

        transfer::public_transfer(simple_object, tx_context::sender(ctx));
    }
}
