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
        playerA: The player who picks the secret
        playerB: The player who wages an amount on his guess
        max_bet: the maximum amount the playerA is willing to potentialy lose or win
        min_bet: the minimum amount the playerA is willing to accept as a wager/stake (too low of an amount might be worth the gas) min_bet <= max_bet
        stake: the amount the playerB is willing to wager on his guess, min_bet <= stake <= max_bet
    */

    /*
        Conventions:
        A guess = 2 in the outcome means player B has not bet but the game was forcefully ended
        A guess = 3 in the outcome means player B has bet but A refused to reveal in time
    */

    // everyone should know which hashing function is going to be used
    const HASH: vector<u8> = b"SHA3-256";

    // After how many epochs is cancelling possible
    const EpochsCancelAfter: u64 = 2;

    // errors
    const EStakeTooHigh: u64 = 0;
    const EStakeTooLow: u64 = 1;
    // if you reached here then you inserted something other than 1 or 0 as a guess.
    const EGuessNot1Or0: u64 = 2;
    const EHashAndSecretDontMatch: u64 = 3;
    // min_bet must me lower or equal to max_bet
    const EMinBetTooHigh: u64 = 4;
    const EGameAlreadyEnded: u64 = 5; 
    const ENotEnoughEpochsPassedToCancel: u64 = 6;
    const EGameNotEnded: u64 = 7;
    const ENotAllowedToEndGame: u64 = 8;
    // structs

    // use separate structs for bet and bank so we can extract the coins later
    struct BetData has store {
        stake: Balance<SUI>,
        guess: u8,
    }

    struct BankData has store {
        max_bet: Balance<SUI>,
        min_bet: u64,
    }

    struct Outcome has store {
        secret: vector<u8>,
        guess: u8,
        playerB_won: bool
    }

    struct Game has key {
        id: UID,
        epoch: u64,
        hashed_secret: Sha3256Digest,
        playerA: address , // 1st player,
        playerB: Option<address>, // 2nd player
        bank_data: Option<BankData>,
        bet_data: Option<BetData>,
        outcome: Option<Outcome>
    }

    // fun!

    // this is invoked by playerA
    public entry fun start_game(hash: vector<u8>, max_bet: Coin<SUI>, min_bet: u64, ctx: &mut TxContext) {
        assert!(coin::value(&max_bet) >= min_bet, EMinBetTooHigh);
        let bank_data = BankData {
            max_bet: coin::into_balance(max_bet),
            min_bet
        };

        let new_game = Game {
            id: object::new(ctx),
            epoch: tx_context::epoch(ctx),
            hashed_secret: digest::sha3_256_digest_from_bytes(hash),
            playerA: tx_context::sender(ctx),
            playerB: option::none(),
            bank_data: option::none(),
            bet_data: option::none(),
            outcome: option::none()

        };

        option::fill(&mut new_game.bank_data, bank_data);

        transfer::share_object(new_game); // make it a shared object for now
    }

    // accessors for playerB to check

    public fun playerA(game: &Game): address {
        game.playerA
    }

    public fun max_bet(game: &Game): u64 {
        let bank_data = option::borrow(&game.bank_data);
        balance::value(&bank_data.max_bet)
    }

    public fun min_bet(game: &Game): u64 {
        let bank_data = option::borrow(&game.bank_data);
        bank_data.min_bet
    }
    
    // these are to view a finished game's result
    public fun is_playerB_winner(game: &mut Game): bool {
        let game_outcome = option::borrow(&game.outcome);
        game_outcome.playerB_won
    }

    public fun secret(game: &mut Game): vector<u8> {
        let game_outcome = option::borrow(&game.outcome);
        game_outcome.secret
    }

    public fun guess(game: &mut Game): u8 {
        let game_outcome = option::borrow(&game.outcome);
        game_outcome.guess
    }

    // this is invoked by playerB
    public entry fun bet(game: &mut Game, guess: u8, stake: Coin<SUI>, ctx: &mut TxContext) {
        let bank_data = option::borrow(&game.bank_data);
        assert!(coin::value(&stake) <= balance::value(&bank_data.max_bet), EStakeTooHigh);
        assert!(coin::value(&stake) > bank_data.min_bet, EStakeTooLow);
        assert!(guess == 0 || guess == 1, EGuessNot1Or0);
        let bet_data = BetData {
            stake: coin::into_balance(stake),
            guess
        };
        option::fill(&mut game.bet_data, bet_data);
        option::fill(&mut game.playerB, tx_context::sender(ctx));
    }

    public entry fun end_game(game: &mut Game, secret: vector<u8>, ctx: &mut TxContext) {
        // only player A should be able to end the game
        assert!(game.playerA == tx_context::sender(ctx), ENotAllowedToEndGame);
        let hash = sha3_256(secret);
        if (hash != digest::sha3_256_digest_to_bytes(&game.hashed_secret)) {
            let BetData {stake, guess:_} = option::extract(&mut game.bet_data);
            let BankData {max_bet, min_bet: _} = option::extract(&mut game.bank_data);
            let playerB = option::extract(&mut game.playerB);
            // TODO: Maybe add some punishment for the playerA
            transfer::transfer(coin::from_balance(stake, ctx), playerB);
            transfer::transfer(coin::from_balance(max_bet, ctx), game.playerA);
            abort EHashAndSecretDontMatch
        };
        let BankData {max_bet, min_bet: _} = option::extract(&mut game.bank_data);
        if (option::is_some<BetData>(&mut game.bet_data)){
            // extract balances and guess
            let BetData {stake, guess} = option::extract(&mut game.bet_data);
            let first_byte = vector::borrow(&secret, 0);
            let won = guess == *first_byte % 2;

            if (won) {
                let outcome = Outcome {
                    secret,
                    guess,
                    playerB_won: true
                };
                option::fill(&mut game.outcome, outcome);
                let playerB = option::extract(&mut game.playerB);
                pay_playerB(playerB, game.playerA, stake, max_bet, ctx);
            } else {
                // playerB lost
                let outcome = Outcome {
                    secret,
                    guess,
                    playerB_won: false
                };
                option::fill(&mut game.outcome, outcome);

                balance::join(&mut max_bet, stake);
                transfer::transfer(coin::from_balance(max_bet, ctx), game.playerA);
            };
        } else {
            let outcome = Outcome {
                secret,
                guess: 2,
                playerB_won: false
            };
            option::fill(&mut game.outcome, outcome);

            // no bet placed, return the max_bet to playerA
            let wins = coin::from_balance(max_bet, ctx);
            transfer::transfer(wins, game.playerA);
        }
    }

    // usually called by playerB when playerA refuses to reveal
    public entry fun cancel_game(game: &mut Game, ctx: &mut TxContext) {
        // this can be called on an ended game
        assert!(option::is_some<Outcome>(&game.outcome), EGameAlreadyEnded);
        // this call can only be called 2 epochs after the game was created
        assert!(game.epoch + EpochsCancelAfter <= tx_context::epoch(ctx), ENotEnoughEpochsPassedToCancel);
        // this exists since the game is created
        let BankData {max_bet, min_bet: _} = option::extract(&mut game.bank_data);

        // if player B hasn't bet, return max_bet to player A
        if (!option::is_none(&game.bet_data)) {
            let outcome = Outcome {
                secret: b"Game Canceled",
                guess: 2,
                playerB_won: false
            };
            option::fill(&mut game.outcome, outcome);
            transfer::transfer(coin::from_balance(max_bet, ctx), game.playerA);
        }
        // if player B has bet and A has not revealed 2 epochs later, pay player B
        else {
            let BetData {stake, guess: _} = option::extract(&mut game.bet_data);
            
            let outcome = Outcome {
                secret: b"Game Canceled",
                guess: 3,
                playerB_won: true
            };
            option::fill(&mut game.outcome, outcome);
            let playerB = option::extract(&mut game.playerB);
            pay_playerB(playerB, game.playerA, stake, max_bet, ctx);
        }
    }

    // helper functions

    fun pay_playerB(playerB: address, playerA: address, stake: Balance<SUI>, max_bet: Balance<SUI>, ctx: &mut TxContext) {
        // if bet is less than max_bet, return the difference to player A after paying the wins
         if (balance::value(&stake) < balance::value(&max_bet)) {
            let profit = balance::split(&mut max_bet, balance::value(&stake));
            // calculate the wins = profit + stake
            balance::join(&mut stake, profit);
            // pay the wins
            transfer::transfer(coin::from_balance(stake, ctx), playerB);
            // return the rest back to playerA
            transfer::transfer(coin::from_balance(max_bet, ctx), playerA);
        } else {
            // profit is the whole max_bet
            balance::join(&mut stake, max_bet);
            transfer::transfer(coin::from_balance(stake, ctx), playerB);
        };
    }
}