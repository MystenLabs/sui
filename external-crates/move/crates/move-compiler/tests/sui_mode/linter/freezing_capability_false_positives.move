// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module a::test_false_positives {
    use sui::object::UID;
    use sui::transfer;

    struct Capture has key {
       id: UID
    }

    struct Handicap has key {
       id: UID
    }

    struct Recap has key {
       id: UID
    }

    struct MyCapybara has key {
       id: UID
    }

    public fun freeze_capture(w: Capture) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_handicap(w: Handicap) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_recap(w: Recap) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_capybara(w: MyCapybara) {
        transfer::public_freeze_object(w);
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::transfer {
    public fun public_freeze_object<T: key>(_: T) {
        abort 0
    }
}