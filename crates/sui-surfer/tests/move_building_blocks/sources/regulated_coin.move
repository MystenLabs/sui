// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises regulated coins and the deny list v2 (`create_regulated_currency_v2`,
/// `deny_list_v2_add` / `_remove` / `_enable_global_pause`). The shared `DenyList`
/// (0x403) is taken as a mutable input.
module move_building_blocks::regulated_coin {
    use sui::coin::{Self, TreasuryCap, DenyCapV2};
    use sui::deny_list::DenyList;

    public struct REGULATED_COIN has drop {}

    public struct Caps has key {
        id: UID,
        treasury: TreasuryCap<REGULATED_COIN>,
        deny: DenyCapV2<REGULATED_COIN>,
    }

    fun init(witness: REGULATED_COIN, ctx: &mut TxContext) {
        let (treasury, deny, metadata) = coin::create_regulated_currency_v2(
            witness,
            6,
            b"RSURF",
            b"Regulated Surf",
            b"Regulated coin minted by sui-surfer building blocks",
            option::none(),
            true,
            ctx,
        );
        transfer::public_freeze_object(metadata);
        transfer::share_object(Caps { id: object::new(ctx), treasury, deny });
    }

    public fun mint(caps: &mut Caps, amount: u64, ctx: &mut TxContext) {
        let coin = coin::mint(&mut caps.treasury, amount % 100_000, ctx);
        transfer::public_transfer(coin, ctx.sender());
    }

    public fun deny_add(deny_list: &mut DenyList, caps: &mut Caps, addr: address, ctx: &mut TxContext) {
        if (!coin::deny_list_v2_contains_next_epoch<REGULATED_COIN>(deny_list, addr)) {
            coin::deny_list_v2_add(deny_list, &mut caps.deny, addr, ctx);
        }
    }

    public fun deny_remove(deny_list: &mut DenyList, caps: &mut Caps, addr: address, ctx: &mut TxContext) {
        if (coin::deny_list_v2_contains_next_epoch<REGULATED_COIN>(deny_list, addr)) {
            coin::deny_list_v2_remove(deny_list, &mut caps.deny, addr, ctx);
        }
    }
}
