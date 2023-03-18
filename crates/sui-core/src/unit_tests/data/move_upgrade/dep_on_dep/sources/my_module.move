// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module a::my_module {
    use dep_on_upgrading_package::my_module;

    public fun call_return_0(): u64 { my_module::call_return_0() }
}
