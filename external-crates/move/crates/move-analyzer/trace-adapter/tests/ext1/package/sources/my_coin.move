/// Module: my_coin
module package::my_coin;

use sui::balance;
use sui::coin::{Self, TreasuryCap, Coin};

public struct MY_COIN has drop {}

fun init(witness: MY_COIN, ctx: &mut TxContext) {
    let (treasury, metadata) = coin::create_currency(
        witness,
        6,
        b"MY_COIN",
        b"",
        b"",
        option::none(),
        ctx,
    );
    transfer::public_freeze_object(metadata);
    transfer::public_transfer(treasury, ctx.sender())
}

public fun mint(
    treasury_cap: &mut TreasuryCap<MY_COIN>,
    mut vec: vector<u64>,
    ctx: &mut TxContext,
): coin::Coin<MY_COIN> {
    let mut amount = 0;
    while (!vec.is_empty()) {
        amount = amount + vec.pop_back();

    };
    coin::mint(treasury_cap, amount, ctx)
}

public fun burn(treasury_cap: &mut TreasuryCap<MY_COIN>, mut vec: vector<Coin<MY_COIN>>) {
    while (!vec.is_empty()) {
        let coin = vec.pop_back();
        let amount = coin.into_balance();
        let supply = treasury_cap.supply_mut();
        balance::decrease_supply(supply, amount);
    };
    vec.destroy_empty();
}
