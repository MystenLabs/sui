// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A simple NFT that can be airdropped to users without a value and converted to a random metal NFT.
/// The probability of getting a gold, silver, or bronze NFT is 10%, 30%, and 60% respectively.
module random_nft::example;

use std::string;
use sui::{object::delete, random::{Random, new_generator}};

const EInvalidParams: u64 = 0;

const GOLD: u8 = 1;
const SILVER: u8 = 2;
const BRONZE: u8 = 3;

public struct AirDropNFT has key, store {
    id: UID,
}

public struct MetalNFT has key, store {
    id: UID,
    metal: u8,
}

public struct MintingCapability has key {
    id: UID,
}

#[allow(unused_function)]
fun init(ctx: &mut TxContext) {
    transfer::transfer(
        MintingCapability { id: object::new(ctx) },
        ctx.sender(),
    );
}

public fun mint(_cap: &MintingCapability, n: u16, ctx: &mut TxContext): vector<AirDropNFT> {
    let mut result = vector[];
    let mut i = 0;
    while (i < n) {
        result.push_back(AirDropNFT { id: object::new(ctx) });
        i = i + 1;
    };
    result
}

/// Reveal the metal of the airdrop NFT and convert it to a metal NFT.
/// This function uses arithmetic_is_less_than to determine the metal of the NFT in a way that consumes the same
/// amount of gas regardless of the value of the random number.
/// See reveal_alternative1 and reveal_alternative2_step1 for different implementations.
entry fun reveal(nft: AirDropNFT, r: &Random, ctx: &mut TxContext) {
    destroy_airdrop_nft(nft);

    let mut generator = new_generator(r, ctx);
    let v = generator.generate_u8_in_range(1, 100);

    let is_gold = arithmetic_is_less_than(v, 11, 100); // probability of 10%
    let is_silver = arithmetic_is_less_than(v, 41, 100) * (1 - is_gold); // probability of 30%
    let is_bronze = (1 - is_gold) * (1 - is_silver); // probability of 60%
    let metal = is_gold * GOLD + is_silver * SILVER + is_bronze * BRONZE;

    transfer::public_transfer(
        MetalNFT { id: object::new(ctx), metal },
        ctx.sender(),
    );
}

// Implements "is v < w? where v <= v_max" using integer arithmetic. Returns 1 if true, 0 otherwise.
// Safe in case w and v_max are independent of the randomenss (e.g., fixed).
// Does not check if v <= v_max.
fun arithmetic_is_less_than(v: u8, w: u8, v_max: u8): u8 {
    assert!(v_max >= w && w > 0, EInvalidParams);
    let v_max_over_w = v_max / w;
    let v_over_w = v / w; // 0 if v < w, [1, v_max_over_w] if above
    (v_max_over_w - v_over_w) / v_max_over_w
}

/// An alternative implementation of reveal that uses if-else statements to determine the metal of the NFT.
/// Here the "happier flows" consume more gas than the less happy ones (it assumes that users always prefer the
/// rarest metals).
entry fun reveal_alternative1(nft: AirDropNFT, r: &Random, ctx: &mut TxContext) {
    destroy_airdrop_nft(nft);

    let mut generator = new_generator(r, ctx);
    let v = generator.generate_u8_in_range(1, 100);

    if (v <= 60) {
        transfer::public_transfer(
            MetalNFT { id: object::new(ctx), metal: BRONZE },
            ctx.sender(),
        );
    } else if (v <= 90) {
        transfer::public_transfer(
            MetalNFT { id: object::new(ctx), metal: SILVER },
            ctx.sender(),
        );
    } else if (v <= 100) {
        transfer::public_transfer(
            MetalNFT { id: object::new(ctx), metal: GOLD },
            ctx.sender(),
        );
    };
}

/// An alternative implementation of reveal that uses two steps to determine the metal of the NFT.
/// reveal_alternative2_step1 retrieves the random value, and reveal_alternative2_step2 determines the metal.

public struct RandomnessNFT has key, store {
    id: UID,
    value: u8,
}

entry fun reveal_alternative2_step1(nft: AirDropNFT, r: &Random, ctx: &mut TxContext) {
    destroy_airdrop_nft(nft);

    let mut generator = new_generator(r, ctx);
    let v = generator.generate_u8_in_range(1, 100);

    transfer::public_transfer(
        RandomnessNFT { id: object::new(ctx), value: v },
        ctx.sender(),
    );
}

public fun reveal_alternative2_step2(nft: RandomnessNFT, ctx: &mut TxContext): MetalNFT {
    let RandomnessNFT { id, value } = nft;
    delete(id);

    let metal = if (value <= 10) GOLD
    else if (10 < value && value <= 40) SILVER
    else BRONZE;

    MetalNFT {
        id: object::new(ctx),
        metal,
    }
}

fun destroy_airdrop_nft(nft: AirDropNFT) {
    let AirDropNFT { id } = nft;
    object::delete(id)
}

public fun metal_string(nft: &MetalNFT): string::String {
    if (nft.metal == GOLD) string::utf8(b"Gold")
    else if (nft.metal == SILVER) string::utf8(b"Silver")
    else string::utf8(b"Bronze")
}

#[test_only]
public fun destroy_cap(cap: MintingCapability) {
    let MintingCapability { id } = cap;
    object::delete(id)
}

#[test_only]
public fun test_init(ctx: &mut TxContext) {
    init(ctx)
}
