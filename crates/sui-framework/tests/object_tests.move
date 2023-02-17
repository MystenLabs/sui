// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::object_tests {
    use sui::address;
    use sui::object;
    use sui::tx_context;
    use sui::test_scenario::{Self as ts};

    const EDifferentAddress: u64 = 0xF000;
    const EDifferentBytes: u64 = 0xF001;
    const EAddressRoundTrip: u64 = 0xF002;

    #[test]
    fun test_bytes_address_roundtrip() {
        let ctx = tx_context::dummy();

        let uid0 = object::new(&mut ctx);
        let uid1 = object::new(&mut ctx);

        let addr0 = object::uid_to_address(&uid0);
        let byte0 = object::uid_to_bytes(&uid0);
        let addr1 = object::uid_to_address(&uid1);
        let byte1 = object::uid_to_bytes(&uid1);

        assert!(addr0 != addr1, EDifferentAddress);
        assert!(byte0 != byte1, EDifferentBytes);

        assert!(addr0 == address::from_bytes(byte0), EAddressRoundTrip);
        assert!(addr1 == address::from_bytes(byte1), EAddressRoundTrip);

        object::delete(uid0);
        object::delete(uid1);
    }

    #[test]
    fun test_deletion_proof() {
        use sui::deletion_proof as proof;

        let test = ts::begin(@0x1);
        ts::next_tx(&mut test, @0x1); {
            proof::create(ts::ctx(&mut test));
        };

        ts::next_tx(&mut test, @0x1); {
            let obj = ts::take_from_sender<proof::Test>(&mut test);
            let _ = proof::delete(obj);
        };

        ts::end(test);
    }
}

#[test_only]
/// Module used to reproduce proof deletion logic.
module sui::deletion_proof {
    use sui::tx_context::{sender, TxContext};
    use sui::object::{Self, UID, DeletedUID};

    /// A Test struct
    struct Test has key { id: UID }

    public entry fun create(ctx: &mut TxContext) {
        sui::transfer::transfer(Test { id: object::new(ctx) }, sender(ctx))
    }

    public fun delete(t: Test): DeletedUID {
        let Test { id } = t;
        object::delete_with_proof(id)
    }
}
