// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A betting game that depends on Sui randomness.
///
/// Anyone can create a new game for the current epoch by depositing SUIs as the initial balance. The creator can
/// withdraw the remaining balance after the epoch is over.
///
/// Anyone can play the game by betting on X SUIs. They win X with probability 49% and loss the X SUIs otherwise.
///
module games::slot_machine {
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::math;
    use sui::object::{Self, UID};
    use sui::random::{Self, Random, new_generator};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Error codes
    const EInvalidAmount: u64 = 0;
    const EInvalidSender: u64 = 1;
    const EInvalidEpoch: u64 = 2;

    /// Game for a specific epoch.
    struct Game has key, store {
        id: UID,
        creator: address,
        epoch: u64,
        balance: Balance<SUI>,
    }

    /// Create a new game with a given initial reward for the current epoch.
    public fun create(
        reward: Coin<SUI>,
        ctx: &mut TxContext
    ) {
        let amount = coin::value(&reward);
        assert!(amount > 0, EInvalidAmount);
        transfer::public_share_object(Game {
            id: object::new(ctx),
            creator: tx_context::sender(ctx),
            epoch: tx_context::epoch(ctx),
            balance: coin::into_balance(reward),
        });
    }

    /// Creator can withdraw remaining balance if the game is over.
    public fun close(game: &mut Game, ctx: &mut TxContext): Coin<SUI> {
        assert!(tx_context::epoch(ctx) > game.epoch, EInvalidEpoch);
        assert!(tx_context::sender(ctx) == game.creator, EInvalidSender);
        let full_balance = balance::value(&game.balance);
        coin::take(&mut game.balance, full_balance, ctx)
    }

    /// Play one turn of the game.
    ///
    /// The function consumes more gas in the "winning" case than the "losing" case, thus gas consumption attacks are
    /// not possible.
    entry fun play(game: &mut Game, r: &Random, coin: Coin<SUI>, ctx: &mut TxContext) {
        assert!(tx_context::epoch(ctx) == game.epoch, EInvalidEpoch);
        assert!(coin::value(&coin) > 0, EInvalidAmount);

        let coin_value = coin::value(&coin);
        let bet_amount = math::min(coin_value, balance::value(&game.balance));
        coin::put(&mut game.balance, coin);

        let generator = new_generator(r, ctx);
        let bet = random::generate_u8_in_range(&mut generator, 1, 100);
        let won = bet <= 49;

        let amount = coin_value - bet_amount;
        if (won) { amount = amount + 2 * bet_amount; };
        let to_user_coin = coin::take(&mut game.balance, amount, ctx);
        transfer::public_transfer(to_user_coin, tx_context::sender(ctx));
    }

    #[test_only]
    public fun get_balance(game: &Game): u64 {
        balance::value(&game.balance)
    }

    #[test_only]
    public fun get_epoch(game: &Game): u64 {
        game.epoch
    }

}
