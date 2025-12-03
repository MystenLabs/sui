// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Case: successful call through delegated public function

//# init --addresses test=0x0

//# publish
module test::custom_type {
    public struct CustomType {}

    public fun create_internal(): internal::Permit<CustomType> {
        internal::permit<CustomType>()
    }
}

module test::internal_delegated {
    use test::custom_type;

    public fun test_internal() {
        let _ = custom_type::create_internal();
    }
}

//# run test::internal_delegated::test_internal
