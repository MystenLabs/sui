// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module a::test_suppression {
    use sui::object::UID;
    use sui::transfer;

    struct SuperAdminCap has key {
       id: UID
    }

    struct MasterCapability has key {
       id: UID
    }

    struct RootCapV3 has key {
       id: UID
    }

    #[allow(lint(freezing_capability))]
    public fun freeze_super_admin(w: SuperAdminCap) {
        transfer::public_freeze_object(w);
    }

    #[allow(lint(freezing_capability))]
    public fun freeze_master_cap(w: MasterCapability) {
        transfer::public_freeze_object(w);
    }

    #[allow(lint(freezing_capability))]
    public fun freeze_root_cap(w: RootCapV3) {
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
