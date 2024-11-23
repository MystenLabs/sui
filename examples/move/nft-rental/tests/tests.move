// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#[test_only]
module nft_rental::tests;

use kiosk::kiosk_lock_rule as lock_rule;
use nft_rental::rentables_ext::{Self, Promise, ProtectedTP, RentalPolicy, Listed};
use sui::{
    clock::{Self, Clock},
    kiosk::{Kiosk, KioskOwnerCap},
    kiosk_test_utils,
    package::{Self, Publisher},
    test_scenario::{Self as ts, Scenario},
    transfer_policy::{Self, TransferPolicy, TransferPolicyCap}
};

const CREATOR: address = @0xCCCC;
const RENTER: address = @0xAAAA;
const BORROWER: address = @0xBBBB;
const THIEF: address = @0xDDDD;

public struct T has key, store { id: UID }
public struct WITNESS has drop {}

// ==================== Tests ====================
#[test]
fun test_install_extension() {
    let mut ts = ts::begin(RENTER);
    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());

    ts.install_ext(RENTER, renter_kiosk_id);
    ts.end();
}

#[test]
fun test_remove_extension() {
    let mut ts = ts::begin(RENTER);

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());

    ts.install_ext(RENTER, renter_kiosk_id);
    ts.remove_ext(RENTER, renter_kiosk_id);
    ts.end();
}

#[test]
fun test_setup_renting() {
    let mut ts = ts::begin(RENTER);

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    ts.setup(RENTER, &publisher, 50);

    {
        ts.next_tx(RENTER);
        let protected_tp: ProtectedTP<T> = ts.take_shared();
        ts::return_shared(protected_tp);
    };

    publisher.burn_publisher();
    ts.end();
}

#[test]
fun test_list_with_extension() {
    let mut ts = ts::begin(RENTER);
    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);

    publisher.burn_publisher();
    ts.end();
}

#[test]
#[expected_failure(abort_code = rentables_ext::EExtensionNotInstalled)]
fun test_list_without_extension() {
    let mut ts = ts::begin(RENTER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    abort 0xbad
}

#[test]
#[expected_failure(abort_code = 0x2::kiosk::ENotOwner)]
fun test_list_with_wrong_cap() {
    let mut ts = ts::begin(RENTER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let _borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);

    ts.next_tx(RENTER);
    let mut kiosk: Kiosk = ts.take_shared_by_id(renter_kiosk_id);
    let kiosk_cap: KioskOwnerCap = ts.take_from_address(BORROWER);
    let protected_tp: ProtectedTP<T> = ts.take_shared();

    rentables_ext::list(&mut kiosk, &kiosk_cap, &protected_tp, item_id, 10, 10, ts.ctx());
    abort 0xbad
}

#[test]
fun test_delist_locked() {
    let mut ts = ts::begin(RENTER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let witness = WITNESS {};

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());

    let publisher = package::test_claim(witness, ts.ctx());
    create_transfer_policy(CREATOR, &publisher, ts.ctx());

    ts.add_lock_rule(CREATOR);
    ts.setup(RENTER, &publisher, 50);
    ts.lock_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.delist_from_rent(RENTER, renter_kiosk_id, item_id);

    publisher.burn_publisher();
    ts.end();
}

#[test]
fun test_delist_placed() {
    let mut ts = ts::begin(RENTER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());

    create_transfer_policy(CREATOR, &publisher, ts.ctx());
    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.delist_from_rent(RENTER, renter_kiosk_id, item_id);

    publisher.burn_publisher();
    ts.end();
}

#[test]
#[expected_failure(abort_code = rentables_ext::EObjectNotExist)]
fun test_delist_rented() {
    let mut ts = ts::begin(RENTER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    create_transfer_policy(CREATOR, &publisher, ts.ctx());
    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);
    ts.delist_from_rent(BORROWER, borrower_kiosk_id, item_id);
    abort 0xbad
}

#[test]
#[expected_failure(abort_code = rentables_ext::ENotOwner)]
fun test_delist_with_wrong_cap() {
    let mut ts = ts::begin(RENTER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let _borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    create_transfer_policy(CREATOR, &publisher, ts.ctx());
    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);

    ts.next_tx(RENTER);
    let mut kiosk: Kiosk = ts.take_shared_by_id(renter_kiosk_id);
    let kiosk_cap: KioskOwnerCap = ts.take_from_address(BORROWER);
    let transfer_policy: TransferPolicy<T> = ts.take_shared();

    rentables_ext::delist<T>(&mut kiosk, &kiosk_cap, &transfer_policy, item_id, ts.ctx());
    abort 0xbad
}

#[test]
fun test_rent_with_extension() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

    clock.destroy_for_testing();
    publisher.burn_publisher();
    ts.end();
}

