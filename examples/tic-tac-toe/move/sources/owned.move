// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An implementation of Tic Tac Toe, using owned objects.
///
/// The `Game` object is owned by an admin, so players cannot mutate the game
/// board directly. Instead, they convey their intention to place a mark by
/// transferring a `Mark` object to the `Game`.
///
/// This means that every move takes two owned object fast path operations --
/// one by the player, and one by the admin. The admin could be a third party
/// running a centralized service that monitors marker placement events and
/// responds to them, or it could be a 1-of-2 multisig address shared between
/// the two players, as demonstrated in the demo app.
///
/// The `shared` module shows a variant of this game implemented using shared
/// objects, which provides different trade-offs: Using shared objects is more
/// expensive, however the implementation is more straightforward and each move
/// only requires one transaction.
module tic_tac_toe::owned;

use sui::{event, transfer::Receiving};

// === Object Types ===

/// The state of an active game of tic-tac-toe.
public struct Game has key, store {
    id: UID,
    /// Marks on the board.
    board: vector<u8>,
    /// The next turn to be played.
    turn: u8,
    /// The address expected to send moves on behalf of X.
    x: address,
    /// The address expected to send moves on behalf of O.
    o: address,
    /// Public key of the admin address.
    admin: vector<u8>,
}

/// The player that the next turn is expected from is given a `TurnCap`.
public struct TurnCap has key {
    id: UID,
    game: ID,
}

/// A request to make a play -- only the player with the `TurnCap` can
/// create and send `Mark`s.
public struct Mark has key, store {
    id: UID,
    player: address,
    row: u8,
    col: u8,
}

/// An NFT representing a finished game. Sent to the winning player if there
/// is one, or to both players in the case of a draw.
public struct Trophy has key {
    id: UID,
    /// Whether the game was won or drawn.
    status: u8,
    /// The state of the board at the end of the game.
    board: vector<u8>,
    /// The number of turns played
    turn: u8,
    /// The other player (relative to the player who owns this Trophy).
    other: address,
}

// === Event Types ===

public struct MarkSent has copy, drop {
    game: ID,
    mark: ID,
}

public struct GameEnd has copy, drop {
    game: ID,
}

// === Constants ===

// Marks
const MARK__: u8 = 0;
const MARK_X: u8 = 1;
const MARK_O: u8 = 2;

// Trophy status
const TROPHY_NONE: u8 = 0;
const TROPHY_DRAW: u8 = 1;
const TROPHY_WIN: u8 = 2;

// === Errors ===

#[error]
const EInvalidLocation: vector<u8> = b"Move was for a position that doesn't exist on the board";

#[error]
const EWrongPlayer: vector<u8> = b"Game expected a move from another player";

#[error]
const ENotFinished: vector<u8> = b"Game has not reached an end condition";

#[error]
const EAlreadyFinished: vector<u8> = b"Can't place a mark on a finished game";

#[error]
const EInvalidEndState: vector<u8> = b"Game reached an end state that wasn't expected";

// === Public Functions ===

/// Create a new game, played by `x` and `o`. The game should be
/// transfered to the address that will administrate the game. If
/// that address is a multi-sig of the two players, its public key
/// should be passed as `admin`.
public fun new(x: address, o: address, admin: vector<u8>, ctx: &mut TxContext): Game {
    let game = Game {
        id: object::new(ctx),
        board: vector[MARK__, MARK__, MARK__, MARK__, MARK__, MARK__, MARK__, MARK__, MARK__],
        turn: 0,
        x,
        o,
        admin,
    };

    let turn = TurnCap {
        id: object::new(ctx),
        game: object::id(&game),
    };

    // X is the first player, so send the capability to them.
    transfer::transfer(turn, x);
    game
}

/// Called by the active player to express their intention to make a move.
/// This consumes the `TurnCap` to prevent a player from making more than
/// one move on their turn.
public fun send_mark(cap: TurnCap, row: u8, col: u8, ctx: &mut TxContext) {
    assert!(row < 3 && col < 3, EInvalidLocation);

    let TurnCap { id, game } = cap;
    id.delete();

    let mark = Mark {
        id: object::new(ctx),
        player: ctx.sender(),
        row,
        col,
    };

    event::emit(MarkSent { game, mark: object::id(&mark) });
    transfer::transfer(mark, game.to_address());
}

