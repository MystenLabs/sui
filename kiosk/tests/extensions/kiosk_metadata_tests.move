// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A set of tests for the `kiosk_metadata` extension.
#[test_only]
module kiosk::kiosk_metadata_tests{

    use kiosk::kiosk_metadata;
    use sui::tx_context::{Self, TxContext};
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::coin;
    use sui::vec_map::{Self, VecMap};
    use std::string::{utf8, String};
    use sui::test_scenario as ts;
    use sui::dynamic_field as df;
    use sui::transfer;

    const Alice: address =  @0x1;
    const Bob: address = @0x2;

    // Prepare: dummy context
    public fun ctx(): TxContext { tx_context::dummy() }

    fun get_metadata_vec_map(include_data: bool): VecMap<String, String> {

        let vec_map = vec_map::empty();
        
        if(!include_data) return vec_map;

        vec_map::insert(&mut vec_map, utf8(b"key1"), utf8(b"value1"));
        vec_map
    }

    fun get_kiosk(ctx: &mut TxContext): (Kiosk, KioskOwnerCap) {
        kiosk::new(ctx)
    }

    fun return_kiosk(kiosk: Kiosk, cap: KioskOwnerCap, ctx: &mut TxContext): u64 {
        let profits = kiosk::close_and_withdraw(kiosk, cap, ctx);
        coin::burn_for_testing(profits)
    }

    // test sets 
    // Success. Normal execution flow (both add_field and add_multiple_fields)
    // Create a kiosk, create metadata for the kiosk, add_field a (key,val) to it. 
    // Also adds an array of (key,val) to it too. (all keys are unique)
    // And removes an existing field successfully.
    #[test]
    fun normal_execution_flow(){
        let ctx = &mut ctx();
        // create kiosk 
        let (kiosk, kioskOwnerCap) = get_kiosk(ctx);
        

        let vec_map = get_metadata_vec_map(false);
        // enable metadata extension
        kiosk_metadata::enable(&mut kiosk, &kioskOwnerCap, vec_map);

        // get metadata value
        let metadata = kiosk_metadata::get_metadata(kiosk::uid_mut(&mut kiosk));

        // get vecmap from metadata struct
        let metadata_vecmap = kiosk_metadata::get_metadata_vecmap(metadata);

        // check that metadata vecmap is empty.
        assert!(vec_map::is_empty(metadata_vecmap), 1);

        // replace with a full vecmap
        kiosk_metadata::replace(&mut kiosk, &kioskOwnerCap, get_metadata_vec_map(true));

        // get metadata value again! 
        metadata = kiosk_metadata::get_metadata(kiosk::uid_mut(&mut kiosk));

        // get vecmap from metadata struct
        metadata_vecmap = kiosk_metadata::get_metadata_vecmap(metadata);
                // check that metadata vecmap is empty.
        assert!(vec_map::size(metadata_vecmap) == 1, 1);


        return_kiosk(kiosk, kioskOwnerCap, ctx);
    }

    // Failed execution: Create 2 kiosks, try to create metadata to a
    // different kiosk using owned OwnerCap
    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotOwner)]
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
            kiosk_metadata::enable(&mut aliceKiosk, &kioskOwnerCap, get_metadata_vec_map(false));

            return_kiosk(kiosk, kioskOwnerCap, ts::ctx(&mut scenario));
            ts::return_shared(aliceKiosk);
        };

        ts::end(scenario);
    }

    // Failed execution: Tries to replace metadata for a kiosk that hasn't already created metadata for (df::EFieldDoesNotExist)
    #[test]
    #[expected_failure(abort_code = df::EFieldDoesNotExist)]
    fun not_created_metadata(){

        let ctx = &mut ctx();
        let (kiosk, kioskOwnerCap) = get_kiosk(ctx);
        kiosk_metadata::replace(&mut kiosk, &kioskOwnerCap, get_metadata_vec_map(true));

        return_kiosk(kiosk, kioskOwnerCap, ctx);
    }

    // Failed execution: Try to create kiosk_metadata while it already exists (EAlreadyEnabled)
    #[test]
    #[expected_failure(abort_code = df::EFieldAlreadyExists)]
    fun already_created_metadata(){

        let ctx = &mut ctx();
        let (kiosk, kioskOwnerCap) = get_kiosk(ctx);
        kiosk_metadata::enable(&mut kiosk, &kioskOwnerCap, get_metadata_vec_map(false));

        // enable metadata extension again and should error.
        kiosk_metadata::enable(&mut kiosk, &kioskOwnerCap, get_metadata_vec_map(false));

        return_kiosk(kiosk, kioskOwnerCap, ctx);
    }

    // Failed execution: Try to replace metadata after removing it.
    #[test]
    #[expected_failure(abort_code = df::EFieldDoesNotExist)]
    fun try_to_replace_on_removed_metadata(){

        let ctx = &mut ctx();
        let (kiosk, kioskOwnerCap) = get_kiosk(ctx);
        kiosk_metadata::enable(&mut kiosk, &kioskOwnerCap, get_metadata_vec_map(false));

        // remove the metadata
        kiosk_metadata::remove(&mut kiosk, &kioskOwnerCap);

        // try to replace non existing metadata;
        kiosk_metadata::replace(&mut kiosk, &kioskOwnerCap, get_metadata_vec_map(true));
        return_kiosk(kiosk, kioskOwnerCap, ctx);
    }

}