#[test]
#[expected_failure(abort_code = rentables_ext::EExtensionNotInstalled)]
fun test_rent_without_extension() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);
    abort 0xbad
}

#[test]
#[expected_failure(abort_code = rentables_ext::ENotEnoughCoins)]
fun test_rent_with_not_enough_coins() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 10, &clock);
    abort 0xbad
}

#[test]
#[expected_failure(abort_code = rentables_ext::ETotalPriceOverflow)]
fun test_rent_with_overflow() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 100, 1844674407370955160);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);
    abort 0xbad
}

#[test]
fun test_borrow() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);

    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);
    ts.borrow(BORROWER, borrower_kiosk_id, item_id);

    clock.destroy_for_testing();
    publisher.burn_publisher();
    ts.end();
}

#[test]
#[expected_failure(abort_code = rentables_ext::ENotOwner)]
fun test_borrow_with_wrong_cap() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

    ts.next_tx(BORROWER);
    let mut kiosk: Kiosk = ts.take_shared_by_id(borrower_kiosk_id);
    let kiosk_cap: KioskOwnerCap = ts.take_from_address(RENTER);

    let _object = rentables_ext::borrow<T>(&mut kiosk, &kiosk_cap, item_id, ts.ctx());
    abort 0xbad
}

#[test]
fun test_borrow_val() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

    let promise = ts.borrow_val(BORROWER, borrower_kiosk_id, item_id);
    ts.return_val(promise, BORROWER, borrower_kiosk_id);

    clock.destroy_for_testing();
    publisher.burn_publisher();
    ts.end();
}

#[test]
#[expected_failure(abort_code = rentables_ext::ENotOwner)]
fun test_borrow_val_with_wrong_cap() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

    ts.next_tx(BORROWER);
    let mut kiosk: Kiosk = ts.take_shared_by_id(borrower_kiosk_id);
    let kiosk_cap: KioskOwnerCap = ts.take_from_address(RENTER);

    let (_object, _promise) = rentables_ext::borrow_val<T>(
        &mut kiosk,
        &kiosk_cap,
        item_id,
        ts.ctx(),
    );

    abort 0xbad
}

#[test]
fun test_return_val() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

    let promise = ts.borrow_val(BORROWER, borrower_kiosk_id, item_id);

    ts.return_val(promise, BORROWER, borrower_kiosk_id);

    clock.destroy_for_testing();
    publisher.burn_publisher();
    ts.end();
}

#[test]
#[expected_failure(abort_code = rentables_ext::EExtensionNotInstalled)]
fun test_return_val_without_extension() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

    let promise = ts.borrow_val(BORROWER, borrower_kiosk_id, item_id);
    ts.remove_ext(BORROWER, borrower_kiosk_id);
    ts.return_val(promise, BORROWER, borrower_kiosk_id);
    abort 0xbad
}

#[test]
#[expected_failure(abort_code = rentables_ext::EInvalidKiosk)]
fun test_return_val_wrong_kiosk() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);

    let promise = ts.borrow_val(BORROWER, borrower_kiosk_id, item_id);
    ts.return_val(promise, BORROWER, renter_kiosk_id);
    abort 0xbad
}

