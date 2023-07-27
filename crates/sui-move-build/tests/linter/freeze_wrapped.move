// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::test {
    use sui::object::UID;
    use sui::transfer;

    struct Inner has key, store {
        id: UID
    }

    struct Wrapper has key, store {
        id: UID,
        inner: Inner,
    }

    struct S has store {
        inner: Inner
    }

    struct IndirectWrapper has key, store {
        id: UID,
        s: S,
    }


    public fun freeze_direct(w: Wrapper) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_indirect(w: IndirectWrapper) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_direct_var(w: Wrapper) {
        let v = w;
        transfer::public_freeze_object(v);
    }
}
