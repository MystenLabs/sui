// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that instructions inside a user-written named block ('a) that
// appears inside a macro body are correctly attributed to the macro's
// expansion frame (MacroBody), and that the binder StLoc generated to
// store the macro body's result has a color/location consistent with
// its call-site location.
//
// Two invocations of `foo!` are used so that the binder for each
// invocation's result survives the eliminate_locals optimization
// (because the binder is read in the binop), making the binder StLoc's
// color observable in the snapshot.
module A::m {
    macro fun foo($x: u64): u64 {
        'a: {
            let y = $x + 1;
            y
        }
    }

    public fun test(v: u64): u64 {
        foo!(v) + foo!(v)
    }
}
