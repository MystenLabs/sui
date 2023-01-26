// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A basic game that depends on randomness from drand (chained mode). See details on how to work with drand in
/// drand_based_lottery.move.
///
/// Anyone can create a new game by depositing X*100 SUIs as a reward, and setting the current drand round as the base
/// round. This creates two objects:
/// - Game - an immutable object that includes all parameters to be used when buying tickets.
/// - Reward - a shared object that holds the reward. It can be withdrawn by any winner ("first come, first served").
///   If not withdrawn within a few epochs, can be returned to the game creator.
///
/// A user who wishes to play game G should:
/// - Check if G.epoch is the current epoch, and that G.base_drand_round + 24h is in the future.
///   See drand_based_lottery.move for how to calculate a round for a given point in time.
/// - Check that the relevant reward is still non negative.
/// - Call buy_ticket() with the right amount of SUI.
/// - Wait until the relevant drand round's randomness is available, and can call evaluate().
/// - If received an object Winner, claim the reward using take_reward().
///
/// Important: There is 1 reward per game, and there is no limit on the number of tickets that can be bought for a
/// a single game. *Only* the winner of a game who called take_reward first will receive a reward.
/// One may extend this game and add another round in which winners can register their winner tickets, and then one of
/// the winners is chosen at random. This part, however, will require using a shared object.
///
module games::drand_based_scratch_card {
    use games::drand_lib;
    use sui::balance::Balance;
    use sui::balance::{Self};
    use sui::coin::{Self, Coin};
    use sui::digest;
    use sui::hmac::hmac_sha3_256;
    use sui::object::{Self, ID, UID};
    use sui::randomness::safe_selection;
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Error codes
    const EInvalidDeposit: u64 = 0;
    const EInvalidEpoch: u64 = 1;
    const EInvalidTicket: u64 = 2;
    const EInvalidRandomness: u64 = 3;
    const EInvalidReward: u64 = 4;
    const ETooSoonToRedeem: u64 = 5;
    const EInvalidGame: u64 = 6;

    /// Game represents a set of parameters of a single game.
    struct Game has key {
        id: UID,
        creator: address,
        reward_amount: u64,
        reward_factor: u64,
        base_epoch: u64,
        base_drand_round: u64,
    }

    /// Reward that is attached to a specific game. Can be withdrawn once.
    struct Reward has key {
        id: UID,
        game_id: ID,
        balance: Balance<SUI>,
    }

    /// Ticket represents a participant in a single game.
    /// Can be deconstructed only by the owner.
    struct Ticket has key, store {
        id: UID,
        game_id: ID,
    }

    /// Winner represents a participant that won in a specific game.
    /// Can be consumed by the take_reward.
    struct Winner has key, store {
        id: UID,
        game_id: ID,
    }

    /// Create a new game with a given reward.
    ///
    /// The reward must be a positive balance, dividable by reward_factor. reward/reward_factor will be the ticket
    /// price. base_drand_round is the current drand round.
    public entry fun create(
        reward: Coin<SUI>,
        reward_factor: u64,
        base_drand_round: u64,
        ctx: &mut TxContext
    ) {
        let amount = coin::value(&reward);
        assert!(amount > 0 && amount % reward_factor == 0 , EInvalidReward);

        let game = Game {
            id: object::new(ctx),
            reward_amount: coin::value(&reward),
            creator: tx_context::sender(ctx),
            reward_factor,
            base_epoch: tx_context::epoch(ctx),
            base_drand_round,
        };
        let reward = Reward {
            id: object::new(ctx),
            game_id: object::id(&game),
            balance: coin::into_balance(reward),
        };
        transfer::freeze_object(game);
        transfer::share_object(reward);
    }

    /// Buy a ticket for a specific game, costing reward/reward_factor SUI. Can be called only during the epoch in which
    /// the game was created.
    /// Note that the reward might have been withdrawn already. It's the user's responsibility to verify that.
    public entry fun buy_ticket(coin: Coin<SUI>, game: &Game, ctx: &mut TxContext) {
        assert!(coin::value(&coin) * game.reward_factor == game.reward_amount, EInvalidDeposit);
        assert!(tx_context::epoch(ctx) == game.base_epoch, EInvalidEpoch);
        let ticket = Ticket {
            id: object::new(ctx),
            game_id: object::id(game),
        };
        transfer::transfer(coin, game.creator);
        transfer::transfer(ticket, tx_context::sender(ctx));
    }

    public entry fun evaluate(
        ticket: Ticket,
        game: &Game,
        drand_sig: vector<u8>,
        drand_prev_sig: vector<u8>,
        ctx: &mut TxContext
    ) {
        assert!(ticket.game_id == object::id(game), EInvalidTicket);
        drand_lib::verify_drand_signature(drand_sig, drand_prev_sig, end_of_game_round(game.base_drand_round));
        // The randomness for the current ticket is derived by HMAC(drand randomness, ticket id).
        // A solution like checking if (drand randomness % reward_factor) == (ticket id % reward_factor) is not secure
        // as the adversary can control the values of ticket id. (For this particular game this attack is not
        // devastating, but for similar games it might be.)
        let random_key = drand_lib::derive_randomness(drand_sig);
        let randomness = hmac_sha3_256(&random_key, &object::id_to_bytes(&object::id(&ticket)));
        let is_winner = (safe_selection(game.reward_factor, &digest::sha3_256_digest_to_bytes(&randomness)) == 0);

        if (is_winner) {
            let winner = Winner {
                id: object::new(ctx),
                game_id: object::id(game),
            };
            transfer::transfer(winner, tx_context::sender(ctx));
        };
        // Delete the ticket.
        let Ticket { id, game_id:  _} = ticket;
        object::delete(id);
    }

    public entry fun take_reward(winner: Winner, reward: &mut Reward, ctx: &mut TxContext) {
        assert!(winner.game_id == reward.game_id, EInvalidTicket);
        let full_balance = balance::value(&reward.balance);
        if (full_balance > 0) {
            transfer::transfer(coin::take(&mut reward.balance, full_balance, ctx), tx_context::sender(ctx));
        };
        let Winner { id, game_id:  _} = winner;
        object::delete(id);
    }

    /// Can be called in case the reward was not withdrawn, to return the coins to the creator.
    public entry fun redeem(reward: &mut Reward, game: &Game, ctx: &mut TxContext) {
        assert!(balance::value(&reward.balance) > 0, EInvalidReward);
        assert!(object::id(game) == reward.game_id, EInvalidGame);
        // Since we define the game to take 24h+25h, a game that is created in epoch x may be completed in epochs
        // x+2 or x+3.
        assert!(game.base_epoch + 3 < tx_context::epoch(ctx), ETooSoonToRedeem);
        let full_balance = balance::value(&reward.balance);
        transfer::transfer(coin::take(&mut reward.balance, full_balance, ctx), game.creator);
    }

    public entry fun delete_ticket(ticket: Ticket) {
        let Ticket { id, game_id:  _} = ticket;
        object::delete(id);
    }

    public fun get_game_base_drand_round(game: &Game): u64 {
        game.base_drand_round
    }

    public fun get_game_base_epoch(game: &Game): u64 {
        game.base_epoch
    }

    public fun end_of_game_round(round: u64): u64 {
        // Since users do not know when an epoch has began, they can only check if the game depends on a round that is
        // at least 24 hours from now. Since the creator does not know as well if its game is created in the beginning
        // or the end of the epoch, we define the end of the game to be 24h + 24h from when it started, +1h to be on
        // the safe side since epoch duration is not deterministic.
        round + 2 * 60 * (24 + 25)
    }
}
