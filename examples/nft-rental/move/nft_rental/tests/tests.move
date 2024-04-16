// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#[test_only]
module nft_rental::tests {
    // sui imports
    use sui::test_scenario::{Self, Scenario};
    use sui::object::{Self, UID, ID};
    use sui::transfer_policy::{Self, TransferPolicy, TransferPolicyCap};
    use sui::package::{Self, Publisher};
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::kiosk_test_utils;
    use sui::transfer;
    use sui::clock::{Self, Clock};
    use sui::tx_context::{TxContext, dummy};

    // other imports 
    use nft_rental::rentables_ext::{Self, Promise, ProtectedTP, RentalPolicy, Listed};
    use kiosk::kiosk_lock_rule::{Self as lock_rule};
    
    const CREATOR: address = @0xCCCC;
    const RENTER: address = @0xAAAA;
    const BORROWER: address = @0xBBBB;
    const THIEF: address = @0xDDDD;

    struct T has key, store {id: UID}
    struct WITNESS has drop {}

    // ==================== Tests ====================
    #[test]
    fun test_install_extension() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;

        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));

        install_ext(test, RENTER, renter_kiosk_id);

        test_scenario::end(scenario);
    }

    #[test]
    fun test_remove_extension() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;

        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));

        install_ext(test, RENTER, renter_kiosk_id);

        remove_ext(test, RENTER, renter_kiosk_id);

        test_scenario::end(scenario);
    }

    #[test]
    fun test_setup_renting() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;
        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());

        setup(test, RENTER, &publisher, 50);

        test_scenario::next_tx(test, RENTER);
        {
            let protected_tp = test_scenario::take_shared<ProtectedTP<T>>(test);
            test_scenario::return_shared<ProtectedTP<T>>(protected_tp);
        };

        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_list_with_extension() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());

        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::EExtensionNotInstalled)]
    fun test_list_without_extension() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());

        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=0x2::kiosk::ENotOwner)]
    fun test_list_with_wrong_cap() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());

        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let _borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        test_scenario::next_tx(test, RENTER);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(test, renter_kiosk_id);
            let kiosk_cap = test_scenario::take_from_address<KioskOwnerCap>(test, BORROWER);
            let protected_tp = test_scenario::take_shared<ProtectedTP<T>>(test);

            rentables_ext::list(&mut kiosk, &kiosk_cap, &protected_tp, item_id, 10, 10, test_scenario::ctx(test));

            test_scenario::return_shared(kiosk);
            test_scenario::return_to_sender(test, kiosk_cap);
            test_scenario::return_shared<ProtectedTP<T>>(protected_tp);
        };
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_delist_locked() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);
        let witness = WITNESS {};

        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));

        let publisher = package::test_claim(witness, &mut dummy());
        create_transfer_policy(CREATOR, &publisher, test_scenario::ctx(test));
        add_lock_rule(test, CREATOR);
        
        setup(test, RENTER, &publisher, 50);

        lock_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        delist_from_rent(test, RENTER, renter_kiosk_id, item_id);

        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_delist_placed() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);
        let witness = WITNESS {};

        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));

        let publisher = package::test_claim(witness, &mut dummy());
        create_transfer_policy(CREATOR, &publisher, test_scenario::ctx(test));
    
        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        delist_from_rent(test, RENTER, renter_kiosk_id, item_id);

        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::EObjectNotExist)]
    fun test_delist_rented() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};

        let publisher = package::test_claim(witness, &mut dummy());
        create_transfer_policy(CREATOR, &publisher, test_scenario::ctx(test));

        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));
    
        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        delist_from_rent(test, BORROWER, borrower_kiosk_id, item_id);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::ENotOwner)]
    fun test_delist_with_wrong_cap() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);
        let witness = WITNESS {};

        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let _borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        let publisher = package::test_claim(witness, &mut dummy());
        create_transfer_policy(CREATOR, &publisher, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        test_scenario::next_tx(test, RENTER);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(test, renter_kiosk_id);
            let kiosk_cap = test_scenario::take_from_address<KioskOwnerCap>(test, BORROWER);
            let transfer_policy = test_scenario::take_shared<TransferPolicy<T>>(test);

            rentables_ext::delist<T>(&mut kiosk, &kiosk_cap, &transfer_policy, item_id, test_scenario::ctx(test));

            test_scenario::return_shared(kiosk);
            test_scenario::return_to_sender(test, kiosk_cap);
            test_scenario::return_shared(transfer_policy);

        };
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_rent_with_extension() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::EExtensionNotInstalled)]
    fun test_rent_without_extension() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::ENotEnoughCoins)]
    fun test_rent_with_not_enough_coins() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 10, &clock);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::ETotalPriceOverflow)]
    fun test_rent_with_overflow() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 100, 1844674407370955160);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_borrow() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);
        
        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock); 

        borrow(test, BORROWER, borrower_kiosk_id, item_id);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::ENotOwner)]
    fun test_borrow_with_wrong_cap() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        test_scenario::next_tx(test, BORROWER);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(test, borrower_kiosk_id);
            let kiosk_cap = test_scenario::take_from_address<KioskOwnerCap>(test, RENTER);

            let _object = rentables_ext::borrow<T>(&mut kiosk, &kiosk_cap, item_id, test_scenario::ctx(test));

            test_scenario::return_shared(kiosk);
            test_scenario::return_to_sender(test, kiosk_cap);
        };
        
        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_borrow_val() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        let promise = borrow_val(test, BORROWER, borrower_kiosk_id, item_id);
        return_val(test, promise, BORROWER, borrower_kiosk_id);
        
        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::ENotOwner)]
    fun test_borrow_val_with_wrong_cap() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        test_scenario::next_tx(test, BORROWER);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(test, borrower_kiosk_id);
            let kiosk_cap = test_scenario::take_from_address<KioskOwnerCap>(test, RENTER);

            let (object, promise) = rentables_ext::borrow_val<T>(&mut kiosk, &kiosk_cap, item_id, test_scenario::ctx(test));
            
            transfer::public_transfer(object, BORROWER);

            return_val(test, promise, BORROWER, borrower_kiosk_id);

            test_scenario::return_shared(kiosk);
            test_scenario::return_to_sender(test, kiosk_cap);
        };

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_return_val() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        let promise = borrow_val(test, BORROWER, borrower_kiosk_id, item_id);

        return_val(test, promise, BORROWER, borrower_kiosk_id);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::EExtensionNotInstalled)]
    fun test_return_val_without_extension() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        let promise = borrow_val(test, BORROWER, borrower_kiosk_id, item_id);

        remove_ext(test, BORROWER, borrower_kiosk_id);

        return_val(test, promise, BORROWER, borrower_kiosk_id);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::EInvalidKiosk)]
    fun test_return_val_wrong_kiosk() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        let promise = borrow_val(test, BORROWER, borrower_kiosk_id, item_id);

        return_val(test, promise, BORROWER, renter_kiosk_id);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_reclaim() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        create_transfer_policy(CREATOR, &publisher, test_scenario::ctx(test));
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        reclaim(test, RENTER, renter_kiosk_id, borrower_kiosk_id, item_id, 432000000, &mut clock);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_reclaim_locked() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);
        let witness = WITNESS {};

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        let publisher = package::test_claim(witness, &mut dummy());
        create_transfer_policy(CREATOR, &publisher, test_scenario::ctx(test));
        add_lock_rule(test, CREATOR);

        setup(test, RENTER, &publisher, 50);

        lock_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        reclaim(test, RENTER, renter_kiosk_id, borrower_kiosk_id, item_id, 432000000, &mut clock);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::EInvalidKiosk)]
    fun test_reclaim_wrong_kiosk() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        create_transfer_policy(CREATOR, &publisher, test_scenario::ctx(test));
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));
        let thief_kiosk_id = create_kiosk(THIEF, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        install_ext(test, THIEF, thief_kiosk_id);

        reclaim(test, RENTER, thief_kiosk_id, borrower_kiosk_id, item_id, 432000000, &mut clock);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::ERentingPeriodNotOver)]
    fun test_reclaim_renting_period_not_over() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        create_transfer_policy(CREATOR, &publisher, test_scenario::ctx(test));
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        reclaim(test, RENTER, renter_kiosk_id, borrower_kiosk_id, item_id, 20000, &mut clock);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::EExtensionNotInstalled)]
    fun test_reclaim_without_extension() {
        let scenario= test_scenario::begin(BORROWER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);

        let clock = clock::create_for_testing(test_scenario::ctx(test));

        let witness = WITNESS {};
        let publisher = package::test_claim(witness, &mut dummy());
        create_transfer_policy(CREATOR, &publisher, test_scenario::ctx(test));
        
        let renter_kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));
        let borrower_kiosk_id = create_kiosk(BORROWER, test_scenario::ctx(test));

        setup(test, RENTER, &publisher, 50);

        place_in_kiosk(test, RENTER, renter_kiosk_id, item);

        install_ext(test, RENTER, renter_kiosk_id);

        list_for_rent(test, RENTER, renter_kiosk_id, item_id, 10, 10);

        install_ext(test, BORROWER, borrower_kiosk_id);

        rent(test, BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

        remove_ext(test, RENTER, renter_kiosk_id);

        reclaim(test, RENTER, renter_kiosk_id, borrower_kiosk_id, item_id, 432000000, &mut clock);

        clock::destroy_for_testing(clock);
        package::burn_publisher(publisher);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code=rentables_ext::EObjectNotExist)]
    fun test_take_non_existed_item() {
        let scenario= test_scenario::begin(RENTER);
        let test = &mut scenario;
        let item = T {id: object::new(test_scenario::ctx(test))};
        let item_id = object::id(&item);
        transfer::public_transfer(item, RENTER);
        
        let kiosk_id = create_kiosk(RENTER, test_scenario::ctx(test));

        install_ext(test, RENTER, kiosk_id);

        test_scenario::next_tx(test, RENTER);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(test, kiosk_id);
            let listed = rentables_ext::create_listed(item_id);

            rentables_ext::test_take_from_bag<T, Listed>(&mut kiosk, listed);

            test_scenario::return_shared(kiosk);
        };
        test_scenario::end(scenario);
    }

    // ==================== Helper methods ====================
    fun create_kiosk(sender: address, ctx: &mut TxContext): ID {
        let (kiosk, kiosk_cap) = kiosk_test_utils::get_kiosk(ctx);
        let kiosk_id = object::id(&kiosk);
        transfer::public_share_object(kiosk);
        transfer::public_transfer(kiosk_cap, sender);

        kiosk_id
    }

    fun create_transfer_policy(sender: address, publisher: &Publisher, ctx: &mut TxContext) {
        let (transfer_policy, policy_cap) = transfer_policy::new<T>(publisher, ctx);
        // transfer::public_transfer(transfer_policy, sender);
        transfer::public_share_object(transfer_policy);
        transfer::public_transfer(policy_cap, sender);
    }

    fun install_ext(scenario: &mut Scenario, sender: address, kiosk_id: ID) {
        test_scenario::next_tx(scenario, sender);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, kiosk_id);
            let kiosk_cap = test_scenario::take_from_sender<KioskOwnerCap>(scenario);

            rentables_ext::install(&mut kiosk, &kiosk_cap, test_scenario::ctx(scenario));

            test_scenario::return_shared(kiosk);
            test_scenario::return_to_sender(scenario, kiosk_cap);
        };
    }

    fun remove_ext(scenario: &mut Scenario, sender: address, kiosk_id: ID) {
        test_scenario::next_tx(scenario, sender);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, kiosk_id);
            let kiosk_cap = test_scenario::take_from_sender<KioskOwnerCap>(scenario);

            rentables_ext::remove(&mut kiosk, &kiosk_cap, test_scenario::ctx(scenario));

            test_scenario::return_shared(kiosk);
            test_scenario::return_to_sender(scenario, kiosk_cap);
        };
    }

    fun setup(scenario: &mut Scenario, sender: address, publisher: &Publisher, amount_bp: u64) {
        test_scenario::next_tx(scenario, sender);
        {
            rentables_ext::setup_renting<T>(publisher, amount_bp, test_scenario::ctx(scenario)); 

        };
    }

    fun lock_in_kiosk(scenario: &mut Scenario, sender: address, kiosk_id: ID, item: T) {
        test_scenario::next_tx(scenario, sender);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, kiosk_id);
            let kiosk_cap = test_scenario::take_from_sender<KioskOwnerCap>(scenario);
            let transfer_policy = test_scenario::take_shared<TransferPolicy<T>>(scenario);

            kiosk::lock<T>(&mut kiosk, &kiosk_cap, &transfer_policy, item);

            test_scenario::return_shared(kiosk);
            test_scenario::return_to_sender(scenario, kiosk_cap);
            test_scenario::return_shared(transfer_policy);
        };
    }

    fun place_in_kiosk(scenario: &mut Scenario, sender: address, kiosk_id: ID, item: T) {
        test_scenario::next_tx(scenario, sender);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, kiosk_id);
            let kiosk_cap = test_scenario::take_from_sender<KioskOwnerCap>(scenario);

            kiosk::place<T>(&mut kiosk, &kiosk_cap, item);

            test_scenario::return_shared(kiosk);
            test_scenario::return_to_sender(scenario, kiosk_cap);
        };
    }

    fun list_for_rent(scenario: &mut Scenario, sender: address, kiosk_id: ID, item_id: ID, duration: u64, price: u64) {
        test_scenario::next_tx(scenario, sender);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, kiosk_id);
            let kiosk_cap = test_scenario::take_from_sender<KioskOwnerCap>(scenario);
            let protected_tp = test_scenario::take_shared<ProtectedTP<T>>(scenario);

            rentables_ext::list(&mut kiosk, &kiosk_cap, &protected_tp, item_id, duration, price, test_scenario::ctx(scenario));

            test_scenario::return_shared(kiosk);
            test_scenario::return_to_sender(scenario, kiosk_cap);
            test_scenario::return_shared<ProtectedTP<T>>(protected_tp);
        };
    }

    fun delist_from_rent(scenario: &mut Scenario, sender: address, kiosk_id: ID, item_id: ID) {
        test_scenario::next_tx(scenario, sender);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, kiosk_id);
            let kiosk_cap = test_scenario::take_from_sender<KioskOwnerCap>(scenario);
            let transfer_policy = test_scenario::take_shared<TransferPolicy<T>>(scenario);

            rentables_ext::delist<T>(&mut kiosk, &kiosk_cap, &transfer_policy, item_id, test_scenario::ctx(scenario));

            test_scenario::return_shared(kiosk);
            test_scenario::return_to_sender(scenario, kiosk_cap);
            test_scenario::return_shared(transfer_policy);
        };
    }

    fun rent(scenario: &mut Scenario, sender: address, renter_kiosk_id: ID, borrower_kiosk_id: ID, item_id: ID, coin_amount: u64, clock: &Clock) {
        test_scenario::next_tx(scenario, sender);
        {
            let borrower_kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, borrower_kiosk_id);
            let renter_kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, renter_kiosk_id);
            let rental_policy = test_scenario::take_shared<RentalPolicy<T>>(scenario);

            let coin = kiosk_test_utils::get_sui(coin_amount, test_scenario::ctx(scenario));

            rentables_ext::rent<T>(&mut renter_kiosk, &mut borrower_kiosk, &mut rental_policy, item_id, coin, clock, test_scenario::ctx(scenario));
            test_scenario::return_shared(borrower_kiosk);
            test_scenario::return_shared(renter_kiosk);
            test_scenario::return_shared<RentalPolicy<T>>(rental_policy);
        };
    }

    fun borrow(scenario: &mut Scenario, sender: address, kiosk_id: ID, item_id: ID) {
        test_scenario::next_tx(scenario, sender);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, kiosk_id);
            let kiosk_cap = test_scenario::take_from_sender<KioskOwnerCap>(scenario);

            let _object = rentables_ext::borrow<T>(&mut kiosk, &kiosk_cap, item_id, test_scenario::ctx(scenario));

            test_scenario::return_shared(kiosk);
            test_scenario::return_to_sender(scenario, kiosk_cap);
        };
    }

    fun borrow_val(scenario: &mut Scenario, sender: address, kiosk_id: ID, item_id: ID): Promise {
        test_scenario::next_tx(scenario, sender);
        let kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, kiosk_id);
        let kiosk_cap = test_scenario::take_from_sender<KioskOwnerCap>(scenario);

        let (object, promise) = rentables_ext::borrow_val<T>(&mut kiosk, &kiosk_cap, item_id, test_scenario::ctx(scenario));

        transfer::public_transfer(object, sender);
        test_scenario::return_shared(kiosk);
        test_scenario::return_to_sender(scenario, kiosk_cap);
        promise
    }

    fun return_val(scenario: &mut Scenario, promise: Promise, sender: address, kiosk_id: ID) {
        test_scenario::next_tx(scenario, sender);
        {
            let kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, kiosk_id);
            let object = test_scenario::take_from_sender<T>(scenario);
            
            rentables_ext::return_val(&mut kiosk, object, promise, test_scenario::ctx(scenario));
            test_scenario::return_shared(kiosk);
        };
    }

    fun reclaim(scenario: &mut Scenario, sender: address, renter_kiosk_id: ID, borrower_kiosk_id: ID, item_id: ID, tick: u64, clock: &mut Clock) {
        test_scenario::next_tx(scenario, sender);
        {
            let borrower_kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, borrower_kiosk_id);
            let renter_kiosk = test_scenario::take_shared_by_id<Kiosk>(scenario, renter_kiosk_id);
            let policy = test_scenario::take_shared<TransferPolicy<T>>(scenario);

            clock::increment_for_testing(clock, tick);

            rentables_ext::reclaim<T>(&mut renter_kiosk, &mut borrower_kiosk, &policy, clock, item_id, test_scenario::ctx(scenario));

            test_scenario::return_shared(policy);
            test_scenario::return_shared(borrower_kiosk);
            test_scenario::return_shared(renter_kiosk);
        };
    }

    fun add_lock_rule(scenario: &mut Scenario, sender: address) {
        test_scenario::next_tx(scenario, sender);
        {
            let transfer_policy = test_scenario::take_shared<TransferPolicy<T>>(scenario);
            let policy_cap = test_scenario::take_from_sender<TransferPolicyCap<T>>(scenario);

            lock_rule::add(&mut transfer_policy, &policy_cap);

            test_scenario::return_shared(transfer_policy);
            test_scenario::return_to_sender(scenario, policy_cap);
        };
    }
}