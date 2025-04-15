// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module tic_tac_toe::shared_tests;

use sui::test_scenario::{Self as ts, Scenario};
use tic_tac_toe::shared as ttt;

const ALICE: address = @0xA;
const BOB: address = @0xB;

const MARK__: u8 = 0;
const MARK_X: u8 = 1;
const MARK_O: u8 = 2;

const TROPHY_DRAW: u8 = 1;
const TROPHY_WIN: u8 = 2;

#[test]
fun x_wins() {
    let mut ts = ts::begin(ALICE);

    ttt::new(ALICE, BOB, ts.ctx());

    ts.place_mark(ALICE, 1, 1);
    // . . .
    // . X .
    // . . .

    ts.place_mark(BOB, 0, 0);
    // O . .
    // . X .
    // . . .

    ts.place_mark(ALICE, 0, 2);
    // O . X
    // . X .
    // . . .

    ts.place_mark(BOB, 1, 0);
    // O . X
    // O X .
    // . . .

    ts.place_mark(ALICE, 2, 0);
    // O . X
    // O X .
    // X . .

    ts.next_tx(ALICE);
    assert!(!ts::has_most_recent_for_address<ttt::Trophy>(BOB));

    let trophy: ttt::Trophy = ts.take_from_sender();
    assert!(trophy.other() == BOB);
    assert!(trophy.status() == TROPHY_WIN);
    assert!(trophy.turn() == 5);
    assert!(
        trophy.board() == vector[
            MARK_O, MARK__, MARK_X,
            MARK_O, MARK_X, MARK__,
            MARK_X, MARK__, MARK__,
        ],
    );

    ts.return_to_sender(trophy);

    let game: ttt::Game = ts.take_shared();
    game.burn();
    ts.end();
}

#[test]
fun o_wins() {
    let mut ts = ts::begin(ALICE);

    ttt::new(ALICE, BOB, ts.ctx());

    ts.place_mark(ALICE, 1, 1);
    // . . .
    // . X .
    // . . .

    ts.place_mark(BOB, 2, 2);
    // . . .
    // . X .
    // . . O

    ts.place_mark(ALICE, 0, 2);
    // . . X
    // . X .
    // . . O

    ts.place_mark(BOB, 2, 0);
    // . . X
    // . X .
    // O . O

    ts.place_mark(ALICE, 0, 0);
    // X . X
    // . X .
    // O . O

    ts.place_mark(BOB, 2, 1);
    // X . X
    // . X .
    // O O O

    ts.next_tx(BOB);
    assert!(!ts::has_most_recent_for_address<ttt::Trophy>(ALICE));

    let trophy: ttt::Trophy = ts.take_from_sender();
    assert!(trophy.other() == ALICE);
    assert!(trophy.status() == TROPHY_WIN);
    assert!(trophy.turn() == 6);
    assert!(
        trophy.board() == vector[
            MARK_X, MARK__, MARK_X,
            MARK__, MARK_X, MARK__,
            MARK_O, MARK_O, MARK_O,
        ],
    );

    ts.return_to_sender(trophy);

    let game: ttt::Game = ts.take_shared();
    game.burn();
    ts.end();
}

