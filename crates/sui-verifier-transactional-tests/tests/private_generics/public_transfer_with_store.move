// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests modules can use transfer functions outside of the defining module, if the type
// has store

//# init --addresses a=0x0 t1=0x0 t2=0x0 t3=0x0 t4=0x0 t5=0x0 t6=0x0 t7=0x0 t8=0x0 t9=0x0

//# publish
module a::m {
    struct S has key, store { id: sui::object::UID }
}

//# publish --dependencies a
module t1::m {
    fun t(s: a::m::S) {
        sui::transfer::transfer(s, @100)
    }
}

//# publish --dependencies a
module t3::m {
    fun t(s: a::m::S) {
        sui::transfer::freeze_object(s)
    }
}

//# publish --dependencies a
module t4::m {
    fun t(s: a::m::S) {
        sui::transfer::share_object(s)
    }
}
