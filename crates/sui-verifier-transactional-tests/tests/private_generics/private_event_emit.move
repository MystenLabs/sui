// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests modules cannot emit events for types not defined in the current module

//# init --addresses a=0x0 test=0x0

//# publish
module a::m {
    struct S has copy, drop {}
}

//# publish --dependencies a
module test::m {
    fun t(s: a::m::S) {
        sui::event::emit(s)
    }
}

//# publish
module test::m {
    fun t<T: copy + drop>(x: T) {
        sui::event::emit(x)
    }
}

//# publish
module test::m {
    fun t(x: u64) {
        sui::event::emit(x)
    }
}

//# publish
module test::m {
    struct X has copy, drop {}
    fun t(x: vector<X>) {
        sui::event::emit(x)
    }
}
