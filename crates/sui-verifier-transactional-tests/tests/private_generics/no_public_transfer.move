// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests modules cannot use transfer internal functions outside of the defining module

//# init --addresses a=0x0 test=0x0

//# publish
module a::m {
    struct S has key { id: sui::object::UID }
}

//# publish --dependencies a
module test::m {
    fun t(s: a::m::S) {
        sui::transfer::transfer(s, @100)
    }
}

//# publish --dependencies a
module test::m {
    fun t(s: a::m::S) {
        sui::transfer::freeze_object(s)
    }
}

//# publish --dependencies a
module test::m {
    fun t(s: a::m::S) {
        sui::transfer::share_object(s)
    }
}
