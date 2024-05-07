// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A betting game that depends on Sui randomness.
///
/// Anyone can create a new game for the current epoch by depositing SUI as the initial balance. The creator can
/// withdraw the remaining balance after the epoch is over.
///
/// Anyone can play the game by betting on X SUI. They win X with probability 49% and lose the X SUI otherwise.
///
module games::slot_machine {
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::math;
    use sui::random::{Self, Random, new_generator};
    use sui::sui::SUI;

    /// Error codes
    const EInvalidAmount: u64 = 0;
    const EInvalidSender: u64 = 1;
    const EInvalidEpoch: u64 = 2;

    /// Game for a specific epoch.
    public struct Game has key {
        id: UID,
        creator: address,
        epoch: u64,
        balance: Balance<SUI>,
    }

    /// Create a new game with a given initial reward for the current epoch.
    public fun create(
        reward: Coin<SUI>,
        ctx: &mut TxContext,
    ) {
        let amount = coin::value(&reward);
        assert!(amount > 0, EInvalidAmount);
        transfer::share_object(Game {
            id: object::new(ctx),
            creator: tx_context::sender(ctx),
            epoch: tx_context::epoch(ctx),
            balance: coin::into_balance(reward),
        });
    }

    /// Creator can withdraw remaining balance if the game is over.
    public fun close(game: Game, ctx: &mut TxContext): Coin<SUI> {
        assert!(tx_context::epoch(ctx) > game.epoch, EInvalidEpoch);
        assert!(tx_context::sender(ctx) == game.creator, EInvalidSender);
        let Game { id, creator: _, epoch: _, balance } = game;
        object::delete(id);
        coin::from_balance(balance, ctx)
    }

    /// Play one turn of the game.
    ///
    /// The function consumes the same amount of gas independently of the random outcome.
    entry fun play(game: &mut Game, r: &Random, coin: &mut Coin<SUI>, ctx: &mut TxContext) {
        assert!(tx_context::epoch(ctx) == game.epoch, EInvalidEpoch);
        assert!(coin::value(coin) > 0, EInvalidAmount);

        // play the game
        let mut generator = new_generator(r, ctx);
        let bet = random::generate_u8_in_range(&mut generator, 1, 100);
        let lost = bet / 50; // 0 with probability 49%, and 1 or 2 with probability 51%
        let won = (2 - lost) / 2; // 1 with probability 49%, and 0 with probability 51%

        // move the bet amount from the user's coin to the game's balance
        let coin_value = coin::value(coin);
        let bet_amount = math::min(coin_value, balance::value(&game.balance));
        coin::put(&mut game.balance, coin::split(coin, bet_amount, ctx));

        // move the reward to the user's coin
        let reward = 2 * (won as u64) * bet_amount;
        // the assumption here is that the next line does not consumes more gas when called with zero reward than with
        // non-zero reward
        coin::join(coin, coin::take(&mut game.balance, reward, ctx));
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
