// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A basic game that depends on randomness from drand (chained mode). See details on how to work
/// with drand in drand_based_lottery.move.
///
/// Anyone can create a new game by depositing X*100 SUIs as a reward, and setting the current drand
/// round as the base round. This creates two objects:
/// - Game - an immutable object that includes all parameters to be used when buying tickets.
/// - Reward - a shared object that holds the reward. It can be withdrawn by any winner ("first
///   come, first served"). If not withdrawn within a few epochs, can be returned to the game creator.
///
/// A user who wishes to play game G should:
/// - Check if G.epoch is the current epoch, and that G.base_drand_round + 24h is in the future.
///   See lottery.move for how to calculate a round for a given point in time.
/// - Check that the relevant reward is still non negative.
/// - Call buy_ticket() with the right amount of SUI.
/// - Wait until the relevant drand round's randomness is available, and can call evaluate().
/// - If received an object Winner, claim the reward using take_reward().
///
/// Important: There is 1 reward per game, and there is no limit on the number of tickets that can
/// be bought for a a single game. *Only* the winner of a game who called take_reward first will
/// receive a reward. One may extend this game and add another round in which winners can register
/// their winner tickets, and then one of the winners is chosen at random. This part, however, will
/// require using a shared object.
module drand::scratch_card {
    use drand::lib;
    use sui::balance::Balance;
    use sui::coin::{Self, Coin};
    use sui::hmac::hmac_sha3_256;
    use sui::sui::SUI;

    // === Object Types ===

    /// Game represents a set of parameters of a single game.
    public struct Game has key {
        id: UID,
        creator: address,
        reward_amount: u64,
        reward_factor: u64,
        base_epoch: u64,
        base_drand_round: u64,
    }

    /// Reward that is attached to a specific game. Can be withdrawn once.
    public struct Reward has key {
        id: UID,
        game_id: ID,
        balance: Balance<SUI>,
    }

    /// Ticket represents a participant in a single game.
    /// Can be deconstructed only by the owner.
    public struct Ticket has key, store {
        id: UID,
        game_id: ID,
    }

    /// Winner represents a participant that won in a specific game.
    /// Can be consumed by the take_reward.
    public struct Winner has key, store {
        id: UID,
        game_id: ID,
    }

    // === Error Codes ===

    #[error]
    const EInvalidDeposit: vector<u8> =
        b"Deposit does not match expected for Game.";

    #[error]
    const EInvalidEpoch: vector<u8> =
        b"Epoch for participation has passed.";

    #[error]
    const EInvalidTicket: vector<u8> =
        b"Ticket does not match game.";

    #[error]
    const EInvalidReward: vector<u8> =
        b"No balance left for reward.";

    #[error]
    const ETooSoonToRedeem: vector<u8> =
        b"Cannot return funds to creator until 3 epochs after the game.";

    #[error]
    const EInvalidGame: vector<u8> =
        b"Game does not match reward pool";

    // === Public Functions ===

    /// Create a new game with a given reward.
    ///
    /// The reward must be a positive balance, dividable by reward_factor. reward/reward_factor will
    /// be the ticket price. base_drand_round is the current drand round.
    public fun create(
        reward: Coin<SUI>,
        reward_factor: u64,
        base_drand_round: u64,
        ctx: &mut TxContext
    ) {
        let amount = reward.value();
        assert!(amount > 0 && amount % reward_factor == 0 , EInvalidReward);

        let game = Game {
            id: object::new(ctx),
            reward_amount: reward.value(),
            creator: ctx.sender(),
            reward_factor,
            base_epoch: ctx.epoch(),
            base_drand_round,
        };
        let reward = Reward {
            id: object::new(ctx),
            game_id: object::id(&game),
            balance: reward.into_balance(),
        };
        transfer::freeze_object(game);
        transfer::share_object(reward);
    }

    /// Buy a ticket for a specific game, costing reward/reward_factor SUI. Can be called only
    /// during the epoch in which the game was created.
    ///
    /// Note that the reward might have been withdrawn already. It's the user's responsibility to
    /// verify that.
    public fun buy_ticket(game: &Game, coin: Coin<SUI>, ctx: &mut TxContext): Ticket {
        assert!(coin.value() * game.reward_factor == game.reward_amount, EInvalidDeposit);
        assert!(ctx.epoch() == game.base_epoch, EInvalidEpoch);
        transfer::public_transfer(coin, game.creator);
        Ticket {
            id: object::new(ctx),
            game_id: object::id(game),
        }
    }

    #[lint_allow(self_transfer)]
    public fun evaluate(ticket: Ticket, game: &Game, drand_sig: vector<u8>, ctx: &mut TxContext) {
        assert!(ticket.game_id == object::id(game), EInvalidTicket);
        lib::verify_drand_signature(drand_sig, game.end_drand_round());

        // The randomness for the current ticket is derived by HMAC(drand randomness, ticket id). A
        // solution like checking if
        //
        //     (drand randomness % reward_factor) == (ticket id % reward_factor)
        //
        // is not secure as the adversary can control the values of ticket id. (For this particular
        // game this attack is not devastating, but for similar games it might be.)
        let random_key = lib::derive_randomness(drand_sig);
        let randomness = hmac_sha3_256(&random_key, &object::id(&ticket).to_bytes());
        let is_winner = (lib::safe_selection(game.reward_factor, &randomness) == 0);

        if (is_winner) {
            let winner = Winner {
                id: object::new(ctx),
                game_id: object::id(game),
            };
            transfer::public_transfer(winner, ctx.sender());
        };

        // Delete the ticket.
        let Ticket { id, game_id:  _} = ticket;
        object::delete(id);
    }

    public fun take_reward(winner: Winner, reward: &mut Reward, ctx: &mut TxContext): Coin<SUI> {
        assert!(winner.game_id == reward.game_id, EInvalidTicket);
        let Winner { id, game_id:  _} = winner;
        object::delete(id);
        let full_balance = reward.balance.value();
        coin::take(&mut reward.balance, full_balance, ctx)
    }

    /// Can be called in case the reward was not withdrawn, to return the coins to the creator.
    public fun redeem(reward: &mut Reward, game: &Game, ctx: &mut TxContext) {
        assert!(reward.balance.value() > 0, EInvalidReward);
        assert!(object::id(game) == reward.game_id, EInvalidGame);

        // Since we define the game to take 24h+25h, a game that is created in epoch x may be
        // completed in epochs x+2 or x+3.
        assert!(game.base_epoch + 3 < ctx.epoch(), ETooSoonToRedeem);
        let full_balance = reward.balance.value();
        transfer::public_transfer(coin::take(&mut reward.balance, full_balance, ctx), game.creator);
    }

    public fun delete_ticket(ticket: Ticket) {
        let Ticket { id, game_id:  _} = ticket;
        object::delete(id);
    }

    public fun base_drand_round(game: &Game): u64 {
        game.base_drand_round
    }

    public fun end_drand_round(game: &Game): u64 {
        // Since users do not know when an epoch began, they can only check if the game depends on a
        // round that is at least 24 hours from now. Since the creator does not know as well if its
        // game is created in the beginning or the end of the epoch, we define the end of the game
        // to be 24h + 24h from when it started, +1h to be on the safe side since epoch duration is
        // not deterministic.
        game.base_drand_round + 20 * 60 * (24 + 25)
    }

    public fun base_epoch(game: &Game): u64 {
        game.base_epoch
    }
}
