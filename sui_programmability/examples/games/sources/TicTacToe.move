// This is an implementation of the TicTacToe game.
// The game object (which includes gameboard) is owned by a game admin.
// Since players don't have ownership over the game object, they cannot
// mutate the gameboard directly. In order for each plaer to place
// a marker, they must first show their intention of placing a marker
// by creating a marker object with the placement information and send
// the marker to the admin. The admin needs to run a centralized cervice
// that monitors the marker placement events and respond do them.
// Upon receiving an event, the admin will attempt the place the new
// marker on the gameboard. This means that every marker placement operation
// always take two transactions, one by the player, and one by the admin.
// It also means that we need to trust the centralized service for liveness,
// i.e. the service is willing to make progress in the game.
// TicTacToeV2 shows a simpler way to implement this using shared objects,
// providing different trade-offs: using shared object is more expensive,
// however it eliminates the need of a centralized service.
module Games::TicTacToe {
    use Std::Option::{Self, Option};
    use Std::Vector;

    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Event;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    // Game status
    const IN_PROGRESS: u8 = 0;
    const X_WIN: u8 = 1;
    const O_WIN: u8 = 2;
    const DRAW: u8 = 3;

    // Error codes
    const INVALID_LOCATION: u64 = 0;
    const NO_MORE_MARK: u64 = 1;

    struct TicTacToe has key {
        id: VersionedID,
        gameboard: vector<vector<Option<Mark>>>,
        cur_turn: u8,
        game_status: u8,
        x_address: address,
        o_address: address,
    }

    struct MarkMintCap has key {
        id: VersionedID,
        game_id: ID,
        remaining_supply: u8,
    }

    struct Mark has key, store {
        id: VersionedID,
        player: address,
        row: u64,
        col: u64,
    }

    struct Trophy has key {
        id: VersionedID,
    }

    struct MarkSentEvent has copy, drop {
        // The Object ID of the game object
        game_id: ID,
        // The object ID of the mark sent
        mark_id: ID,
    }

    struct GameEndEvent has copy, drop {
        // The Object ID of the game object
        game_id: ID,
    }

    /// `x_address` and `o_address` are the account address of the two players.
    public fun create_game(x_address: address, o_address: address, ctx: &mut TxContext) {
        // TODO: Validate sender address, only GameAdmin can create games.

        let id = TxContext::new_id(ctx);
        let game_id = *ID::inner(&id);
        let gameboard = vector[
            vector[Option::none(), Option::none(), Option::none()],
            vector[Option::none(), Option::none(), Option::none()],
            vector[Option::none(), Option::none(), Option::none()],
        ];
        let game = TicTacToe {
            id,
            gameboard,
            cur_turn: 0,
            game_status: IN_PROGRESS,
            x_address: x_address,
            o_address: o_address,
        };
        Transfer::transfer(game, TxContext::sender(ctx));
        let cap = MarkMintCap {
            id: TxContext::new_id(ctx),
            game_id: copy game_id,
            remaining_supply: 5,
        };
        Transfer::transfer(cap, x_address);
        let cap = MarkMintCap {
            id: TxContext::new_id(ctx),
            game_id,
            remaining_supply: 5,
        };
        Transfer::transfer(cap, o_address);
    }

    /// Generate a new mark intended for location (row, col).
    /// This new mark is not yet placed, just transferred to the game.
    public fun send_mark_to_game(cap: &mut MarkMintCap, game_address: address, row: u64, col: u64, ctx: &mut TxContext) {
        if (row > 2 || col > 2) {
            abort INVALID_LOCATION
        };
        let mark = mint_mark(cap, row, col, ctx);
        // Once an event is emitted, it should be observed by a game server.
        // The game server will then call `place_mark` to place this mark.
        Event::emit(MarkSentEvent {
            game_id: *&cap.game_id,
            mark_id: *ID::inner(&mark.id),
        });
        Transfer::transfer(mark, game_address);
    }