#[test]
fun test_reclaim() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let mut clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    create_transfer_policy(CREATOR, &publisher, ts.ctx());
    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);
    ts.reclaim(RENTER, renter_kiosk_id, borrower_kiosk_id, item_id, 432000000, &mut clock);

    clock.destroy_for_testing();
    publisher.burn_publisher();
    ts.end();
}

#[test]
fun test_reclaim_locked() {
    let mut ts = ts::begin(RENTER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let mut clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    create_transfer_policy(CREATOR, &publisher, ts.ctx());
    ts.add_lock_rule(CREATOR);
    ts.setup(RENTER, &publisher, 50);
    ts.lock_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);
    ts.reclaim(RENTER, renter_kiosk_id, borrower_kiosk_id, item_id, 432000000, &mut clock);

    clock.destroy_for_testing();
    publisher.burn_publisher();
    ts.end();
}

#[test]
#[expected_failure(abort_code = rentables_ext::EInvalidKiosk)]
fun test_reclaim_wrong_kiosk() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let mut clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());
    let thief_kiosk_id = create_kiosk(THIEF, ts.ctx());

    create_transfer_policy(CREATOR, &publisher, ts.ctx());
    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);
    ts.install_ext(THIEF, thief_kiosk_id);
    ts.reclaim(RENTER, thief_kiosk_id, borrower_kiosk_id, item_id, 432000000, &mut clock);
    abort 0xbad
}

#[test]
#[expected_failure(abort_code = rentables_ext::ERentingPeriodNotOver)]
fun test_reclaim_renting_period_not_over() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let mut clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    create_transfer_policy(CREATOR, &publisher, ts.ctx());
    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);
    ts.reclaim(RENTER, renter_kiosk_id, borrower_kiosk_id, item_id, 20000, &mut clock);
    abort 0xbad
}

#[test]
#[expected_failure(abort_code = rentables_ext::EExtensionNotInstalled)]
fun test_reclaim_without_extension() {
    let mut ts = ts::begin(BORROWER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);

    let mut clock = clock::create_for_testing(ts.ctx());

    let witness = WITNESS {};
    let publisher = package::test_claim(witness, ts.ctx());

    let renter_kiosk_id = create_kiosk(RENTER, ts.ctx());
    let borrower_kiosk_id = create_kiosk(BORROWER, ts.ctx());

    create_transfer_policy(CREATOR, &publisher, ts.ctx());
    ts.setup(RENTER, &publisher, 50);
    ts.place_in_kiosk(RENTER, renter_kiosk_id, item);
    ts.install_ext(RENTER, renter_kiosk_id);
    ts.list_for_rent(RENTER, renter_kiosk_id, item_id, 10, 10);
    ts.install_ext(BORROWER, borrower_kiosk_id);
    ts.rent(BORROWER, renter_kiosk_id, borrower_kiosk_id, item_id, 100, &clock);
    ts.remove_ext(RENTER, renter_kiosk_id);
    ts.reclaim(RENTER, renter_kiosk_id, borrower_kiosk_id, item_id, 432000000, &mut clock);
    abort 0xbad
}

#[test]
#[expected_failure(abort_code = rentables_ext::EObjectNotExist)]
fun test_take_non_existed_item() {
    let mut ts = ts::begin(RENTER);

    let item = T { id: object::new(ts.ctx()) };
    let item_id = object::id(&item);
    transfer::public_transfer(item, RENTER);

    let kiosk_id = create_kiosk(RENTER, ts.ctx());

    ts.install_ext(RENTER, kiosk_id);

    ts.next_tx(RENTER);
    let mut kiosk: Kiosk = ts.take_shared_by_id(kiosk_id);
    let listed = rentables_ext::create_listed(item_id);
    rentables_ext::test_take_from_bag<T, Listed>(&mut kiosk, listed);
    abort 0xbad
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
    transfer::public_share_object(transfer_policy);
    transfer::public_transfer(policy_cap, sender);
}

use fun install_ext as Scenario.install_ext;

