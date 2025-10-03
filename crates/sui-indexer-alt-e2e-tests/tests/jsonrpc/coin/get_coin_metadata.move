// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses Test=0x0 --accounts A B --simulator --objects-snapshot-min-checkpoint-lag 2

//# publish --sender A
#[allow(deprecated_usage)]
module Test::fake {
    use sui::coin;
    use sui::url;

    public struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext){
        let (treasury_cap, metadata) = coin::create_currency(
            witness,
            8,
            b"FAKE",
            b"Fake Coin",
            b"A coin that is fake",
            option::some(url::new_unsafe_from_bytes(b"https://fake.com")),
            ctx,
        );

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
    }
}

#[allow(deprecated_usage)]
module Test::real {
    use sui::coin;
    use sui::url;

    public struct REAL has drop {}

    public struct Wrapper has key, store {
        id: UID,
        coin_metadata: coin::CoinMetadata<REAL>,
    }

    fun init(witness: REAL, ctx: &mut TxContext){
        let (treasury_cap, metadata) = coin::create_currency(
            witness,
            // tbh it's crazy that this much decimal is allowed
            255,
            b"REAL",
            b"Real Coin",
            b"A coin that is real",
            option::some(url::new_unsafe_from_bytes(b"https://real.com")),
            ctx,
        );

        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
        transfer::public_transfer(metadata, tx_context::sender(ctx));
    }

    entry fun wrap_coin_metadata(metadata: coin::CoinMetadata<REAL>, ctx: &mut TxContext) {
        let wrapper = Wrapper {
            id: object::new(ctx),
            coin_metadata: metadata,
        };

        transfer::public_transfer(wrapper, tx_context::sender(ctx));
    }

    entry fun update_metadata_name(treasury: &mut coin::TreasuryCap<REAL>, metadata: &mut coin::CoinMetadata<REAL>) {
        coin::update_name(treasury, metadata, std::string::utf8(b"New Real Name"));
    }
}

//# view-object 1,1

//# view-object 1,2

//# view-object 1,4

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getCoinMetadata",
  "params": ["@{Test}::real::REAL"]
}

//# run-jsonrpc
{
  "method": "suix_getCoinMetadata",
  "params": ["@{Test}::fake::FAKE"]
}

//# run-jsonrpc
{
  "method": "suix_getCoinMetadata",
  "params": ["@{Test}::fake::NonExistent"]
}

//# run-jsonrpc
{
  "method": "suix_getCoinMetadata",
  "params": ["invalid_coin_type"]
}

//# programmable --sender A --inputs object(1,4) object(1,2)
//> 0: Test::real::update_metadata_name(Input(0),Input(1));

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getCoinMetadata",
  "params": ["@{Test}::real::REAL"]
}

//# transfer-object 1,2 --sender A --recipient B

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getCoinMetadata",
  "params": ["@{Test}::real::REAL"]
}

//# programmable --sender B --inputs object(1,2)
//> 0: Test::real::wrap_coin_metadata(Input(0));

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getCoinMetadata",
  "params": ["@{Test}::real::REAL"]
}
