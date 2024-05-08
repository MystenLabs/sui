// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This is an implementation of the TicTacToe game.
// The game object (which includes gameboard) is owned by a game admin.
// Since players don't have ownership over the game object, they cannot
// mutate the gameboard directly. In order for each player to place
// a marker, they must first show their intention of placing a marker
// by creating a marker object with the placement information and send
// the marker to the admin. The admin needs to run a centralized service
// that monitors the marker placement events and respond do them.
// Upon receiving an event, the admin will attempt the place the new
// marker on the gameboard. This means that every marker placement operation
// always take two transactions, one by the player, and one by the admin.
// It also means that we need to trust the centralized service for liveness,
// i.e. the service is willing to make progress in the game.
// TicTacToeV2 shows a simpler way to implement this using shared objects,
// providing different trade-offs: using shared object is more expensive,
// however it eliminates the need of a centralized service.
module games::tic_tac_toe {
    use sui::event;

    // Game status
    const IN_PROGRESS: u8 = 0;
    const X_WIN: u8 = 1;
    const O_WIN: u8 = 2;
    const DRAW: u8 = 3;
    const FINAL_TURN: u8 = 8;

    // Error codes
    const EInvalidLocation: u64 = 0;
    const ENoMoreMark: u64 = 1;

    public struct TicTacToe has key {
        id: UID,
        gameboard: vector<vector<Option<Mark>>>,
        cur_turn: u8,
        game_status: u8,
        x_address: address,
        o_address: address,
    }

    public struct MarkMintCap has key {
        id: UID,
        game_id: ID,
        remaining_supply: u8,
    }

    public struct Mark has key, store {
        id: UID,
        player: address,
        row: u64,
        col: u64,
    }

    public struct Trophy has key {
        id: UID,
    }

    public struct MarkSentEvent has copy, drop {
        // The Object ID of the game object
        game_id: ID,
        // The object ID of the mark sent
        mark_id: ID,
    }

    public struct GameEndEvent has copy, drop {
        // The Object ID of the game object
        game_id: ID,
    }

    /// `x_address` and `o_address` are the account address of the two players.
    public entry fun create_game(x_address: address, o_address: address, ctx: &mut TxContext) {
        // TODO: Validate sender address, only GameAdmin can create games.

        let id = object::new(ctx);
        let game_id = id.to_inner();
        let gameboard = vector[
            vector[option::none(), option::none(), option::none()],
            vector[option::none(), option::none(), option::none()],
            vector[option::none(), option::none(), option::none()],
        ];
        let game = TicTacToe {
            id,
            gameboard,
            cur_turn: 0,
            game_status: IN_PROGRESS,
            x_address: x_address,
            o_address: o_address,
        };
        transfer::transfer(game, ctx.sender());
        let cap = MarkMintCap {
            id: object::new(ctx),
            game_id,
            remaining_supply: 5,
        };
        transfer::transfer(cap, x_address);
        let cap = MarkMintCap {
            id: object::new(ctx),
            game_id,
            remaining_supply: 5,
        };
        transfer::transfer(cap, o_address);
    }

    /// Generate a new mark intended for location (row, col).
    /// This new mark is not yet placed, just transferred to the game.
    public entry fun send_mark_to_game(
        cap: &mut MarkMintCap,
        game_address: address,
        row: u64,
        col: u64,
        ctx: &mut TxContext,
    ) {
        if (row > 2 || col > 2) {
            abort EInvalidLocation
        };
        let mark = mint_mark(cap, row, col, ctx);
        // Once an event is emitted, it should be observed by a game server.
        // The game server will then call `place_mark` to place this mark.
        event::emit(MarkSentEvent {
            game_id: *&cap.game_id,
            mark_id: object::id(&mark),
        });
        transfer::public_transfer(mark, game_address);
    }

