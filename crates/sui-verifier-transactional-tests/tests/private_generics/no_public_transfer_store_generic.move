// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests modules cannot use transfer internal functions outside of the defining module
// Note: it is not possible to make a generic type `T<...> has key, store`
// where a given instantiation`T<...>` has key but does _not_ have store

//# init --addresses test=0x0

//# publish
module test::m {
    fun t<T: key + store>(s: T) {
        sui::transfer::transfer(s, @100)
    }
}

//# publish
module test::m {
    fun t<T: key + store>(s: T) {
        sui::transfer::freeze_object(s)
    }
}

//# publish
module test::m {
    fun t<T: key + store>(s: T) {
        sui::transfer::share_object(s)
    }
}
