// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Executing a swap of two objects via a third party, using object wrapping to
/// hand ownership of the objects to swap to the third party without giving them
/// the ability to modify those objects.
module trusted_swap::example;

use sui::{balance::{Self, Balance}, coin::{Self, Coin}, sui::SUI};

public struct Object has key, store {
    id: UID,
    scarcity: u8,
    style: u8,
}

public struct SwapRequest has key {
    id: UID,
    owner: address,
    object: Object,
    fee: Balance<SUI>,
}

// === Errors ===

/// Fee is too low for the service
const EFeeTooLow: u64 = 0;

/// The two swap requests are not compatible
const EBadSwap: u64 = 1;

// === Constants ===

const MIN_FEE: u64 = 1000;

// === Public Functions ===

public fun new(scarcity: u8, style: u8, ctx: &mut TxContext): Object {
    Object { id: object::new(ctx), scarcity, style }
}

/// Anyone who owns an `Object` can make it available for swapping, which
/// sends a `SwapRequest` to a `service` responsible for matching swaps.
public fun request_swap(object: Object, fee: Coin<SUI>, service: address, ctx: &mut TxContext) {
    assert!(coin::value(&fee) >= MIN_FEE, EFeeTooLow);

    let request = SwapRequest {
        id: object::new(ctx),
        owner: tx_context::sender(ctx),
        object,
        fee: coin::into_balance(fee),
    };

    transfer::transfer(request, service)
}

/// When the service has two swap requests, it can execute them, sending the
/// objects to the respective owners and taking its fee.
public fun execute_swap(s1: SwapRequest, s2: SwapRequest): Balance<SUI> {
    let SwapRequest { id: id1, owner: owner1, object: o1, fee: mut fee1 } = s1;
    let SwapRequest { id: id2, owner: owner2, object: o2, fee: fee2 } = s2;

    assert!(o1.scarcity == o2.scarcity, EBadSwap);
    assert!(o1.style != o2.style, EBadSwap);

    // Perform the swap
    transfer::transfer(o1, owner2);
    transfer::transfer(o2, owner1);

    // Delete the wrappers
    object::delete(id1);
    object::delete(id2);

    // Take the fee and return it
    balance::join(&mut fee1, fee2);
    fee1
}

// === Tests ===
#[test_only]
use sui::test_scenario as ts;

#[test]
fun successful_swap() {
    let mut ts = ts::begin(@0x0);
    let alice = @0xA;
    let bob = @0xB;
    let custodian = @0xC;

    let i1 = {
        ts::next_tx(&mut ts, alice);
        let o1 = new(1, 0, ts::ctx(&mut ts));
        let c1 = coin::mint_for_testing<SUI>(MIN_FEE, ts::ctx(&mut ts));
        let i = object::id(&o1);
        request_swap(o1, c1, custodian, ts::ctx(&mut ts));
        i
    };

    let i2 = {
        ts::next_tx(&mut ts, bob);
        let o2 = new(1, 1, ts::ctx(&mut ts));
        let c2 = coin::mint_for_testing<SUI>(MIN_FEE, ts::ctx(&mut ts));
        let i = object::id(&o2);
        request_swap(o2, c2, custodian, ts::ctx(&mut ts));
        i
    };

    {
        ts::next_tx(&mut ts, custodian);
        let s1 = ts::take_from_sender<SwapRequest>(&ts);
        let s2 = ts::take_from_sender<SwapRequest>(&ts);

        let bal = execute_swap(s1, s2);
        let fee = coin::from_balance(bal, ts::ctx(&mut ts));

        transfer::public_transfer(fee, custodian);
    };

    {
        ts::next_tx(&mut ts, custodian);
        let fee: Coin<SUI> = ts::take_from_sender(&ts);

        assert!(ts::ids_for_address<Object>(alice) == vector[i2], 0);
        assert!(ts::ids_for_address<Object>(bob) == vector[i1], 0);
        assert!(coin::value(&fee) == MIN_FEE * 2, 0);

        ts::return_to_sender(&ts, fee);
    };

    ts::end(ts);
}

#[test]
#[expected_failure(abort_code = EFeeTooLow)]
fun swap_too_cheap() {
    let alice = @0xA;
    let custodian = @0xC;

    let mut ts = ts::begin(alice);
    let o1 = new(1, 0, ts::ctx(&mut ts));
    let c1 = coin::mint_for_testing<SUI>(MIN_FEE - 1, ts::ctx(&mut ts));
    request_swap(o1, c1, custodian, ts::ctx(&mut ts));

    abort 1337
}

#[test]
#[expected_failure(abort_code = EBadSwap)]
fun swap_different_scarcity() {
    let mut ts = ts::begin(@0x0);
    let alice = @0xA;
    let bob = @0xB;
    let custodian = @0xC;

    {
        ts::next_tx(&mut ts, alice);
        let o1 = new(1, 0, ts::ctx(&mut ts));
        let c1 = coin::mint_for_testing<SUI>(MIN_FEE, ts::ctx(&mut ts));
        request_swap(o1, c1, custodian, ts::ctx(&mut ts));
    };

    {
        ts::next_tx(&mut ts, bob);
        let o2 = new(0, 1, ts::ctx(&mut ts));
        let c2 = coin::mint_for_testing<SUI>(MIN_FEE, ts::ctx(&mut ts));
        request_swap(o2, c2, custodian, ts::ctx(&mut ts));
    };

    {
        ts::next_tx(&mut ts, custodian);
        let s1 = ts::take_from_sender<SwapRequest>(&ts);
        let s2 = ts::take_from_sender<SwapRequest>(&ts);
        let _fee = execute_swap(s1, s2);
    };

    abort 1337
}

#[test]
#[expected_failure(abort_code = EBadSwap)]
fun swap_same_style() {
    let mut ts = ts::begin(@0x0);
    let alice = @0xA;
    let bob = @0xB;
    let custodian = @0xC;

    {
        ts::next_tx(&mut ts, alice);
        let o1 = new(1, 0, ts::ctx(&mut ts));
        let c1 = coin::mint_for_testing<SUI>(MIN_FEE, ts::ctx(&mut ts));
        request_swap(o1, c1, custodian, ts::ctx(&mut ts));
    };

    {
        ts::next_tx(&mut ts, bob);
        let o2 = new(1, 0, ts::ctx(&mut ts));
        let c2 = coin::mint_for_testing<SUI>(MIN_FEE, ts::ctx(&mut ts));
        request_swap(o2, c2, custodian, ts::ctx(&mut ts));
    };

    {
        ts::next_tx(&mut ts, custodian);
        let s1 = ts::take_from_sender<SwapRequest>(&ts);
        let s2 = ts::take_from_sender<SwapRequest>(&ts);
        let _fee = execute_swap(s1, s2);
    };

    abort 1337
}
