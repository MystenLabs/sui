// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::object_tests;

use sui::address;
use sui::test_scenario;

const EDifferentAddress: u64 = 0xF000;
const EDifferentBytes: u64 = 0xF001;
const EAddressRoundTrip: u64 = 0xF002;

public struct Obj has key { id: UID }

#[test]
fun test_bytes_address_roundtrip() {
    let mut ctx = tx_context::dummy();

    let uid0 = object::new(&mut ctx);
    let uid1 = object::new(&mut ctx);

    let addr0 = uid0.to_address();
    let byte0 = uid0.to_bytes();
    let addr1 = uid1.to_address();
    let byte1 = uid1.to_bytes();

    assert!(addr0 != addr1, EDifferentAddress);
    assert!(byte0 != byte1, EDifferentBytes);

    assert!(addr0 == address::from_bytes(byte0), EAddressRoundTrip);
    assert!(addr1 == address::from_bytes(byte1), EAddressRoundTrip);

    uid0.delete();
    uid1.delete();
}

#[test]
fun test_is_shared_input() {
    let sender = @0xA;
    let mut scenario = test_scenario::begin(sender);
    transfer::share_object(Obj { id: object::new(scenario.ctx()) });
    transfer::transfer(Obj { id: object::new(scenario.ctx()) }, sender);
    transfer::freeze_object(Obj { id: object::new(scenario.ctx()) });

    scenario.next_tx(sender);
    let shared = scenario.take_shared<Obj>();
    let owned = scenario.take_from_sender<Obj>();
    let frozen = scenario.take_immutable<Obj>();
    let new = Obj { id: object::new(scenario.ctx()) };
    assert!(shared.id.is_shared_input());
    assert!(!owned.id.is_shared_input());
    assert!(!frozen.id.is_shared_input());
    assert!(!new.id.is_shared_input());

    test_scenario::return_shared(shared);
    scenario.return_to_sender(owned);
    test_scenario::return_immutable(frozen);
    let Obj { id } = new;
    id.delete();
    scenario.end();
}
