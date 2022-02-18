#[test_only]
module Examples::TicTacToeTests {
    use FastX::TxContext::{Self, TxContext};
    use FastX::TestHelper;
    use Examples::TicTacToe::{Self, Mark, MarkMintCap, TicTacToe, Trophy};

    const SEND_MARK_FAILED: u64 = 0;
    const UNEXPECTED_WINNER: u64 = 1;
    const MARK_PLACEMENT_FAILED: u64 = 2;

    #[test]
    fun play_tictactoe() {
        let admin_ctx = TxContext::dummy_with_hint(0);
        let player_x_ctx = TxContext::dummy_with_hint(2);
        let player_o_ctx = TxContext::dummy_with_hint(1);

        // Create a game under admin.
        TicTacToe::create_game(
            TxContext::get_signer_address(&player_x_ctx),
            TxContext::get_signer_address(&player_o_ctx),
            &mut admin_ctx,
        );
        let game = TestHelper::get_last_received_object<TicTacToe>(&admin_ctx);
        
        let cap_x = TestHelper::get_last_received_object<MarkMintCap>(&player_x_ctx);
        let cap_o = TestHelper::get_last_received_object<MarkMintCap>(&player_o_ctx);

        // Player1 places an X in (1, 1).
        place_mark(&mut game, &mut cap_x, 1, 1, &mut admin_ctx, &mut player_x_ctx);
        /*
        Current game board:
        _|_|_
        _|X|_
         | |
        */

        // Player2 places an O in (0, 0).
        place_mark(&mut game, &mut cap_x, 0, 0, &mut admin_ctx, &mut player_o_ctx);
        /*
        Current game board:
        O|_|_
        _|X|_
         | |
        */

        // Player1 places an X in (0, 2).
        place_mark(&mut game, &mut cap_x, 0, 2, &mut admin_ctx, &mut player_x_ctx);
        /*
        Current game board:
        O|_|X
        _|X|_
         | |
        */

        // Player2 places an O in (1, 0).
        place_mark(&mut game, &mut cap_x, 1, 0, &mut admin_ctx, &mut player_o_ctx);
        /*
        Current game board:
        O|_|X
        O|X|_
         | |
        */

        // Opportunity for Player1! Player1 places an X in (2, 0).
        place_mark(&mut game, &mut cap_x, 2, 0, &mut admin_ctx, &mut player_x_ctx);
        /*
        Current game board:
        O|_|X
        O|X|_
        X| |
        */
        // Check that X has won!
        let trophy = TestHelper::get_last_received_object<Trophy>(&mut player_x_ctx);
        TicTacToe::delete_trophy(trophy, &mut player_x_ctx);

        // Cleanup and delete all objects in the game.
        TicTacToe::delete_game(game, &mut admin_ctx);
        TicTacToe::delete_cap(cap_x, &mut player_x_ctx);
        TicTacToe::delete_cap(cap_o, &mut player_o_ctx);
    }

    fun place_mark(
        game: &mut TicTacToe,
        cap: &mut MarkMintCap,
        row: u64,
        col: u64,
        admin_ctx: &mut TxContext,
        player_ctx: &mut TxContext,
    ) {
        // Step 1: Create a mark and send it to the game.
        TicTacToe::send_mark_to_game(cap, TxContext::get_signer_address(admin_ctx), row, col, player_ctx);

        // Step 2: Game places the mark on the game board.
        let mark = TestHelper::get_last_received_object<Mark>(admin_ctx);
        TicTacToe::place_mark(game, mark, admin_ctx);
    }
}