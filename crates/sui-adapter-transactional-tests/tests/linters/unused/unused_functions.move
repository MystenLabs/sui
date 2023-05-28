// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0

//# lint
module test::unused_functions {
    friend test::unused_functions_friend;

    public fun f() {
        used_private()
    }

    // make sure that defining a function after its use does not matter
    fun unused_private() {}

    fun used_private() {}

    public(friend) fun used_friend() {}

    public(friend) fun unused_friend() {}
}
module test::unused_functions_friend {
    use test::unused_functions;

    public fun g() {
        unused_functions::used_friend()
    }
}
