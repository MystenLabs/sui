// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A set of tests for the `kiosk_metadata` extension.
#[test_only]
module kiosk::kiosk_metadata_tests{

    use kiosk::kiosk_metadata;
    use sui::tx_context::{Self, TxContext};
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::coin;
    use sui::vec_map;
    use std::vector;
    use std::string::{utf8, String};
    use sui::test_scenario as ts;
    use sui::dynamic_field as df;
    use sui::transfer;

    const Alice: address =  @0x1;
    const Bob: address = @0x2;

    // Prepare: dummy context
    public fun ctx(): TxContext { tx_context::dummy() }

    fun get_kiosk(ctx: &mut TxContext): (Kiosk, KioskOwnerCap) {
        kiosk::new(ctx)
    }

    fun return_kiosk(kiosk: Kiosk, cap: KioskOwnerCap, ctx: &mut TxContext): u64 {
        let profits = kiosk::close_and_withdraw(kiosk, cap, ctx);
        coin::burn_for_testing(profits)
    }

    // test sets 
    // 1. Success. Normal execution flow (both add and add_multiple)
    // Create a kiosk, create metadata for the kiosk, add a (key,val) to it. 
    // Also adds an array of (key,val) to it too. (all keys are unique)
    // And removes an existing field successfully.
    #[test]
    fun normal_execution_flow(){
        let ctx = &mut ctx();
        // create kiosk 
        let (kiosk, kioskOwnerCap) = get_kiosk(ctx);
        
        // enable metadata extension
        kiosk_metadata::enable(&mut kiosk, &kioskOwnerCap);

        // get metadata value
        let metadata = kiosk_metadata::get_metadata(kiosk::uid_mut(&mut kiosk));

        // get vecmap from metadata struct
        let metadata_vecmap = kiosk_metadata::get_metadata_vecmap(metadata);

        // check that metadata vecmap is empty.
        assert!(vec_map::is_empty(metadata_vecmap), 1);

        // add two fields
        kiosk_metadata::add(&mut kiosk, &kioskOwnerCap, utf8(b"key"), utf8(b"value"));
        kiosk_metadata::add(&mut kiosk, &kioskOwnerCap, utf8(b"hello"), utf8(b"world"));

        let keys_vec = vector::empty<String>();
        vector::push_back(&mut keys_vec, utf8(b"lorem"));
        vector::push_back(&mut keys_vec, utf8(b"ipsum"));
        let values_vec = vector::empty<String>();
        vector::push_back(&mut values_vec, utf8(b"test"));
        vector::push_back(&mut values_vec, utf8(b"test"));

        // add an array of key,vals
        kiosk_metadata::add_multiple(&mut kiosk, &kioskOwnerCap, keys_vec, values_vec);

        //remove an existing field!
        kiosk_metadata::remove_key(&mut kiosk, &kioskOwnerCap, utf8(b"key"));

        return_kiosk(kiosk, kioskOwnerCap, ctx);
    }

    // 2. Failed execution: Create 2 kiosks, try to create metadata to a
    // different kiosk using owned OwnerCap
    #[test]
    #[expected_failure(abort_code = kiosk_metadata::ENotKioskOwner)]
    fun invalidOwnerCapExecution(){
        let scenario = ts::begin(Alice);
        {
            // create kiosk as Alice
            let (kiosk, kioskOwnerCap) = get_kiosk(ts::ctx(&mut scenario));

            // share kiosk
            transfer::public_share_object(kiosk);
            // transfer ownerCap to Alice
            transfer::public_transfer(kioskOwnerCap, Alice);
        };
        ts::next_tx(&mut scenario, Bob);
        {
            // get registry
            // create kiosk as Bob
            let (kiosk, kioskOwnerCap) = get_kiosk(ts::ctx(&mut scenario));

            // get Alice's owner Cap
            let aliceKiosk = ts::take_shared<Kiosk>(&scenario);

            // try to enable Alices's kiosk with Bob's kioskOwnerCap
            kiosk_metadata::enable(&mut aliceKiosk, &kioskOwnerCap);

            return_kiosk(kiosk, kioskOwnerCap, ts::ctx(&mut scenario));
            ts::return_shared(aliceKiosk);
        };

        ts::end(scenario);
    }

    // 3. Failed execution: Try to add metadata for a kiosk that hasn't already created metadata for (df::EFieldDoesNotExist)
    #[test]
    #[expected_failure(abort_code = df::EFieldDoesNotExist)]
    fun not_created_metadata(){

        let ctx = &mut ctx();
        let (kiosk, kioskOwnerCap) = get_kiosk(ctx);

        // add metadata without having enabled the Kiosk
        kiosk_metadata::add(&mut kiosk, &kioskOwnerCap, utf8(b"key"), utf8(b"value"));

        return_kiosk(kiosk, kioskOwnerCap, ctx);
    }

    // 4. Failed execution: Try to create kiosk_metadata while it already exists (EAlreadyEnabled)
    #[test]
    #[expected_failure(abort_code = df::EFieldAlreadyExists)]
    fun already_created_metadata(){

        let ctx = &mut ctx();
        let (kiosk, kioskOwnerCap) = get_kiosk(ctx);
        kiosk_metadata::enable(&mut kiosk, &kioskOwnerCap);

        // enable metadata extension again and should error.
        kiosk_metadata::enable(&mut kiosk, &kioskOwnerCap);

        return_kiosk(kiosk, kioskOwnerCap, ctx);

    }
    
    // // 5. Failed execution: Try to insert key,vals array to a kiosk with assymetric size of keys, values (EVecLengthMismatch)
    #[test]
    #[expected_failure(abort_code = kiosk_metadata::EVecLengthMismatch)]
    fun asymmetric_size_vecs(){

        let ctx = &mut ctx();
        let (kiosk, kioskOwnerCap) = get_kiosk(ctx);
        kiosk_metadata::enable(&mut kiosk, &kioskOwnerCap);

        let keys_vec = vector::empty<String>();
        vector::push_back(&mut keys_vec, utf8(b"lorem"));
        vector::push_back(&mut keys_vec, utf8(b"ipsum"));
        let values_vec = vector::empty<String>();
        vector::push_back(&mut values_vec, utf8(b"test"));

        // adds asymmetric vector lengths and should error.
        kiosk_metadata::add_multiple(&mut kiosk, &kioskOwnerCap, keys_vec, values_vec);

        return_kiosk(kiosk, kioskOwnerCap, ctx);
    }

    // // 6. Failed execution. Tries to remove a field that doesn't exist.
    #[test]
    #[expected_failure(abort_code = kiosk_metadata::EKeyNotExists)]
    fun remove_key_that_does_not_exist(){

        let ctx = &mut ctx();
        let (kiosk, kioskOwnerCap) = get_kiosk(ctx);
        kiosk_metadata::enable(&mut kiosk, &kioskOwnerCap);

        // try to remove a key that doesn't exist!
        kiosk_metadata::remove_key(&mut kiosk, &kioskOwnerCap, utf8(b"key"));
        return_kiosk(kiosk, kioskOwnerCap, ctx);
    }
}
