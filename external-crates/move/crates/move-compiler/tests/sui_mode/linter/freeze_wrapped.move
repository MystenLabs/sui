// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module a::test {
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

    struct GenWrapper<T: key + store> has key, store {
        id: UID,
        inner: T,
    }

    struct S2<T: key + store> has store {
        inner: T
    }

    struct IndirectGenWrapper<T: key + store> has key, store {
        id: UID,
        inner: S2<T>,
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

    public fun freeze_direct_gen<T: key + store>(w: GenWrapper<T>) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_indirect_gen<T: key + store>(w: IndirectGenWrapper<T>) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_arg(w1: Wrapper, w2: Wrapper) {
        transfer::public_freeze_object({ transfer::public_freeze_object(w1) ; w2});
    }

}

module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::transfer {
    const ZERO: u64 = 0;
    public fun public_freeze_object<T: key>(_: T) {
        abort ZERO
    }
}
