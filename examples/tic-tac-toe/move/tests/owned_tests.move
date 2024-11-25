// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module tic_tac_toe::owned_tests;

use sui::{test_scenario::{Self as ts, Scenario}, transfer::Receiving};
use tic_tac_toe::owned as ttt;

const ADMIN: address = @0xAD;
const ALICE: address = @0xA;
const BOB: address = @0xB;

// Dummy key -- this field is only relevant off-chain.
const KEY: vector<u8> = vector[];

const MARK__: u8 = 0;
const MARK_X: u8 = 1;
const MARK_O: u8 = 2;

const TROPHY_DRAW: u8 = 1;
const TROPHY_WIN: u8 = 2;

#[test]
fun x_wins() {
    let mut ts = ts::begin(ADMIN);

    let game = ttt::new(ALICE, BOB, KEY, ts.ctx());
    transfer::public_transfer(game, ADMIN);

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
    assert!(!ts::has_most_recent_for_address<ttt::TurnCap>(ALICE));
    assert!(!ts::has_most_recent_for_address<ttt::TurnCap>(BOB));

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

    ts.next_tx(ADMIN);
    let game: ttt::Game = ts.take_from_sender();
    game.burn();

    ts.end();
}

#[test]
fun o_wins() {
    let mut ts = ts::begin(ADMIN);

    let game = ttt::new(ALICE, BOB, KEY, ts.ctx());
    transfer::public_transfer(game, ADMIN);

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
    assert!(!ts::has_most_recent_for_address<ttt::TurnCap>(ALICE));
    assert!(!ts::has_most_recent_for_address<ttt::TurnCap>(BOB));

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

    ts.next_tx(ADMIN);
    let game: ttt::Game = ts.take_from_sender();
    game.burn();

    ts.end();
}

#[test]
fun draw() {
    let mut ts = ts::begin(ADMIN);

    let game = ttt::new(ALICE, BOB, KEY, ts.ctx());
    transfer::public_transfer(game, ADMIN);

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
    assert!(!ts::has_most_recent_for_address<ttt::TurnCap>(ALICE));
    assert!(!ts::has_most_recent_for_address<ttt::TurnCap>(BOB));

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

    ts.next_tx(ADMIN);
    let game: ttt::Game = ts.take_from_sender();
    game.burn();

    ts.end();
}

#[test]
/// Only one player has the TurnCap at any one time.
fun turn_cap_conservation() {
    let mut ts = ts::begin(ADMIN);

    let game = ttt::new(ALICE, BOB, KEY, ts.ctx());
    transfer::public_transfer(game, ADMIN);

    ts.next_tx(ADMIN);
    assert!(ts::has_most_recent_for_address<ttt::TurnCap>(ALICE));
    assert!(!ts::has_most_recent_for_address<ttt::TurnCap>(BOB));

    ts.place_mark(ALICE, 1, 1);
    ts.next_tx(ADMIN);
    assert!(!ts::has_most_recent_for_address<ttt::TurnCap>(ALICE));
    assert!(ts::has_most_recent_for_address<ttt::TurnCap>(BOB));

    ts.end();
}

#[test]
#[expected_failure(abort_code = ttt::EInvalidLocation)]
fun location_out_of_bounds() {
    let mut ts = ts::begin(ADMIN);

    let game = ttt::new(ALICE, BOB, KEY, ts.ctx());
    transfer::public_transfer(game, ADMIN);

    ts.place_mark(ALICE, 3, 3);
    abort 0
}

#[test]
/// When a position is already marked, the turn cap is returned to
/// the player who made the "false" move, rather than the next
/// player.
fun already_marked() {
    let mut ts = ts::begin(ADMIN);

    let game = ttt::new(ALICE, BOB, KEY, ts.ctx());
    transfer::public_transfer(game, ADMIN);

    ts.place_mark(ALICE, 1, 1);
    ts.place_mark(BOB, 1, 1);

    ts.next_tx(ADMIN);
    assert!(ts::has_most_recent_for_address<ttt::TurnCap>(BOB));
    assert!(!ts::has_most_recent_for_address<ttt::TurnCap>(ALICE));

    let game: ttt::Game = ts.take_from_sender();
    assert!(
        game.board() == vector[
            MARK__, MARK__, MARK__,
            MARK__, MARK_X, MARK__,
            MARK__, MARK__, MARK__,
        ],
    );

    ts.return_to_sender(game);
    ts.end();
}

#[test]
#[expected_failure(abort_code = ttt::ENotFinished)]
fun burn_unfinished_game() {
    let mut ts = ts::begin(ADMIN);

    let game = ttt::new(ALICE, BOB, KEY, ts.ctx());
    transfer::public_transfer(game, ADMIN);

    ts.place_mark(ALICE, 1, 1);

    ts.next_tx(ADMIN);
    let game: ttt::Game = ts.take_from_sender();

    game.burn();
    abort 0
}

// === Test Helpers ===
use fun place_mark as Scenario.place_mark;

// The current player places a mark at the given location.
fun place_mark(ts: &mut Scenario, player: address, row: u8, col: u8) {
    ts.next_tx(player);

    let cap: ttt::TurnCap = ts.take_from_sender();
    cap.send_mark(row, col, ts.ctx());

    ts.next_tx(ADMIN);
    let mut game: ttt::Game = ts.take_from_sender();
    let rcv: Receiving<ttt::Mark> = ts::most_recent_receiving_ticket(&object::id(&game));

    game.place_mark(rcv, ts.ctx());
    ts.return_to_sender(game);
}
