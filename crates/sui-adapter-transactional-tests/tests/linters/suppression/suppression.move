// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0

//# lint
#[allow(all)]
module test::module_level {
    struct UnusedType has drop {}

    fun unused_private() {}
}

//# lint
module test::struct_level {
    #[allow(all)]
    struct UnusedType has drop {}
}

//# lint
module test::function_level {
    #[allow(all)]
    fun unused_private() {}
}
