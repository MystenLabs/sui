// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This is a rewrite of TicTacToe using a completely different approach.
// In TicTacToe, since the game object is owned by the admin, the players are not
// able to directly mutate the gameboard. Hence each marker placement takes
// two transactions.
// In this implementation, we make the game object a shared object.
// Both players have access and can mutate the game object, and hence they
// can place markers directly in one transaction.
// In general, using shared object has an extra cost due to the fact
// that Sui needs to sequence the operations that mutate the shared object from
// different transactions. In this case however, since it is expected for players
// to take turns to place the marker, there won't be a significant overhead in practice.
// As we can see, by using shared object, the implementation is much
// simpler than the other implementation.
module games::shared_tic_tac_toe {
    use std::vector;

    use sui::object::{Self, ID, UID};
    use sui::event;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    // Game status
    const IN_PROGRESS: u8 = 0;
    const X_WIN: u8 = 1;
    const O_WIN: u8 = 2;
    const DRAW: u8 = 3;
    const FINAL_TURN: u8 = 8;


    // Mark type
    const MARK_EMPTY: u8 = 2;

    // Error codes
    /// Trying to place a mark when it's not your turn.
    const EInvalidTurn: u64 = 0;
    /// Trying to place a mark when the game has already ended.
    const EGameEnded: u64 = 1;
    /// Trying to place a mark in an invalid location, i.e. row/column out of bound.
    const EInvalidLocation: u64 = 2;
    /// The cell to place a new mark at is already oocupied.
    const ECellOccupied: u64 = 3;

    struct TicTacToe has key {
        id: UID,
        gameboard: vector<vector<u8>>,
        cur_turn: u8,
        game_status: u8,
        x_address: address,
        o_address: address,
    }

    struct Trophy has key {
        id: UID,
    }

    struct GameEndEvent has copy, drop {
        // The Object ID of the game object
        game_id: ID,
    }

    /// `x_address` and `o_address` are the account address of the two players.
    public entry fun create_game(x_address: address, o_address: address, ctx: &mut TxContext) {
        // TODO: Validate sender address, only GameAdmin can create games.

        let id = object::new(ctx);
        let gameboard = vector[
            vector[MARK_EMPTY, MARK_EMPTY, MARK_EMPTY],
            vector[MARK_EMPTY, MARK_EMPTY, MARK_EMPTY],
            vector[MARK_EMPTY, MARK_EMPTY, MARK_EMPTY],
        ];
        let game = TicTacToe {
            id,
            gameboard,
            cur_turn: 0,
            game_status: IN_PROGRESS,
            x_address: x_address,
            o_address: o_address,
        };
        // Make the game a shared object so that both players can mutate it.
        transfer::share_object(game);
    }

    public entry fun place_mark(game: &mut TicTacToe, row: u8, col: u8, ctx: &mut TxContext) {
        assert!(row < 3 && col < 3, EInvalidLocation);
        assert!(game.game_status == IN_PROGRESS, EGameEnded);
        let addr = get_cur_turn_address(game);
        assert!(addr == tx_context::sender(ctx), EInvalidTurn);

        let cell = vector::borrow_mut(vector::borrow_mut(&mut game.gameboard, (row as u64)), (col as u64));
        assert!(*cell == MARK_EMPTY, ECellOccupied);

        *cell = game.cur_turn % 2;
        update_winner(game);
        game.cur_turn = game.cur_turn + 1;

        if (game.game_status != IN_PROGRESS) {
            // Notify the server that the game ended so that it can delete the game.
            event::emit(GameEndEvent { game_id: object::id(game) });
            if (game.game_status == X_WIN) {
                transfer::transfer(Trophy { id: object::new(ctx) }, game.x_address);
            } else if (game.game_status == O_WIN) {
                transfer::transfer(Trophy { id: object::new(ctx) }, game.o_address);
            }
        }
    }

    public entry fun delete_game(game: TicTacToe) {
        let TicTacToe { id, gameboard: _, cur_turn: _, game_status: _, x_address: _, o_address: _ } = game;
        object::delete(id);
    }

    public entry fun delete_trophy(trophy: Trophy) {
        let Trophy { id } = trophy;
        object::delete(id);
    }

    public fun get_status(game: &TicTacToe): u8 {
        game.game_status
    }

    fun get_cur_turn_address(game: &TicTacToe): address {
        if (game.cur_turn % 2 == 0) {
            game.x_address
        } else {
            game.o_address
        }
    }

    fun get_cell(game: &TicTacToe, row: u64, col: u64): u8 {
        *vector::borrow(vector::borrow(&game.gameboard, row), col)
    }

    fun update_winner(game: &mut TicTacToe) {
        // Check all rows
        check_for_winner(game, 0, 0, 0, 1, 0, 2);
        check_for_winner(game, 1, 0, 1, 1, 1, 2);
        check_for_winner(game, 2, 0, 2, 1, 2, 2);

        // Check all columns
        check_for_winner(game, 0, 0, 1, 0, 2, 0);
        check_for_winner(game, 0, 1, 1, 1, 2, 1);
        check_for_winner(game, 0, 2, 1, 2, 2, 2);

        // Check diagonals
        check_for_winner(game, 0, 0, 1, 1, 2, 2);
        check_for_winner(game, 2, 0, 1, 1, 0, 2);

        // Check if we have a draw
        if (game.game_status == IN_PROGRESS && game.cur_turn == FINAL_TURN) {
            game.game_status = DRAW;
        };
    }

    fun check_for_winner(game: &mut TicTacToe, row1: u64, col1: u64, row2: u64, col2: u64, row3: u64, col3: u64) {
        if (game.game_status != IN_PROGRESS) {
            return
        };
        let result = get_winner_if_all_equal(game, row1, col1, row2, col2, row3, col3);
        if (result != MARK_EMPTY) {
            game.game_status = if (result == 0) X_WIN else O_WIN;
        };
    }

    fun get_winner_if_all_equal(game: &TicTacToe, row1: u64, col1: u64, row2: u64, col2: u64, row3: u64, col3: u64): u8 {
        let cell1 = get_cell(game, row1, col1);
        let cell2 = get_cell(game, row2, col2);
        let cell3 = get_cell(game, row3, col3);
        if (cell1 == cell2 && cell1 == cell3) cell1 else MARK_EMPTY
    }
}
