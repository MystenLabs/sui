// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module a::test_false_negatives {
    use sui::object::UID;
    use sui::transfer;

    struct AdminRights has key {
       id: UID
    }

    struct PrivilegeToken has key {
       id: UID
    }

    struct AccessControl has key {
       id: UID
    }

    struct Capv0 has key {
        id: UID
    }

    public fun freeze_admin_rights(w: AdminRights) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_privilege_token(w: PrivilegeToken) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_access_control(w: AccessControl) {
        transfer::public_freeze_object(w);
    }

    public fun freeze_cap_v(w: Capv0) {
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
