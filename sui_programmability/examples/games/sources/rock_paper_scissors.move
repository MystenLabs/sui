// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This is an idea of a module which will allow some asset to be
// won by playing a rock-paper-scissors (then lizard-spoke) game.
//
// Initial implementation implies so-called commit-reveal scheme
// in which players first submit their commitments
// and then reveal the data that led to these commitments. The
// data is then being verified by one of the parties or a third
// party (depends on implementation and security measures).
//
// In this specific example, the flow is:
//   1. User A creates a Game struct, where he puts a prize asset
//   2. Both users B and C submit their hashes to the game as their
// guesses but don't reveal the actual values yet
//   3. Users B and C submit their salts, so the user A
// can see and prove that the values match, and decides who won the
// round. Asset is then released to the winner or to the game owner
// if nobody won.
//
// TODO:
// - Error codes
// - Status checks
// - If player never revealed the secret
// - If game owner never took or revealed the results (incentives?)

module games::rock_paper_scissors {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use std::vector;
    use std::hash;

    // -- Gestures and additional consts -- //

    const NONE: u8 = 0;
    const ROCK: u8 = 1;
    const PAPER: u8 = 2;
    const SCISSORS: u8 = 3;
    const CHEAT: u8 = 111;

    public fun rock(): u8 { ROCK }
    public fun paper(): u8 { PAPER }
    public fun scissors(): u8 { SCISSORS }

    // -- Game statuses list -- //

    const STATUS_READY: u8 = 0;
    const STATUS_HASH_SUBMISSION: u8 = 1;
    const STATUS_HASHES_SUBMITTED: u8 = 2;
    const STATUS_REVEALING: u8 = 3;
    const STATUS_REVEALED: u8 = 4;

    /// The Prize that's being held inside the [`Game`] object. Should be
    /// eventually replaced with some generic T inside the [`Game`].
    struct ThePrize has key, store {
        id: UID
    }

    /// The main resource of the rock_paper_scissors module. Contains all the
    /// information about the game state submitted by both players. By default
    /// contains empty values and fills as the game progresses.
    /// Being destroyed in the end, once [`select_winner`] is called and the game
    /// has reached its final state by that time.
    struct Game has key {
        id: UID,
        prize: ThePrize,
        player_one: address,
        player_two: address,
        hash_one: vector<u8>,
        hash_two: vector<u8>,
        gesture_one: u8,
        gesture_two: u8,
    }

    /// Hashed gesture. It is not reveal-able until both players have
    /// submitted their moves to the Game. The turn is passed to the
    /// game owner who then adds a hash to the Game object.
    struct PlayerTurn has key {
        id: UID,
        hash: vector<u8>,
        player: address,
    }

    /// Secret object which is used to reveal the move. Just like [`PlayerTurn`]
    /// it is used to reveal the actual gesture a player has submitted.
    struct Secret has key {
        id: UID,
        salt: vector<u8>,
        player: address,
    }

    /// Shows the current game status. This function is also used in the [`select_winner`]
    /// entry point and limits the ability to select a winner, if one of the secrets hasn't
    /// been revealed yet.
    public fun status(game: &Game): u8 {
        let h1_len = vector::length(&game.hash_one);
        let h2_len = vector::length(&game.hash_two);

        if (game.gesture_one != NONE && game.gesture_two != NONE) {
            STATUS_REVEALED
        } else if (game.gesture_one != NONE || game.gesture_two != NONE) {
            STATUS_REVEALING
        } else if (h1_len == 0 && h2_len == 0) {
            STATUS_READY
        } else if (h1_len != 0 && h2_len != 0) {
            STATUS_HASHES_SUBMITTED
        } else if (h1_len != 0 || h2_len != 0) {
            STATUS_HASH_SUBMISSION
        } else {
            0
        }
    }

    /// Start a new game at sender address. The only arguments needed are players, the rest
    /// is initiated with default/empty values which will be filled later in the game.
    ///
    /// todo: extend with generics + T as prize
    public entry fun new_game(player_one: address, player_two: address, ctx: &mut TxContext) {
        transfer::transfer(Game {
            id: object::new(ctx),
            prize: ThePrize { id: object::new(ctx) },
            player_one,
            player_two,
            hash_one: vector[],
            hash_two: vector[],
            gesture_one: NONE,
            gesture_two: NONE,
        }, tx_context::sender(ctx));
    }

