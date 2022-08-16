// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests modules cannot use transfer functions outside of the defining module

//# init --addresses a=0x0 test=0x0

//# publish
module a::m {
    struct S has key { id: sui::object::UID }
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
        owner_id: &mut sui::object::UID,
        ctx: &mut sui::tx_context::TxContext,
    ) {
        sui::transfer::transfer_to_object_id(s, owner_id)
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
    struct R has key { id: sui::object::UID }
    fun t(child: a::m::S, owner: &mut R) {
        sui::transfer::transfer_to_object(child, owner)
    }
}

//# publish
module test::m {
    struct R has key { id: sui::object::UID }
    fun t(child: R, owner: &mut a::m::S) {
        sui::transfer::transfer_to_object(child, owner)
    }
}

//# publish
module test::m {
    struct R has key { id: sui::object::UID }
    fun t(child: a::m::S, owner: &mut sui::object::UID) {
        sui::transfer::transfer_to_object_id(child, owner)
    }
}
