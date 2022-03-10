// This is a rewrite of TicTacToe using a completely different approach.
// In TicTacToe, since the game object is owned by the admin, the players was not
// able to directly mutate the gameboard. Hence each marker placement takes
// two transactions.
// In this implementation, we make the game object a shared mutable object.
// Both players have access and can mutate the game object, and hence they
// can place markers directly in one transaction.
// In general, using shared mutable object has an extra cost due to the fact
// that Sui need to sequence the operations that mutate the shared object from
// different transactions. In this case however, since it is expected for players
// to take turns to place the marker, there won't be a significant overhead in practice.
// As we can see, by using shared mutable object, the implementation is much
// simpler than the other implementation.
module Games::TicTacToeV2 {
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

    // Mark type
    const MARK_EMPTY: u8 = 0;
    const MARK_X: u8 = 1;
    const MARK_O: u8 = 2;

    // Error codes
    /// Trying to place a mark when it's not your turn.
    const EINVALID_TURN: u64 = 0;
    /// Trying to place a mark when the game has already ended.
    const EGAME_ENDED: u64 = 1;
    /// Trying to place a mark in an invalid location, i.e. row/column out of bound.
    const EINVALID_LOCATION: u64 = 2;
    /// The cell to place a new mark at is already oocupied.
    const ECELL_OCCUPIED: u64 = 3;

    struct TicTacToe has key {
        id: VersionedID,
        gameboard: vector<vector<u8>>,
        cur_turn: u8,
        game_status: u8,
        x_address: address,
        o_address: address,
    }

    struct Trophy has key {
        id: VersionedID,
    }

    struct GameEndEvent has copy, drop {
        // The Object ID of the game object
        game_id: ID,
    }

    /// `x_address` and `o_address` are the account address of the two players.
    public fun create_game(x_address: address, o_address: address, ctx: &mut TxContext) {
        // TODO: Validate sender address, only GameAdmin can create games.

        let id = TxContext::new_id(ctx);
        let gameboard = vector[
            vector[MARK_EMPTY, MARK_EMPTY, MARK_EMPTY],
            vector[MARK_EMPTY, MARK_EMPTY, MARK_EMPTY],
            vector[MARK_EMPTY, MARK_EMPTY, MARK_EMPTY],
        ];
        let game = TicTacToe {
            id,
            gameboard,
            // X always go first.
            cur_turn: MARK_X,
            game_status: IN_PROGRESS,
            x_address: x_address,
            o_address: o_address,
        };
        // Make the game a shared object so that both players can mutate it.
        Transfer::share_object(game);
    }

    public fun place_mark(game: &mut TicTacToe, row: u8, col: u8, ctx: &mut TxContext) {
        assert!(row < 3 && col < 3, EINVALID_LOCATION);
        assert!(game.game_status == IN_PROGRESS, EGAME_ENDED);
        let addr = get_cur_turn_address(game);
        assert!(addr == TxContext::sender(ctx), EINVALID_TURN);

        let cell = Vector::borrow_mut(Vector::borrow_mut(&mut game.gameboard, (row as u64)), (col as u64));
        assert!(*cell == MARK_EMPTY, ECELL_OCCUPIED);

        *cell = game.cur_turn;
        update_winner(game);
        game.cur_turn = if (game.cur_turn == MARK_X) MARK_O else MARK_X;

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
        let TicTacToe { id, gameboard: _, cur_turn: _, game_status: _, x_address: _, o_address: _ } = game;
        ID::delete(id);
    }

    public fun delete_trophy(trophy: Trophy, _ctx: &mut TxContext) {
        let Trophy { id } = trophy;
        ID::delete(id);
    }

    public fun get_status(game: &TicTacToe): u8 {
        game.game_status
    }

    fun get_cur_turn_address(game: &TicTacToe): address {
        if (game.cur_turn == MARK_X) {
            *&game.x_address
        } else {
            *&game.o_address
        }
    }

    fun get_cell(game: &TicTacToe, row: u64, col: u64): u8 {
        *Vector::borrow(Vector::borrow(&game.gameboard, row), col)
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
        if (game.game_status == IN_PROGRESS && game.cur_turn == 9) {
            game.game_status = DRAW;
        };
    }

    fun check_for_winner(game: &mut TicTacToe, row1: u64, col1: u64, row2: u64, col2: u64, row3: u64, col3: u64) {
        if (game.game_status != IN_PROGRESS) {
            return
        };
        let result = get_winner_if_all_equal(game, row1, col1, row2, col2, row3, col3);
        if (result != MARK_EMPTY) {
            game.game_status = if (result == MARK_X) X_WIN else O_WIN;
        };
    }

    fun get_winner_if_all_equal(game: &TicTacToe, row1: u64, col1: u64, row2: u64, col2: u64, row3: u64, col3: u64): u8 {
        let cell1 = get_cell(game, row1, col1);
        let cell2 = get_cell(game, row2, col2);
        let cell3 = get_cell(game, row3, col3);
        if (cell1 == cell2 && cell1 == cell3) cell1 else MARK_EMPTY
    }
}
