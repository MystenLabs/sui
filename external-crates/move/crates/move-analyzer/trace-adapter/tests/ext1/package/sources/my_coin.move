/// Module: my_coin
module package::my_coin;

use sui::{coin::{Self, TreasuryCap, Coin}, balance};

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
    amount: u64,
    ctx: &mut TxContext,
): coin::Coin<MY_COIN> {
    coin::mint(treasury_cap, amount, ctx)
}

public fun burn(
    treasury_cap: &mut TreasuryCap<MY_COIN>,
    coin: Coin<MY_COIN>,
) {
    let amount = coin.into_balance();
    let supply = treasury_cap.supply_mut();
    balance::decrease_supply(supply, amount);
}
