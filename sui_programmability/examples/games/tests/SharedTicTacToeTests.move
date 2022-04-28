// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Games::SharedTicTacToeTests {
    use Sui::TestScenario::{Self, Scenario};
    use Games::SharedTicTacToe::{Self, TicTacToe, Trophy};

    const SEND_MARK_FAILED: u64 = 0;
    const UNEXPECTED_WINNER: u64 = 1;
    const MARK_PLACEMENT_FAILED: u64 = 2;
    const IN_PROGRESS: u8 = 0;
    const X_WIN: u8 = 1;
    const DRAW: u8 = 3;

    #[test]
    public(script) fun play_tictactoe() {
        let player_x = @0x0;
        let player_o = @0x1;

        // Anyone can create a game, because the game object will be eventually shared.
        let scenario = &mut TestScenario::begin(&player_x);
        SharedTicTacToe::create_game(copy player_x, copy player_o, TestScenario::ctx(scenario));
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

        // X has the Trophy
        TestScenario::next_tx(scenario, &player_x);
        assert!(TestScenario::can_take_object<Trophy>(scenario), 1);

        TestScenario::next_tx(scenario, &player_o);
        // O has no Trophy
        assert!(!TestScenario::can_take_object<Trophy>(scenario), 2);
    }


    #[test]
    public(script) fun play_tictactoe_draw() {
        let player_x = @0x0;
        let player_o = @0x1;

        // Anyone can create a game, because the game object will be eventually shared.
        let scenario = &mut TestScenario::begin(&player_x);
        SharedTicTacToe::create_game(copy player_x, copy player_o, TestScenario::ctx(scenario));
        // Player1 places an X in (0, 1).
        let status = place_mark(0, 1, &player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        _|X|_
        _|_|_
         | |
        */

        // Player2 places an O in (0, 0).
        status = place_mark(0, 0, &player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|_|_
         | |
        */

        // Player1 places an X in (1, 1).
        status = place_mark(1, 1, &player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|X|_
         | |
        */

        // Player2 places an O in (2, 1).
        status = place_mark(2, 1, &player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|X|_
         |O|
        */

        // Player1 places an X in (2, 0).
        status = place_mark(2, 0, &player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|X|_
        X|O|
        */

        // Player2 places an O in (0, 2).
        status = place_mark(0, 2, &player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|O
        _|X|_
        X|O|
        */

        // Player1 places an X in (1, 2).
        status = place_mark(1, 2, &player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|O
        _|X|X
        X|O|
        */

        // Player2 places an O in (1, 0).
        status = place_mark(1, 0, &player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|O
        O|X|X
        X|O|
        */

        // Player1 places an X in (2, 2).
        status = place_mark(2, 2, &player_x, scenario);
        /*
        Current game board:
        O|X|O
        O|X|X
        X|O|X
        */

        // We have a draw.
        assert!(status == DRAW, 2);

        // No one has the trophy
        TestScenario::next_tx(scenario, &player_x);
        assert!(!TestScenario::can_take_object<Trophy>(scenario), 1);
        TestScenario::next_tx(scenario, &player_o);
        assert!(!TestScenario::can_take_object<Trophy>(scenario), 1);
    }


    public(script) fun place_mark(
        row: u8,
        col: u8,
        player: &address,
        scenario: &mut Scenario,
    ): u8  {
        // The gameboard is now a shared object.
        // Any player can place a mark on it directly.
        TestScenario::next_tx(scenario, player);
        let status;
        {
            let game_wrapper = TestScenario::take_shared_object<TicTacToe>(scenario);
            let game = TestScenario::borrow_mut(&mut game_wrapper);
            SharedTicTacToe::place_mark(game, row, col, TestScenario::ctx(scenario));
            status = SharedTicTacToe::get_status(game);
            TestScenario::return_shared_object(scenario, game_wrapper);
        };
        status
    }
}
