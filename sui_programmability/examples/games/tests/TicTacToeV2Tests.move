#[test_only]
module Games::TicTacToeV2Tests {
    use Sui::TestScenario::{Self, Scenario};
    use Games::TicTacToeV2::{Self, TicTacToe, Trophy};

    const SEND_MARK_FAILED: u64 = 0;
    const UNEXPECTED_WINNER: u64 = 1;
    const MARK_PLACEMENT_FAILED: u64 = 2;
    const IN_PROGRESS: u8 = 0;
    const X_WIN: u8 = 1;

    #[test]
    fun play_tictactoe() {
        let player_x = @0x0;
        let player_o = @0x1;

        // Anyone can create a game, because the game object will be eventually shared.
        let scenario = &mut TestScenario::begin(&player_x);
        TicTacToeV2::create_game(copy player_x, copy player_o, TestScenario::ctx(scenario));
        // Player1 places an X in (1, 1).
        place_mark(1, 1, &player_x, scenario);
        /*
        Current game board:
        _|_|_
        _|X|_
         | |
        */

        // Player2 places an O in (0, 0).
        place_mark(0, 0, &player_o, scenario);
        /*
        Current game board:
        O|_|_
        _|X|_
         | |
        */

        // Player1 places an X in (0, 2).
        place_mark(0, 2, &player_x, scenario);
        /*
        Current game board:
        O|_|X
        _|X|_
         | |
        */

        // Player2 places an O in (1, 0).
        let status = place_mark(1, 0, &player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|_|X
        O|X|_
         | |
        */

        // Opportunity for Player1! Player1 places an X in (2, 0).
        status = place_mark(2, 0, &player_x, scenario);
        /*
        Current game board:
        O|_|X
        O|X|_
        X| |
        */

        // Check that X has won!
        assert!(status == X_WIN, 2);
        TestScenario::next_tx(scenario, &player_x);
        {
            let trophy = TestScenario::remove_object<Trophy>(scenario);
            TestScenario::return_object(scenario, trophy)
        }
    }

    fun place_mark(
        row: u8,
        col: u8,
        player: &address,
        scenario: &mut Scenario,
    ): u8  {
        // The gameboard is now a shared mutable object.
        // Any player can place a mark on it directly.
        TestScenario::next_tx(scenario, player);
        let status;
        {
            let game = TestScenario::remove_object<TicTacToe>(scenario);
            TicTacToeV2::place_mark(&mut game, row, col, TestScenario::ctx(scenario));
            status = TicTacToeV2::get_status(&game);
            TestScenario::return_object(scenario, game);
        };
        status
    }
}