    /// Transfer [`PlayerTurn`] to the game owner. Nobody at this point knows what move
    /// is encoded inside the [`hash`] argument.
    ///
    /// Currently there's no check on whether the game exists.
    public entry fun player_turn(at: address, hash: vector<u8>, ctx: &mut TxContext) {
        transfer::transfer(PlayerTurn {
            hash,
            id: object::new(ctx),
            player: tx_context::sender(ctx),
        }, at);
    }

    /// Add a hashed gesture to the game. Store it as a `hash_one` or `hash_two` depending
    /// on the player number (one or two)
    public entry fun add_hash(game: &mut Game, cap: PlayerTurn) {
        let PlayerTurn { hash, id, player } = cap;
        let status = status(game);

        assert!(status == STATUS_HASH_SUBMISSION || status == STATUS_READY, 0);
        assert!(game.player_one == player || game.player_two == player, 0);

        if (player == game.player_one && vector::length(&game.hash_one) == 0) {
            game.hash_one = hash;
        } else if (player == game.player_two && vector::length(&game.hash_two) == 0) {
            game.hash_two = hash;
        } else {
            abort 0 // unreachable!()
        };

        object::delete(id);
    }

    /// Submit a [`Secret`] to the game owner who then matches the hash and saves the
    /// gesture in the [`Game`] object.
    public entry fun reveal(at: address, salt: vector<u8>, ctx: &mut TxContext) {
        transfer::transfer(Secret {
            id: object::new(ctx),
            salt,
            player: tx_context::sender(ctx),
        }, at);
    }

    /// Use submitted [`Secret`]'s salt to find the gesture played by the player and set it
    /// in the [`Game`] object.
    /// TODO: think of ways to
    public entry fun match_secret(game: &mut Game, secret: Secret) {
        let Secret { salt, player, id } = secret;

        assert!(player == game.player_one || player == game.player_two, 0);

        if (player == game.player_one) {
            game.gesture_one = find_gesture(salt, &game.hash_one);
        } else if (player == game.player_two) {
            game.gesture_two = find_gesture(salt, &game.hash_two);
        };

        object::delete(id);
    }

    /// The final accord to the game logic. After both secrets have been revealed,
    /// the game owner can choose a winner and release the prize.
    public entry fun select_winner(game: Game, ctx: &TxContext) {
        assert!(status(&game) == STATUS_REVEALED, 0);

        let Game {
            id,
            prize,
            player_one,
            player_two,
            hash_one: _,
            hash_two: _,
            gesture_one,
            gesture_two,
        } = game;

        let p1_wins = play(gesture_one, gesture_two);
        let p2_wins = play(gesture_two, gesture_one);

        object::delete(id);

        // If one of the players wins, he takes the prize.
        // If there's a tie, the game owner gets the prize.
        if (p1_wins) {
            transfer::public_transfer(prize, player_one)
        } else if (p2_wins) {
            transfer::public_transfer(prize, player_two)
        } else {
            transfer::public_transfer(prize, tx_context::sender(ctx))
        };
    }

    /// Implement the basic logic of the game.
    fun play(one: u8, two: u8): bool {
        if (one == ROCK && two == SCISSORS) { true }
        else if (one == PAPER && two == ROCK) { true }
        else if (one == SCISSORS && two == PAPER) { true }
        else if (one != CHEAT && two == CHEAT) { true }
        else { false }
    }

    /// Hash the salt and the gesture_id and match it against the stored hash. If something
    /// matches, the gesture_id is returned, if nothing - player is considered a cheater, and
    /// he automatically loses the round.
    fun find_gesture(salt: vector<u8>, hash: &vector<u8>): u8 {
        if (hash(ROCK, salt) == *hash) {
            ROCK
        } else if (hash(PAPER, salt) == *hash) {
            PAPER
        } else if (hash(SCISSORS, salt) == *hash) {
            SCISSORS
        } else {
            CHEAT
        }
    }

    /// Internal hashing function to build a [`Secret`] and match it later at the reveal stage.
    ///
    /// - `salt` argument here is a secret that is only known to the sender. That way we ensure
    /// that nobody knows the gesture until the end, but at the same time each player commits
    /// to the result with his hash;
    fun hash(gesture: u8, salt: vector<u8>): vector<u8> {
        vector::push_back(&mut salt, gesture);
        hash::sha2_256(salt)
    }
}
