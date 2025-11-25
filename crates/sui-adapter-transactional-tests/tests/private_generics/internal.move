// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Case: successful call to an internal function through a public function

//# init --addresses test=0x0

//# publish
module test::custom_type {
    use std::internal;

    public struct CustomType {}

    public fun create_internal() {
        let _permit = internal::permit<CustomType>();
    }
}

//# run test::custom_type::create_internal