#[test]
fun draw() {
    let mut ts = ts::begin(ALICE);

    ttt::new(ALICE, BOB, ts.ctx());

    ts.place_mark(ALICE, 1, 1);
    // . . .
    // . X .
    // . . .

    ts.place_mark(BOB, 0, 0);
    // O . .
    // . X .
    // . . .

    ts.place_mark(ALICE, 0, 2);
    // O . X
    // . X .
    // . . .

    ts.place_mark(BOB, 2, 0);
    // O . X
    // . X .
    // O . .

    ts.place_mark(ALICE, 1, 0);
    // O . X
    // X X .
    // O . .

    ts.place_mark(BOB, 1, 2);
    // O . X
    // X X O
    // O . .

    ts.place_mark(ALICE, 0, 1);
    // O X X
    // X X O
    // O . .

    ts.place_mark(BOB, 2, 1);
    // O X X
    // X X O
    // O O .

    ts.place_mark(ALICE, 2, 2);
    // O X X
    // X X O
    // O O X

    ts.next_tx(ALICE);

    let trophy: ttt::Trophy = ts.take_from_sender();
    assert!(trophy.other() == BOB);
    assert!(trophy.status() == TROPHY_DRAW);
    assert!(trophy.turn() == 9);
    assert!(
        trophy.board() == vector[
            MARK_O, MARK_X, MARK_X,
            MARK_X, MARK_X, MARK_O,
            MARK_O, MARK_O, MARK_X,
        ],
    );

    ts.return_to_sender(trophy);
    ts.next_tx(BOB);

    let trophy: ttt::Trophy = ts.take_from_sender();
    assert!(trophy.other() == ALICE);
    assert!(trophy.status() == TROPHY_DRAW);
    assert!(trophy.turn() == 9);
    assert!(
        trophy.board() == vector[
            MARK_O, MARK_X, MARK_X,
            MARK_X, MARK_X, MARK_O,
            MARK_O, MARK_O, MARK_X,
        ],
    );

    ts.return_to_sender(trophy);

    let game: ttt::Game = ts.take_shared();
    game.burn();
    ts.end();
}

#[test]
#[expected_failure(abort_code = ttt::EWrongPlayer)]
/// Moves from the wrong player are rejected
fun wrong_player() {
    let mut ts = ts::begin(ALICE);

    ttt::new(ALICE, BOB, ts.ctx());
    ts.place_mark(BOB, 0, 0);
    abort 0
}

#[test]
#[expected_failure(abort_code = ttt::EWrongPlayer)]
/// Moves from a player not in the game are rejected
fun random_player() {
    let mut ts = ts::begin(ALICE);

    ttt::new(ALICE, BOB, ts.ctx());
    ts.place_mark(@0xC, 0, 0);
    abort 0
}

#[test]
#[expected_failure(abort_code = ttt::EInvalidLocation)]
fun location_out_of_bounds() {
    let mut ts = ts::begin(ALICE);

    ttt::new(ALICE, BOB, ts.ctx());
    ts.place_mark(ALICE, 3, 3);
    abort 0
}

#[test]
#[expected_failure(abort_code = ttt::EAlreadyFilled)]
/// When a position is already marked, the turn cap is returned to
/// the player who made the "false" move, rather than the next
/// player.
fun already_marked() {
    let mut ts = ts::begin(ALICE);

    ttt::new(ALICE, BOB, ts.ctx());

    ts.place_mark(ALICE, 1, 1);
    ts.place_mark(BOB, 1, 1);
    abort 0
}

#[test]
#[expected_failure(abort_code = ttt::EAlreadyFinished)]
fun already_finished() {
    let mut ts = ts::begin(ALICE);

    ttt::new(ALICE, BOB, ts.ctx());

    ts.place_mark(ALICE, 1, 1);
    // . . .
    // . X .
    // . . .

    ts.place_mark(BOB, 0, 0);
    // O . .
    // . X .
    // . . .

    ts.place_mark(ALICE, 0, 2);
    // O . X
    // . X .
    // . . .

    ts.place_mark(BOB, 1, 0);
    // O . X
    // O X .
    // . . .

    ts.place_mark(ALICE, 2, 0);
    // O . X
    // O X .
    // X . .

    // Shouldn't work because the game has already finished.
    ts.place_mark(BOB, 2, 0);
    // O . X
    // O X .
    // X . O

    abort 0
}

#[test]
#[expected_failure(abort_code = ttt::ENotFinished)]
fun burn_unfinished_game() {
    let mut ts = ts::begin(ALICE);

    ttt::new(ALICE, BOB, ts.ctx());
    ts.place_mark(ALICE, 1, 1);

    ts.next_tx(BOB);
    let game: ttt::Game = ts.take_shared();
    game.burn();
    abort 0
}

// === Test Helpers ===
use fun place_mark as Scenario.place_mark;

// The current player places a mark at the given location.
fun place_mark(ts: &mut Scenario, player: address, row: u8, col: u8) {
    ts.next_tx(player);

    let mut game: ttt::Game = ts.take_shared();
    game.place_mark(row, col, ts.ctx());
    ts::return_shared(game);
}
