// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0

//# lint
#[no_lint]
module test::module_level {
    struct UnusedType has drop {}

    fun unused_private() {}
}

//# lint
module test::struct_level {
    #[no_lint]
    struct UnusedType has drop {}
}

//# lint
module test::function_level {
    #[no_lint]
    fun unused_private() {}
}
