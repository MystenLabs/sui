#[test_only]
module Games::TicTacToeTests {
    use Sui::TestScenario::{Self, Scenario};
    use Games::TicTacToe::{Self, Mark, MarkMintCap, TicTacToe, Trophy};

    const SEND_MARK_FAILED: u64 = 0;
    const UNEXPECTED_WINNER: u64 = 1;
    const MARK_PLACEMENT_FAILED: u64 = 2;
    const IN_PROGRESS: u8 = 0;
    const X_WIN: u8 = 1;

    #[test]
    fun play_tictactoe() {
        let admin = @0x0;
        let player_x = @0x1;
        let player_o = @0x2;

        let scenario = &mut TestScenario::begin(&admin);
        // Admin creates a game
        TicTacToe::create_game(copy player_x, copy player_o, TestScenario::ctx(scenario));
        // Player1 places an X in (1, 1).
        place_mark(1, 1, &admin, &player_x, scenario);
        /*
        Current game board:
        _|_|_
        _|X|_
         | |
        */

        // Player2 places an O in (0, 0).
        place_mark(0, 0, &admin, &player_o, scenario);
        /*
        Current game board:
        O|_|_
        _|X|_
         | |
        */

        // Player1 places an X in (0, 2).
        place_mark(0, 2, &admin, &player_x, scenario);
        /*
        Current game board:
        O|_|X
        _|X|_
         | |
        */

        // Player2 places an O in (1, 0).
        let status = place_mark(1, 0, &admin, &player_o, scenario);
        /*
        Current game board:
        O|_|X
        O|X|_
         | |
        */

        // Opportunity for Player1! Player1 places an X in (2, 0).
        assert!(status == IN_PROGRESS, 1);
        status = place_mark(2, 0, &admin, &player_x, scenario);
        assert!(status == X_WIN, 2);
        /*
        Current game board:
        O|_|X
        O|X|_
        X| |
        */

        // Check that X has won!
        TestScenario::next_tx(scenario, &player_x);
        let trophy = TestScenario::remove_object<Trophy>(scenario);
        TestScenario::return_object(scenario, trophy)
    }

    fun place_mark(
        row: u64,
        col: u64,
        admin: &address,
        player: &address,
        scenario: &mut Scenario,
    ): u8  {
        // Step 1: player creates a mark and sends it to the game.
        TestScenario::next_tx(scenario, player);
        {
            let cap = TestScenario::remove_object<MarkMintCap>(scenario);
            TicTacToe::send_mark_to_game(&mut cap, *admin, row, col, TestScenario::ctx(scenario));
            TestScenario::return_object(scenario, cap);
        };
        // Step 2: Admin places the received mark on the game board.
        TestScenario::next_tx(scenario, admin);
        let status;
        {
            let game = TestScenario::remove_object<TicTacToe>(scenario);
            let mark = TestScenario::remove_object<Mark>(scenario);
            assert!(TicTacToe::mark_player(&mark) == player, 0);
            assert!(TicTacToe::mark_row(&mark) == row, 1);
            assert!(TicTacToe::mark_col(&mark) == col, 2);
            TicTacToe::place_mark(&mut game, mark, TestScenario::ctx(scenario));
            status = TicTacToe::get_status(&game);
            TestScenario::return_object(scenario, game);
        };
        // return the game status
        status
    }
}