// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Case: try to call internal in a different module, fail publishing

//# init --addresses test=0x0

//# publish
module test::custom_type {
    public struct CustomType {}
}

module test::internal_other_module_fail {
    use std::internal;
    use test::custom_type::CustomType;

    public fun test_internal() {
        let _ = internal::permit<CustomType>();
    }
}