    public entry fun place_mark(game: &mut TicTacToe, mark: Mark, ctx: &mut TxContext) {
        // If we are placing the mark at the wrong turn, or if game has ended,
        // destroy the mark.
        let addr = game.get_cur_turn_address();
        if (game.game_status != IN_PROGRESS || &addr != &mark.player) {
            mark.delete();
            return
        };
        let cell = get_cell_mut_ref(game, mark.row, mark.col);
        if (cell.is_some()) {
            // There is already a mark in the desired location.
            // Destroy the mark.
            mark.delete();
            return
        };
        cell.fill(mark);
        game.update_winner();
        game.cur_turn = game.cur_turn + 1;

        if (game.game_status != IN_PROGRESS) {
            // Notify the server that the game ended so that it can delete the game.
            event::emit(GameEndEvent { game_id: object::id(game) });
            if (game.game_status == X_WIN) {
                transfer::transfer(Trophy { id: object::new(ctx) }, *&game.x_address);
            } else if (game.game_status == O_WIN) {
                transfer::transfer(Trophy { id: object::new(ctx) }, *&game.o_address);
            }
        }
    }

    public entry fun delete_game(game: TicTacToe) {
        let TicTacToe { id, mut gameboard, cur_turn: _, game_status: _, x_address: _, o_address: _ } = game;
        while (gameboard.length() > 0) {
            let mut row = gameboard.pop_back();
            while (row.length() > 0) {
                let mut element = row.pop_back();
                if (element.is_some()) {
                    let mark = element.extract();
                    mark.delete();
                };
                element.destroy_none();
            };
            row.destroy_empty();
        };
        gameboard.destroy_empty();
        object::delete(id);
    }

    public entry fun delete_trophy(trophy: Trophy) {
        let Trophy { id } = trophy;
        object::delete(id);
    }

    public entry fun delete_cap(cap: MarkMintCap) {
        let MarkMintCap { id, game_id: _, remaining_supply: _ } = cap;
        object::delete(id);
    }

    public fun get_status(game: &TicTacToe): u8 {
        game.game_status
    }

    fun mint_mark(cap: &mut MarkMintCap, row: u64, col: u64, ctx: &mut TxContext): Mark {
        if (cap.remaining_supply == 0) {
            abort ENoMoreMark
        };
        cap.remaining_supply = cap.remaining_supply - 1;
        Mark {
            id: object::new(ctx),
            player: ctx.sender(),
            row,
            col,
        }
    }

    fun get_cur_turn_address(game: &TicTacToe): address {
        if (game.cur_turn % 2 == 0) {
            *&game.x_address
        } else {
            *&game.o_address
        }
    }

    fun get_cell_ref(game: &TicTacToe, row: u64, col: u64): &Option<Mark> {
        &game.gameboard[row][col]
    }

    fun get_cell_mut_ref(game: &mut TicTacToe, row: u64, col: u64): &mut Option<Mark> {
        &mut game.gameboard[row][col]
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
        let mut result = check_all_equal(game, row1, col1, row2, col2, row3, col3);
        if (result.is_some()) {
            let winner = result.extract();
            game.game_status = if (&winner == &game.x_address) {
                X_WIN
            } else {
                O_WIN
            };
        };
    }

    fun check_all_equal(game: &TicTacToe, row1: u64, col1: u64, row2: u64, col2: u64, row3: u64, col3: u64): Option<address> {
        let cell1 = get_cell_ref(game, row1, col1);
        let cell2 = get_cell_ref(game, row2, col2);
        let cell3 = get_cell_ref(game, row3, col3);
        if (cell1.is_some() && cell2.is_some() && cell3.is_some()) {
            let cell1_player = cell1.borrow().player;
            let cell2_player = cell2.borrow().player;
            let cell3_player = cell3.borrow().player;
            if (&cell1_player == &cell2_player && &cell1_player == &cell3_player) {
                return option::some(cell1_player)
            };
        };
        option::none()
    }

    use fun delete_mark as Mark.delete;

    fun delete_mark(mark: Mark) {
        let Mark { id, player: _, row: _, col: _ } = mark;
        object::delete(id);
    }

    public fun mark_player(mark: &Mark): &address {
        &mark.player
    }

    public fun mark_row(mark: &Mark): u64 {
        mark.row
    }

    public fun mark_col(mark: &Mark): u64 {
        mark.col
    }
}
