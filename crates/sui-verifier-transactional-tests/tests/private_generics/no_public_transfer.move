// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests modules cannot use transfer functions outside of the defining module

//# init --addresses a=0x0 test=0x0

//# publish
module a::m {
    struct S has key { info: sui::object::Info }
}

//# publish
module test::m {
    fun t(s: a::m::S) {
        sui::transfer::transfer(s, @100)
    }
}

//# publish
module test::m {
    fun t(
        s: a::m::S,
        owner_info: &sui::object::Info,
        ctx: &mut sui::tx_context::TxContext,
    ) {
        sui::transfer::transfer_to_object_id(s, owner_info)
    }
}

//# publish
module test::m {
    fun t(s: a::m::S) {
        sui::transfer::freeze_object(s)
    }
}

//# publish
module test::m {
    fun t(s: a::m::S) {
        sui::transfer::share_object(s)
    }
}

//# publish
module test::m {
    struct R has key { info: sui::object::Info }
    fun t(child: a::m::S, owner: &mut R) {
        sui::transfer::transfer_to_object(child, owner)
    }
}

//# publish
module test::m {
    struct R has key { info: sui::object::Info }
    fun t(child: R, owner: &mut a::m::S) {
        sui::transfer::transfer_to_object(child, owner)
    }
}

//# publish
module test::m {
    struct R has key { info: sui::object::Info }
    fun t(child: a::m::S, owner: &sui::object::Info) {
        sui::transfer::transfer_to_object_id(child, owner)
    }
}