fun install_ext(ts: &mut Scenario, sender: address, kiosk_id: ID) {
    ts.next_tx(sender);
    let mut kiosk: Kiosk = ts.take_shared_by_id(kiosk_id);
    let kiosk_cap = ts.take_from_sender();

    rentables_ext::install(&mut kiosk, &kiosk_cap, ts.ctx());

    ts::return_shared(kiosk);
    ts.return_to_sender(kiosk_cap);
}

use fun remove_ext as Scenario.remove_ext;

fun remove_ext(ts: &mut Scenario, sender: address, kiosk_id: ID) {
    ts.next_tx(sender);
    let mut kiosk: Kiosk = ts.take_shared_by_id(kiosk_id);
    let kiosk_cap: KioskOwnerCap = ts.take_from_sender();

    rentables_ext::remove(&mut kiosk, &kiosk_cap, ts.ctx());

    ts::return_shared(kiosk);
    ts.return_to_sender(kiosk_cap);
}

use fun setup as Scenario.setup;

fun setup(ts: &mut Scenario, sender: address, publisher: &Publisher, amount_bp: u64) {
    ts.next_tx(sender);
    rentables_ext::setup_renting<T>(publisher, amount_bp, ts.ctx());
}

use fun lock_in_kiosk as Scenario.lock_in_kiosk;

fun lock_in_kiosk(ts: &mut Scenario, sender: address, kiosk_id: ID, item: T) {
    ts.next_tx(sender);

    let mut kiosk: Kiosk = ts.take_shared_by_id(kiosk_id);
    let kiosk_cap: KioskOwnerCap = ts.take_from_sender();
    let transfer_policy: TransferPolicy<T> = ts.take_shared();

    kiosk.lock(&kiosk_cap, &transfer_policy, item);

    ts::return_shared(kiosk);
    ts.return_to_sender(kiosk_cap);
    ts::return_shared(transfer_policy);
}

use fun place_in_kiosk as Scenario.place_in_kiosk;

fun place_in_kiosk(ts: &mut Scenario, sender: address, kiosk_id: ID, item: T) {
    ts.next_tx(sender);
    let mut kiosk: Kiosk = ts.take_shared_by_id(kiosk_id);
    let kiosk_cap: KioskOwnerCap = ts.take_from_sender();

    kiosk.place(&kiosk_cap, item);

    ts::return_shared(kiosk);
    ts.return_to_sender(kiosk_cap);
}

use fun list_for_rent as Scenario.list_for_rent;

fun list_for_rent(
    ts: &mut Scenario,
    sender: address,
    kiosk_id: ID,
    item_id: ID,
    duration: u64,
    price: u64,
) {
    ts.next_tx(sender);
    let mut kiosk: Kiosk = ts.take_shared_by_id(kiosk_id);
    let kiosk_cap: KioskOwnerCap = ts.take_from_sender();
    let protected_tp: ProtectedTP<T> = ts.take_shared();

    rentables_ext::list(
        &mut kiosk,
        &kiosk_cap,
        &protected_tp,
        item_id,
        duration,
        price,
        ts.ctx(),
    );

    ts::return_shared(kiosk);
    ts.return_to_sender(kiosk_cap);
    ts::return_shared(protected_tp);
}

use fun delist_from_rent as Scenario.delist_from_rent;

fun delist_from_rent(ts: &mut Scenario, sender: address, kiosk_id: ID, item_id: ID) {
    ts.next_tx(sender);
    let mut kiosk: Kiosk = ts.take_shared_by_id(kiosk_id);
    let kiosk_cap: KioskOwnerCap = ts.take_from_sender();
    let transfer_policy: TransferPolicy<T> = ts.take_shared();

    rentables_ext::delist<T>(&mut kiosk, &kiosk_cap, &transfer_policy, item_id, ts.ctx());

    ts::return_shared(kiosk);
    ts.return_to_sender(kiosk_cap);
    ts::return_shared(transfer_policy);
}

use fun rent as Scenario.rent;

