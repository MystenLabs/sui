// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


/// Optimistic Satoshi flip is a fair and secure way to conduct a coin flip.
/// A player called "house" chooses a random secret and uploads its hash on chain.
/// Another player called "player",  makes a guess on a predetermined bit (eg. last) of the secret.
/// House reveals the secret and the winner is determined.
/// 
/// We call it optimistic or time-locked because the game can end even if the house refuses to reveal the secret,
/// unlike the original satoshi dice.
/// The player, if the house hasn't revealed after he/she placed a guess and a predetermined number of epochs passed (7 in this implementation), 
/// is allowed to end the game and claim an automatic win.
///
/// The secret should be unpredictable, random and at least 16 bytes in size. The house is responsible for the randomness and picking a proper secret.
/// This implementation checks the last bit of the first byte of the secret.
module games_with_chance::satoshi_flip {
    // imports
    use std::option::{Self, Option};
    use std::hash::sha3_256;
    use std::vector;

    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};
    use sui::sui::SUI;
    use sui::digest::{Self, Sha3256Digest};
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;


    /* Terminology
        house: The user who picks the secret.
        player: The user who wages an amount on his/her guess.
        max_bet: the maximum amount the house is willing to potentialy lose or win.
        min_bet: the minimum amount the house is willing to accept as a wager/stake (too low of an amount might be worth the gas) min_bet <= max_bet.
        stake: the amount the player is willing to wager on his/her guess, min_bet <= stake <= max_bet.
    */

    /*
        Conventions:
        A guess = 2 in the outcome means player has not bet but the game was ended
        A guess = 3 in the outcome means player has bet but house refused to reveal in time
    */

    /// Hash function the house will use.
    const HASH: vector<u8> = b"SHA3-256";

    /// How many epochs must pass after `fun bet` was called, until the player may cancel a game.
    ///
    /// We chose 7 epochs because 7 days is the norm for fraud proof systems. This timeframe takes into account system/custody malfunctions,
    /// network outages, blockchain monitoring delays, potential need for cold key retrieval, holidays, weekends...
    // TODO: This should be possible to be set by house up to a max limit.
    const EpochsCancelAfter: u64 = 7;

    // errors
    const EStakeTooHigh: u64 = 0; // this should hold stake <= max bet.
    const EStakeTooLow: u64 = 1; // this should hold stake >= min bet.
    const EGuessNot1Or0: u64 = 2; // guess should be either 1 or 0.
    const EHashAndSecretDontMatch: u64 = 3; // foul play or mistake, house provided wrong secret.
    const EMinBetTooHigh: u64 = 4; // house's min bet must be lower or equal to max bet.
    const EGameAlreadyEnded: u64 = 5; // Too late to cancel a game that has been settled.
    const ENotEnoughEpochsPassedToCancel: u64 = 6; // Can only cancel after above EpochsCancelAfter epochs, after the bet was placed.
    const ENotAllowedToEndGame: u64 = 7; // Only house may end a game.
    const ECannotCancelBeforeBetting: u64 = 8; // Only a game that has received a bet may be cancelled.
    const EHouseCoinNotEnoughOrTooMuch: u64 = 9; // House provided coin should match the max_bet exactly.
    const EZeroMinBet: u64 = 10; // House set min_bet = 0.
    const EGameNotEnded: u64 = 11;
    const EAlreadyAcceptedBet: u64 = 12;
    const ESecretIsEmpty: u64 = 13; // secret can't be an empty string.

    // structs

    /// Player's data, the stake of the wager and her guess.
    struct BetData has store {
        stake: Balance<SUI>,
        guess: u8,
    }

    /// House's data, min_bet is the minimum amount accepted for a bet.
    /// max_bet is the maximum amount accepted for a bet.
    struct HouseData has store {
        house_balance: Balance<SUI>,
        min_bet: u64,
        max_bet: u64
    }

    /// Outcome will hold end game data, the secret the house picked, the player's guess and if the player won.
    /// This way anyone can check if the winner was correctly determined.
    struct Outcome has store {
        secret: vector<u8>,
        guess: u8,
        player_won: bool
    }

    /// Each Game is a shared object, the address that created it must also end it.
    /// The player can only be a single address for each game.
    ///
    ///  @ownership: Shared
    ///

    struct Game has key {
        id: UID,
        bet_placed_epoch: u64,
        hashed_secret: Sha3256Digest,
        house: address,
        player: Option<address>,
        house_data: Option<HouseData>,
        bet_data: Option<BetData>,
        outcome: Option<Outcome>
    }

    // fun(ctions)!

    /// Creates a new game and makes it a transfered object.
    /// A user who wants to become house should call this function and provide a coin with balance equal to max_bet.
    ///
    /// The hash should be a SHA3-256 digest of at least 16 random bytes. The house is repsonsible for picking a proper random secret.
    public entry fun start_game(hash: vector<u8>, house_coin: Coin<SUI>, min_bet: u64, max_bet: u64, ctx: &mut TxContext) {
        assert!(min_bet > 0, EZeroMinBet);
        assert!(max_bet >= min_bet, EMinBetTooHigh);
        assert!(coin::value(&house_coin) == max_bet, EHouseCoinNotEnoughOrTooMuch);
        let house_data = HouseData {
            house_balance: coin::into_balance(house_coin),
            min_bet,
            max_bet
        };

        let new_game = Game {
            id: object::new(ctx),
            bet_placed_epoch: tx_context::epoch(ctx),
            hashed_secret: digest::sha3_256_digest_from_bytes(hash),
            house: tx_context::sender(ctx),
            player: option::none(),
            house_data: option::none(),
            bet_data: option::none(),
            outcome: option::none()

        };

        option::fill(&mut new_game.house_data, house_data);

        transfer::share_object(new_game); // this should be deleted when shared objects acquire this ability.
    }

    // accessors

    /// Get the house's address on Sui.
    public fun house(game: &Game): address {
        game.house
    }

    /// Get the maximum stake the house is willing to accept.
    public fun max_bet(game: &Game): u64 {
        let house_data = option::borrow(&game.house_data);
        house_data.max_bet
    }

    /// Get the minimum stake the house is willing to accept.
    public fun min_bet(game: &Game): u64 {
        let house_data = option::borrow(&game.house_data);
        house_data.min_bet
    }
    
    /// On an ended game (that has a non null outcome value) get if player won.
    public fun is_player_winner(game: &Game): bool {
        assert!(option::is_some<Outcome>(&game.outcome), EGameNotEnded);
        let game_outcome = option::borrow(&game.outcome);
        game_outcome.player_won
    }

    /// On an ended game (that has a non null outcome value) get what secret did the house pick.
    public fun secret(game: &Game): vector<u8> {
        assert!(option::is_some<Outcome>(&game.outcome), EGameNotEnded);
        let game_outcome = option::borrow(&game.outcome);
        game_outcome.secret
    }

    /// On an ended game (that has a non null outcome value) get what did the player guess.
    public fun guess(game: &Game): u8 {
        assert!(option::is_some<Outcome>(&game.outcome), EGameNotEnded);
        let game_outcome = option::borrow(&game.outcome);
        game_outcome.guess
    }

    /// Called by the player for a game with null BetData.
    /// The player should have already split the coin to be used so that it has a proper balance,
    /// the value should be between min and max bet.
    public entry fun bet(game: &mut Game, guess: u8, stake_coin: Coin<SUI>, ctx: &mut TxContext) {
        assert!(option::is_none(&game.bet_data), EAlreadyAcceptedBet);
        let house_data = option::borrow(&game.house_data);
        assert!(coin::value(&stake_coin) <= house_data.max_bet, EStakeTooHigh);
        assert!(coin::value(&stake_coin) >= house_data.min_bet, EStakeTooLow);
        assert!(guess == 0 || guess == 1, EGuessNot1Or0);
        game.bet_placed_epoch = tx_context::epoch(ctx);

        let stake = coin::into_balance(stake_coin);

        let bet_data = BetData {
            stake: stake,
            guess
        };
        option::fill(&mut game.bet_data, bet_data);
        option::fill(&mut game.player, tx_context::sender(ctx));
    }

    /// Called by the house either after `fun bet` has been called by some player to end the game,
    /// either before bet was called to "cancel" the game.
    ///
    /// If the house has "forgotten" the secret, then it can use a random secret and it will
    /// cancel the game, as long as `fun bet` has not been called.
    public entry fun end_game(game: &mut Game, secret: vector<u8>, ctx: &mut TxContext) {
        // only house should be able to end the game
        assert!(game.house == tx_context::sender(ctx), ENotAllowedToEndGame);

        // house wants to cancel the current game (maybe forgot the secret).
        if (option::is_none(& game.bet_data)) {
            let msg = b"Game Canceled by House";
            let outcome = Outcome {
                secret: msg,
                guess: 2,
                player_won: false
            };
            option::fill(&mut game.outcome, outcome);

            // no bet placed, return the balance to house.
            let HouseData {house_balance, min_bet: _, max_bet: _} = option::extract(&mut game.house_data);
            let house_coins = coin::from_balance(house_balance, ctx);
            transfer::transfer(house_coins, game.house);
        } else {
            assert!(!vector::is_empty(&secret), ESecretIsEmpty);
            let hash = sha3_256(secret);
            assert!(hash == digest::sha3_256_digest_to_bytes(&game.hashed_secret), EHashAndSecretDontMatch);
            // extract balances and guess
            let HouseData {house_balance, min_bet: _, max_bet: _} = option::extract(&mut game.house_data);
            let BetData {stake, guess} = option::extract(&mut game.bet_data);
            let first_byte = vector::borrow(&secret, 0);
            let won = guess == *first_byte % 2;

            if (won) {
                let outcome = Outcome {
                    secret,
                    guess,
                    player_won: true
                };
                option::fill(&mut game.outcome, outcome);
                let player = option::extract(&mut game.player);
                pay_player(player, game.house, stake, house_balance, ctx);
            } else {
                // player lost
                let outcome = Outcome {
                    secret,
                    guess,
                    player_won: false
                };
                option::fill(&mut game.outcome, outcome);

                balance::join(&mut house_balance, stake);
                transfer::transfer(coin::from_balance(house_balance, ctx), game.house);
            };
        }
    }

    /// Called by anyone after the required epochs have passed and house has not revealed the secret.
    /// The house automatically loses.
    /// Check EpochsCancelAfter for the number of epochs required to pass after `fun bet` has been called.
    public entry fun cancel_game(game: &mut Game, ctx: &mut TxContext) {
        // this can't be called on an ended game
        assert!(option::is_none<Outcome>(&game.outcome), EGameAlreadyEnded);
        // a bet has to have been placed
        assert!(option::is_some<BetData>(&game.bet_data), ECannotCancelBeforeBetting);
        // this can only be called `CancelEpochsAfter` epochs after the bet has been placed.
        assert!(game.bet_placed_epoch + EpochsCancelAfter <= tx_context::epoch(ctx), ENotEnoughEpochsPassedToCancel);

        let HouseData {house_balance, min_bet: _, max_bet: _} = option::extract(&mut game.house_data);
        let BetData {stake, guess: _} = option::extract(&mut game.bet_data);
        
        let outcome = Outcome {
            secret: b"Game Canceled by Player",
            guess: 3,
            player_won: true
        };
        option::fill(&mut game.outcome, outcome);
        let player = option::extract(&mut game.player);
        pay_player(player, game.house, stake, house_balance, ctx);
    }

    // helper functions.

    /// Helper function to calculate and send SUI Coins with proper balances to each party.
    fun pay_player(player: address, house: address, stake: Balance<SUI>, house_balance: Balance<SUI>, ctx: &mut TxContext) {
        // if bet is less than max_bet, return the difference to house after paying the wins.
         if (balance::value(&stake) < balance::value(&house_balance)) {
            let profit = balance::split(&mut house_balance, balance::value(&stake));
            // calculate the wins = profit + stake.
            balance::join(&mut stake, profit);
            // pay the wins.
            transfer::transfer(coin::from_balance(stake, ctx), player);
            // return the rest back to house.
            transfer::transfer(coin::from_balance(house_balance, ctx), house);
        } else {
            // profit is the whole max_bet.
            balance::join(&mut stake, house_balance);
            transfer::transfer(coin::from_balance(stake, ctx), player);
        };
    }
}