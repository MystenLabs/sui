// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Address-balance withdrawals enter the adapter as Balance<T>. For compatibility with a function
// that expects Coin<T>, the adapter may convert the withdrawal into a Coin<T> input. The package
// that defines T must still participate in minversion linkage during that conversion.
//
// Here T comes from CoinV1. Selecting CoinV2 adds its LeafV2 dependency to the transaction:
//
//   withdrawn Coin<CoinV1::COIN_TYPE> -> selected CoinV2 -> LeafV2
//   Root                                                 -> LeafV1
//
// Root's public dependency can move forward, so the call must use LeafV2. This verifies the special
// withdrawal-compatibility input path rather than the ordinary object- and type-input paths.

//# init --addresses LeafV1=0x0 LeafV2=0x0 CoinV1=0x0 CoinV2=0x0 Root=0x0 --accounts A

//# publish --upgradeable --sender A
module LeafV1::leaf;
public fun value(): u64 { 1 }

//# upgrade --package LeafV1 --upgrade-capability 1,1 --sender A
module LeafV2::leaf;
public fun value(): u64 { 2 }

//# publish --upgradeable --dependencies LeafV1 --sender A
#[allow(deprecated_usage)]
module CoinV1::coin_type;
use LeafV1::leaf;
public struct COIN_TYPE has drop {}
fun init(otw: COIN_TYPE, ctx: &mut sui::tx_context::TxContext) {
    assert!(leaf::value() == 1);
    let (cap, metadata) = sui::coin::create_currency(
        otw, 0, b"T", b"Token", b"", std::option::none(), ctx,
    );
    sui::transfer::public_freeze_object(metadata);
    sui::transfer::public_transfer(cap, ctx.sender())
}

// Seed an address balance for the compatibility withdrawal.
//# programmable --sender A --inputs object(3,1) 10 @A
//> 0: sui::coin::mint<CoinV1::coin_type::COIN_TYPE>(Input(0), Input(1));
//> sui::coin::send_funds<CoinV1::coin_type::COIN_TYPE>(Result(0), Input(2));

//# create-checkpoint

//# upgrade --package CoinV1 --upgrade-capability 3,3 --dependencies LeafV2 --sender A
#[allow(deprecated_usage)]
module CoinV2::coin_type;
use LeafV2::leaf;
public struct COIN_TYPE has drop {}
fun init(otw: COIN_TYPE, ctx: &mut sui::tx_context::TxContext) {
    assert!(leaf::value() == 2);
    let (cap, metadata) = sui::coin::create_currency(
        otw, 0, b"T", b"Token", b"", std::option::none(), ctx,
    );
    sui::transfer::public_freeze_object(metadata);
    sui::transfer::public_transfer(cap, ctx.sender())
}

//# publish --dependencies LeafV1 --sender A
module Root::root;
use LeafV1::leaf;
public struct Result has key, store { id: sui::object::UID, value: u64 }
public fun make(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(Result { id: sui::object::new(ctx), value: 0 })
}
public fun call(result: &mut Result) { result.value = leaf::value() }

//# run Root::root::make --sender A

//# programmable --sender A --inputs object(3,3) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// The withdrawal is converted to Coin<COIN_TYPE>. Selecting CoinV2 for COIN_TYPE lifts Root to LeafV2.
//# programmable --sender A --inputs object(8,0) coin_reservation<sui::balance::Balance<CoinV1::coin_type::COIN_TYPE>>(1) @A
//> 0: Root::root::call(Input(0));
//> sui::coin::send_funds<CoinV1::coin_type::COIN_TYPE>(Input(1), Input(2));

//# view-object 8,0