    public fun place_mark(game: &mut TicTacToe, mark: Mark, ctx: &mut TxContext) {
        // If we are placing the mark at the wrong turn, or if game has ended,
        // destroy the mark.
        let addr = get_cur_turn_address(game);
        if (game.game_status != IN_PROGRESS || &addr != &mark.player) {
            delete_mark(mark);
            return
        };
        let cell = get_cell_mut_ref(game, mark.row, mark.col);
        if (Option::is_some(cell)) {
            // There is already a mark in the desired location.
            // Destroy the mark.
            delete_mark(mark);
            return
        };
        Option::fill(cell, mark);
        update_winner(game);
        game.cur_turn = game.cur_turn + 1;

        if (game.game_status != IN_PROGRESS) {
            // Notify the server that the game ended so that it can delete the game.
            Event::emit(GameEndEvent { game_id: *ID::inner(&game.id) });
            if (game.game_status == X_WIN) {
                Transfer::transfer( Trophy { id: TxContext::new_id(ctx) }, *&game.x_address);
            } else if (game.game_status == O_WIN) {
                Transfer::transfer( Trophy { id: TxContext::new_id(ctx) }, *&game.o_address);
            }
        }
    }

    public fun delete_game(game: TicTacToe, _ctx: &mut TxContext) {
        let TicTacToe { id, gameboard, cur_turn: _, game_status: _, x_address: _, o_address: _ } = game;
        while (Vector::length(&gameboard) > 0) {
            let row = Vector::pop_back(&mut gameboard);
            while (Vector::length(&row) > 0) {
                let element = Vector::pop_back(&mut row);
                if (Option::is_some(&element)) {
                    let mark = Option::extract(&mut element);
                    delete_mark(mark);
                };
                Option::destroy_none(element);
            };
            Vector::destroy_empty(row);
        };
        Vector::destroy_empty(gameboard);
        ID::delete(id);
    }

    public fun delete_trophy(trophy: Trophy, _ctx: &mut TxContext) {
        let Trophy { id } = trophy;
        ID::delete(id);
    }

    public fun delete_cap(cap: MarkMintCap, _ctx: &mut TxContext) {
        let MarkMintCap { id, game_id: _, remaining_supply: _ } = cap;
        ID::delete(id);
    }

    public fun get_status(game: &TicTacToe): u8 {
        game.game_status
    }

    fun mint_mark(cap: &mut MarkMintCap, row: u64, col: u64, ctx: &mut TxContext): Mark {
        if (cap.remaining_supply == 0) {
            abort NO_MORE_MARK
        };
        cap.remaining_supply = cap.remaining_supply - 1;
        Mark {
            id: TxContext::new_id(ctx),
            player: TxContext::sender(ctx),
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
        Vector::borrow(Vector::borrow(&game.gameboard, row), col)
    }

    fun get_cell_mut_ref(game: &mut TicTacToe, row: u64, col: u64): &mut Option<Mark> {
        Vector::borrow_mut(Vector::borrow_mut(&mut game.gameboard, row), col)
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
        if (game.game_status != IN_PROGRESS && game.cur_turn == 9) {
            game.game_status = DRAW;
        };
    }

    fun check_for_winner(game: &mut TicTacToe, row1: u64, col1: u64, row2: u64, col2: u64, row3: u64, col3: u64) {
        if (game.game_status != IN_PROGRESS) {
            return
        };
        let result = check_all_equal(game, row1, col1, row2, col2, row3, col3);
        if (Option::is_some(&result)) {
            let winner = Option::extract(&mut result);
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
        if (Option::is_some(cell1) && Option::is_some(cell2) && Option::is_some(cell3)) {
            let cell1_player = *&Option::borrow(cell1).player;
            let cell2_player = *&Option::borrow(cell2).player;
            let cell3_player = *&Option::borrow(cell3).player;
            if (&cell1_player == &cell2_player && &cell1_player == &cell3_player) {
                return Option::some(cell1_player)
            };
        };
        Option::none()
    }

    fun delete_mark(mark: Mark) {
        let Mark { id, player: _, row: _, col: _ } = mark;
        ID::delete(id);
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
