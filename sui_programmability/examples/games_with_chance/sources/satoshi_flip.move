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
        house: The player who picks the secret
        player: The user who wages an amount on his guess
        max_bet: the maximum amount the house is willing to potentialy lose or win
        min_bet: the minimum amount the house is willing to accept as a wager/stake (too low of an amount might be worth the gas) min_bet <= max_bet
        stake: the amount the player is willing to wager on his guess, min_bet <= stake <= max_bet
    */

    /*
        Conventions:
        A guess = 2 in the outcome means player has not bet but the game was ended
        A guess = 3 in the outcome means player has bet but house refused to reveal in time
    */

    // everyone should know which hashing function is going to be used
    const HASH: vector<u8> = b"SHA3-256";

    // After how many epochs is cancelling possible
    const EpochsCancelAfter: u64 = 5;

    // errors
    const EStakeTooHigh: u64 = 0; // this should hold stake <= max bet
    const EStakeTooLow: u64 = 1; // this should hold stake >= min bet
    const EGuessNot1Or0: u64 = 2; // guess should be either 1 or 0
    const EHashAndSecretDontMatch: u64 = 3; // foul play or mistake, house provided wrong secret
    const EMinBetTooHigh: u64 = 4; // house's min bet must be lower or equal to max bet
    const EGameAlreadyEnded: u64 = 5; // Too late to cancel a game that has been settled
    const ENotEnoughEpochsPassedToCancel: u64 = 6; // Can only cancel after above EpochsCancelAfter epochs, after the bet was placed
    const ENotAllowedToEndGame: u64 = 7; // Only house may end a game
    const ECannotCancelBeforeBetting: u64 = 8; // Only a game that has received a bet may be cancelled
    const EHouseCoinNotEnough: u64 = 9; // House provided coin with insuficient balance to cover max bet
    const EPlayerCoinNotEnoughBalance: u64 = 10; // Player provided a coin with insufficient balance to cover his stake

    // structs

    // use separate structs for bet and house so we can extract the coins later
    // BetData holds info about the player and HouseData holds info about the house
    struct BetData has store {
        stake: Balance<SUI>,
        guess: u8,
    }

    struct HouseData has store {
        house_balance: Balance<SUI>,
        min_bet: u64,
        max_bet: u64
    }

    struct Outcome has store {
        secret: vector<u8>,
        guess: u8,
        player_won: bool
    }

    struct Game has key {
        id: UID,
        epoch: u64,
        hashed_secret: Sha3256Digest,
        house: address ,
        player: Option<address>,
        house_data: Option<HouseData>,
        bet_data: Option<BetData>,
        outcome: Option<Outcome>
    }

    // fun(ctions)!

    // this is invoked by house
    // each game will be a shared object so the player can cancel it in case the house refuses to end after EpochsCancelAfter epochs have passed
    public entry fun start_game(hash: vector<u8>, house_coin: Coin<SUI>, max_bet: u64, min_bet: u64, ctx: &mut TxContext) {
        assert!(max_bet >= min_bet, EMinBetTooHigh);
        assert!(coin::value(&house_coin) >= max_bet, EHouseCoinNotEnough);
        let house_data = HouseData {
            house_balance: coin::into_balance(house_coin),
            min_bet,
            max_bet
        };

        let new_game = Game {
            id: object::new(ctx),
            epoch: tx_context::epoch(ctx),
            hashed_secret: digest::sha3_256_digest_from_bytes(hash),
            house: tx_context::sender(ctx),
            player: option::none(),
            house_data: option::none(),
            bet_data: option::none(),
            outcome: option::none()

        };

        option::fill(&mut new_game.house_data, house_data);

        transfer::share_object(new_game); // this should be deleted when shared objects acquire this ability
    }

    // accessors for player to check
    public fun house(game: &Game): address {
        game.house
    }

    public fun max_bet(game: &Game): u64 {
        let house_data = option::borrow(&game.house_data);
        house_data.max_bet
    }

    public fun min_bet(game: &Game): u64 {
        let house_data = option::borrow(&game.house_data);
        house_data.min_bet
    }
    
    // these are to view a finished game's result
    public fun is_player_winner(game: &mut Game): bool {
        let game_outcome = option::borrow(&game.outcome);
        game_outcome.player_won
    }

    public fun secret(game: &mut Game): vector<u8> {
        let game_outcome = option::borrow(&game.outcome);
        game_outcome.secret
    }

    public fun guess(game: &mut Game): u8 {
        let game_outcome = option::borrow(&game.outcome);
        game_outcome.guess
    }

    // this is invoked by player
    public entry fun bet(game: &mut Game, guess: u8, stake_coin: Coin<SUI>, stake_amount: u64, ctx: &mut TxContext) {
        let house_data = option::borrow(&game.house_data);
        assert!(stake_amount <= house_data.max_bet, EStakeTooHigh);
        assert!(stake_amount > house_data.min_bet, EStakeTooLow);
        assert!(guess == 0 || guess == 1, EGuessNot1Or0);
        assert!(coin::value(&stake_coin) >= stake_amount, EPlayerCoinNotEnoughBalance);
        game.epoch = tx_context::epoch(ctx);
        // Get a balance with the stake_amount value
        let stake = coin::into_balance(stake_coin);
        if (balance::value(&stake) > stake_amount) {
            let total_balance = balance::value(&stake);
            let to_return = balance::split(&mut stake, total_balance - stake_amount);
            // return the rest
            transfer::transfer(coin::from_balance(to_return, ctx), tx_context::sender(ctx));
        };
        let bet_data = BetData {
            stake: stake,
            guess
        };
        option::fill(&mut game.bet_data, bet_data);
        option::fill(&mut game.player, tx_context::sender(ctx));
    }

    public entry fun end_game(game: &mut Game, secret: vector<u8>, ctx: &mut TxContext) {
        // only house should be able to end the game
        assert!(game.house == tx_context::sender(ctx), ENotAllowedToEndGame);

        // house wants to cancel the current game (maybe forgot the secret)
        if (option::is_none(& game.bet_data)) {
            let msg = b"Game Canceled by House";
            let outcome = Outcome {
                secret: msg,
                guess: 2,
                player_won: false
            };
            option::fill(&mut game.outcome, outcome);

            // no bet placed, return the balance to house
            let HouseData {house_balance, min_bet: _, max_bet: _} = option::extract(&mut game.house_data);
            let house_coins = coin::from_balance(house_balance, ctx);
            transfer::transfer(house_coins, game.house);
        } else {
            let hash = sha3_256(secret);
            if (hash != digest::sha3_256_digest_to_bytes(&game.hashed_secret)) {
                let BetData {stake, guess:_} = option::extract(&mut game.bet_data);
                let HouseData {house_balance, min_bet: _, max_bet:_} = option::extract(&mut game.house_data);
                let player = option::extract(&mut game.player);
                // TODO: Maybe add some punishment for the house
                transfer::transfer(coin::from_balance(stake, ctx), player);
                transfer::transfer(coin::from_balance(house_balance, ctx), game.house);
                abort EHashAndSecretDontMatch
            };
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

    // usually called by player when house refuses to reveal
    public entry fun cancel_game(game: &mut Game, ctx: &mut TxContext) {
        // this can't be called on an ended game
        assert!(option::is_none<Outcome>(&game.outcome), EGameAlreadyEnded);
        // a bet has to have been placed
        assert!(option::is_some<BetData>(&game.bet_data), ECannotCancelBeforeBetting);
        // this call can only be called 2 epochs after the bet has been placed
        assert!(game.epoch + EpochsCancelAfter <= tx_context::epoch(ctx), ENotEnoughEpochsPassedToCancel);
        // if player has bet and house has not revealed 2 epochs later, pay player
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

    // helper functions

    fun pay_player(player: address, house: address, stake: Balance<SUI>, house_balance: Balance<SUI>, ctx: &mut TxContext) {
        // if bet is less than max_bet, return the difference to player A after paying the wins
         if (balance::value(&stake) < balance::value(&house_balance)) {
            let profit = balance::split(&mut house_balance, balance::value(&stake));
            // calculate the wins = profit + stake
            balance::join(&mut stake, profit);
            // pay the wins
            transfer::transfer(coin::from_balance(stake, ctx), player);
            // return the rest back to house
            transfer::transfer(coin::from_balance(house_balance, ctx), house);
        } else {
            // profit is the whole max_bet
            balance::join(&mut stake, house_balance);
            transfer::transfer(coin::from_balance(stake, ctx), player);
        };
    }
}