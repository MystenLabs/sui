// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Tests kiosk mail
module kiosk_mail::mail_tests {
    use std::option;

    use sui::object::{Self, UID};
    use sui::test_scenario::{Self as ts, Scenario, ctx};
    use sui::tx_context::TxContext;
    use sui::transfer_policy::{Self as policy, TransferPolicy, TransferPolicyCap};
    use sui::package;
    use sui::coin;
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};

    use kiosk_mail::mail::{Self, Mail};

    /// A fake OTW to init tests
    struct OTW has drop {}

    /// A demo struct to send mail!
    struct DemoItem has key, store {
        id: UID
    }

    const ADDR_ONE: address = @0x1;
    const ADDR_TWO: address = @0x2;

    #[test]
    fun send_and_claim(){
        let scenario_val = init_test();
        let scenario = &mut scenario_val;

        let (policy, cap) = prepare_policy<DemoItem>(ctx(scenario));

        ts::next_tx(scenario, ADDR_ONE);
        let item = new_item(scenario);

        let item_id = object::id(&item);

        mail::send(item, &policy, false, ADDR_TWO, ctx(scenario));

        ts::next_tx(scenario, ADDR_TWO);

        let (kiosk, kiosk_cap) = kiosk::new(ctx(scenario));

        // take owned mail object
        let mail_obj = ts::take_from_address<Mail<DemoItem>>(scenario, ADDR_TWO);

        // claim the item out of it
        mail::claim_direct(mail_obj, &mut kiosk, &kiosk_cap, &policy, ctx(scenario));

        // check that the item is now in the kiosk
        assert!(kiosk::has_item(&kiosk, item_id), 0);
        assert!(kiosk::is_locked(&kiosk, item_id), 0);

        wrapup_policy(policy, cap, ctx(scenario));
        wrapup_kiosk(kiosk, kiosk_cap);

        ts::end(scenario_val);
    }

    #[test]
    fun send_and_return(){
        let scenario_val = init_test();
        let scenario = &mut scenario_val;

        let (policy, cap) = prepare_policy<DemoItem>(ctx(scenario));

        ts::next_tx(scenario, ADDR_ONE);
        let item = new_item(scenario);
        let item_id = object::id(&item);
        mail::send(item, &policy, false, ADDR_TWO, ctx(scenario));


        ts::next_tx(scenario, ADDR_TWO);
        // take owned mail object
        let mail_obj = ts::take_from_address<Mail<DemoItem>>(scenario, ADDR_TWO);
        // return it to the sender
        mail::return_to_sender(mail_obj,ctx(scenario));

        // check that the item is now owned by the sender!
        ts::next_tx(scenario, ADDR_ONE);
        assert!(ts::has_most_recent_for_address<DemoItem>(ADDR_ONE), 0);
        assert!(option::destroy_some(ts::most_recent_id_for_address<DemoItem>(ADDR_ONE)) == item_id, 0);

        wrapup_policy(policy, cap, ctx(scenario));

        ts::end(scenario_val);
    }

    #[test, expected_failure(abort_code=kiosk_mail::mail::ENotPersonalKiosk)]
    fun send_and_require_personal_kiosk_failure(){
        let scenario_val = init_test();
        let scenario = &mut scenario_val;

        let (policy, _cap) = prepare_policy<DemoItem>(ctx(scenario));

        ts::next_tx(scenario, ADDR_ONE);
        let item = new_item(scenario);
        mail::send(item, &policy, true, ADDR_TWO, ctx(scenario));

        ts::next_tx(scenario, ADDR_TWO);

        let (kiosk, kiosk_cap) = kiosk::new(ctx(scenario));

        let mail_obj = ts::take_from_address<Mail<DemoItem>>(scenario, ADDR_TWO);
        // claim the item out of it
        mail::claim_direct(mail_obj, &mut kiosk, &kiosk_cap, &policy, ctx(scenario));

        abort 1337
    }


    fun new_item(scenario: &mut Scenario): DemoItem {
        DemoItem {
            id: object::new(ctx(scenario))
        }
    }
    
    fun init_test(): Scenario {
        let scenario_val = ts::begin(@0x0);
        let scenario = &mut scenario_val;

        ts::next_tx(scenario, @0x0);
        mail::init_for_testing(OTW {}, ctx(scenario));

        scenario_val
    }

    fun wrapup_kiosk(kiosk: Kiosk, cap: KioskOwnerCap) {
        sui::transfer::public_transfer(kiosk, @0x0);
        sui::transfer::public_transfer(cap, @0x0);
    }

    /// === TODO:  WE MUST(!!) ADD A `TRANSFERPOLICY<T> TEST FUNCTION ON FRAMEWORK`
    fun prepare_policy<T>(ctx: &mut TxContext): (TransferPolicy<T>, TransferPolicyCap<T>) {
        let publisher = package::test_claim(OTW {}, ctx);
        let (policy, cap) = policy::new<T>(&publisher, ctx);
        package::burn_publisher(publisher);

        (policy, cap)
    }

    fun wrapup_policy<T>(policy: TransferPolicy<T>, cap: TransferPolicyCap<T>, ctx: &mut TxContext): u64 {
        let profits = policy::destroy_and_withdraw(policy, cap, ctx);
        coin::burn_for_testing(profits)
    }
}