fun rent(
    ts: &mut Scenario,
    sender: address,
    renter_kiosk_id: ID,
    borrower_kiosk_id: ID,
    item_id: ID,
    coin_amount: u64,
    clock: &Clock,
) {
    ts.next_tx(sender);

    let mut borrower_kiosk: Kiosk = ts.take_shared_by_id(borrower_kiosk_id);
    let mut renter_kiosk: Kiosk = ts.take_shared_by_id(renter_kiosk_id);
    let mut rental_policy: RentalPolicy<T> = ts.take_shared();

    let coin = kiosk_test_utils::get_sui(coin_amount, ts.ctx());

    rentables_ext::rent<T>(
        &mut renter_kiosk,
        &mut borrower_kiosk,
        &mut rental_policy,
        item_id,
        coin,
        clock,
        ts.ctx(),
    );

    ts::return_shared(borrower_kiosk);
    ts::return_shared(renter_kiosk);
    ts::return_shared(rental_policy);
}

use fun borrow as Scenario.borrow;

fun borrow(ts: &mut Scenario, sender: address, kiosk_id: ID, item_id: ID) {
    ts.next_tx(sender);
    let mut kiosk: Kiosk = ts.take_shared_by_id(kiosk_id);
    let kiosk_cap: KioskOwnerCap = ts.take_from_sender();

    let _object = rentables_ext::borrow<T>(&mut kiosk, &kiosk_cap, item_id, ts.ctx());

    ts::return_shared(kiosk);
    ts.return_to_sender(kiosk_cap);
}

use fun borrow_val as Scenario.borrow_val;

fun borrow_val(ts: &mut Scenario, sender: address, kiosk_id: ID, item_id: ID): Promise {
    ts.next_tx(sender);
    let mut kiosk: Kiosk = ts.take_shared_by_id(kiosk_id);
    let kiosk_cap: KioskOwnerCap = ts.take_from_sender();

    let (object, promise) = rentables_ext::borrow_val<T>(
        &mut kiosk,
        &kiosk_cap,
        item_id,
        ts.ctx(),
    );

    transfer::public_transfer(object, sender);
    ts::return_shared(kiosk);
    ts.return_to_sender(kiosk_cap);
    promise
}

use fun return_val as Scenario.return_val;

fun return_val(ts: &mut Scenario, promise: Promise, sender: address, kiosk_id: ID) {
    ts.next_tx(sender);
    let mut kiosk: Kiosk = ts.take_shared_by_id(kiosk_id);
    let object: T = ts.take_from_sender();

    rentables_ext::return_val(&mut kiosk, object, promise, ts.ctx());
    ts::return_shared(kiosk);
}

use fun reclaim as Scenario.reclaim;

fun reclaim(
    ts: &mut Scenario,
    sender: address,
    renter_kiosk_id: ID,
    borrower_kiosk_id: ID,
    item_id: ID,
    tick: u64,
    clock: &mut Clock,
) {
    ts.next_tx(sender);
    let mut borrower_kiosk: Kiosk = ts.take_shared_by_id(borrower_kiosk_id);
    let mut renter_kiosk: Kiosk = ts.take_shared_by_id(renter_kiosk_id);
    let policy: TransferPolicy<T> = ts.take_shared();

    clock.increment_for_testing(tick);
    rentables_ext::reclaim(
        &mut renter_kiosk,
        &mut borrower_kiosk,
        &policy,
        clock,
        item_id,
        ts.ctx(),
    );

    ts::return_shared(policy);
    ts::return_shared(borrower_kiosk);
    ts::return_shared(renter_kiosk);
}

use fun add_lock_rule as Scenario.add_lock_rule;

fun add_lock_rule(ts: &mut Scenario, sender: address) {
    ts.next_tx(sender);
    let mut transfer_policy: TransferPolicy<T> = ts.take_shared();
    let policy_cap: TransferPolicyCap<T> = ts.take_from_sender();

    lock_rule::add(&mut transfer_policy, &policy_cap);

    ts::return_shared(transfer_policy);
    ts.return_to_sender(policy_cap);
}