/// Called by the admin (who owns the `Game`), to commit a player's
/// intention to make a move. If the game should end, `Trophy`s are sent to
/// the appropriate players, if the game should continue, a new `TurnCap` is
/// sent to the player who should make the next move.
public fun place_mark(game: &mut Game, mark: Receiving<Mark>, ctx: &mut TxContext) {
    assert!(game.ended() == TROPHY_NONE, EAlreadyFinished);

    // Fetch the mark on behalf of the game -- only works if the mark in
    // question was sent to this game.
    let Mark { id, row, col, player } = transfer::receive(&mut game.id, mark);
    id.delete();

    // Confirm that the mark is from the player we expect -- it should not
    // be possible to hit this assertion, because the `Mark`s can only be
    // created by the address that owns the `TurnCap` which cannot be
    // transferred, and is always held by `game.next_player()`.
    let (me, them, sentinel) = game.next_player();
    assert!(me == player, EWrongPlayer);

    if (game[row, col] == MARK__) {
        *(&mut game[row, col]) = sentinel;
        game.turn = game.turn + 1;
    };

    // Check win condition -- if there is a winner, send them the trophy,
    // otherwise, create a new turn cap and send that to the next player.
    let end = game.ended();
    if (end == TROPHY_WIN) {
        transfer::transfer(game.mint_trophy(end, them, ctx), me);
        event::emit(GameEnd { game: object::id(game) });
    } else if (end == TROPHY_DRAW) {
        transfer::transfer(game.mint_trophy(end, them, ctx), me);
        transfer::transfer(game.mint_trophy(end, me, ctx), them);
        event::emit(GameEnd { game: object::id(game) });
    } else if (end == TROPHY_NONE) {
        let cap = TurnCap { id: object::new(ctx), game: object::id(game) };
        let (to, _, _) = game.next_player();
        transfer::transfer(cap, to);
    } else {
        abort EInvalidEndState
    }
}

public fun burn(game: Game) {
    assert!(game.ended() != TROPHY_NONE, ENotFinished);
    let Game { id, .. } = game;
    id.delete();
}

/// Test whether the game has reached an end condition or not.
public fun ended(game: &Game): u8 {
    if (// Test rows
        test_triple(game, 0, 1, 2) ||
            test_triple(game, 3, 4, 5) ||
            test_triple(game, 6, 7, 8) ||
            // Test columns
            test_triple(game, 0, 3, 6) ||
            test_triple(game, 1, 4, 7) ||
            test_triple(game, 2, 5, 8) ||
            // Test diagonals
            test_triple(game, 0, 4, 8) ||
            test_triple(game, 2, 4, 6)) {
        TROPHY_WIN
    } else if (game.turn == 9) {
        TROPHY_DRAW
    } else {
        TROPHY_NONE
    }
}

#[syntax(index)]
public fun mark(game: &Game, row: u8, col: u8): &u8 {
    &game.board[(row * 3 + col) as u64]
}

#[syntax(index)]
fun mark_mut(game: &mut Game, row: u8, col: u8): &mut u8 {
    &mut game.board[(row * 3 + col) as u64]
}

// === Private Helpers ===

/// Address of the player the move is expected from, the address of the
/// other player, and the mark to use for the upcoming move.
fun next_player(game: &Game): (address, address, u8) {
    if (game.turn % 2 == 0) {
        (game.x, game.o, MARK_X)
    } else {
        (game.o, game.x, MARK_O)
    }
}

/// Test whether the values at the triple of positions all match each other
/// (and are not all EMPTY).
fun test_triple(game: &Game, x: u8, y: u8, z: u8): bool {
    let x = game.board[x as u64];
    let y = game.board[y as u64];
    let z = game.board[z as u64];

    MARK__ != x && x == y && y == z
}

/// Create a trophy from the current state of the `game`, that indicates
/// that a player won or drew against `other` player.
fun mint_trophy(game: &Game, status: u8, other: address, ctx: &mut TxContext): Trophy {
    Trophy {
        id: object::new(ctx),
        status,
        board: game.board,
        turn: game.turn,
        other,
    }
}

// === Test Helpers ===
#[test_only]
public use fun game_board as Game.board;
#[test_only]
public use fun trophy_status as Trophy.status;
#[test_only]
public use fun trophy_board as Trophy.board;
#[test_only]
public use fun trophy_turn as Trophy.turn;
#[test_only]
public use fun trophy_other as Trophy.other;

#[test_only]
public fun game_board(game: &Game): vector<u8> {
    game.board
}

#[test_only]
public fun trophy_status(trophy: &Trophy): u8 {
    trophy.status
}

#[test_only]
public fun trophy_board(trophy: &Trophy): vector<u8> {
    trophy.board
}

#[test_only]
public fun trophy_turn(trophy: &Trophy): u8 {
    trophy.turn
}

#[test_only]
public fun trophy_other(trophy: &Trophy): address {
    trophy.other
}
