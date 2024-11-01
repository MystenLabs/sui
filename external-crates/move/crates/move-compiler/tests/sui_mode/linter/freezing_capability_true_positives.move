// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module a::test_true_positives {
    use sui::object::UID;
    use sui::transfer;

    struct AdminCap has key {
       id: UID
    }

    struct UserCapability has key {
       id: UID
    }

    struct OwnerCapV2 has key {
       id: UID
    }

    public fun freeze_cap1(w: AdminCap) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_cap2(w: UserCapability) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_cap3(w: OwnerCapV2) {
        transfer::public_freeze_object(w);
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
