// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::shared_tic_tac_toe_tests {
    use sui::test_scenario::{Self, Scenario};
    use games::shared_tic_tac_toe::{Self, TicTacToe, Trophy};

    const SEND_MARK_FAILED: u64 = 0;
    const UNEXPECTED_WINNER: u64 = 1;
    const MARK_PLACEMENT_FAILED: u64 = 2;
    const IN_PROGRESS: u8 = 0;
    const X_WIN: u8 = 1;
    const DRAW: u8 = 3;

    #[test]
    fun play_tictactoe() {
        let player_x = @0x0;
        let player_o = @0x1;

        // Anyone can create a game, because the game object will be eventually shared.
        let scenario_val = test_scenario::begin(player_x);
        let scenario = &mut scenario_val;
        shared_tic_tac_toe::create_game(copy player_x, copy player_o, test_scenario::ctx(scenario));
        // Player1 places an X in (1, 1).
        place_mark(1, 1, player_x, scenario);
        /*
        Current game board:
        _|_|_
        _|X|_
         | |
        */

        // Player2 places an O in (0, 0).
        place_mark(0, 0, player_o, scenario);
        /*
        Current game board:
        O|_|_
        _|X|_
         | |
        */

        // Player1 places an X in (0, 2).
        place_mark(0, 2, player_x, scenario);
        /*
        Current game board:
        O|_|X
        _|X|_
         | |
        */

        // Player2 places an O in (1, 0).
        let status = place_mark(1, 0, player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|_|X
        O|X|_
         | |
        */

        // Opportunity for Player1! Player1 places an X in (2, 0).
        status = place_mark(2, 0, player_x, scenario);
        /*
        Current game board:
        O|_|X
        O|X|_
        X| |
        */

        // Check that X has won!
        assert!(status == X_WIN, 2);

        // X has the Trophy
        test_scenario::next_tx(scenario, player_x);
        assert!(
            test_scenario::has_most_recent_for_sender<Trophy>(scenario),
            1
        );

        test_scenario::next_tx(scenario, player_o);
        // O has no Trophy
        assert!(
            !test_scenario::has_most_recent_for_sender<Trophy>(scenario),
            2
        );
        test_scenario::end(scenario_val);
    }


    #[test]
    fun play_tictactoe_draw() {
        let player_x = @0x0;
        let player_o = @0x1;

        // Anyone can create a game, because the game object will be eventually shared.
        let scenario_val = test_scenario::begin(player_x);
        let scenario = &mut scenario_val;
        shared_tic_tac_toe::create_game(copy player_x, copy player_o, test_scenario::ctx(scenario));
        // Player1 places an X in (0, 1).
        let status = place_mark(0, 1, player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        _|X|_
        _|_|_
         | |
        */

        // Player2 places an O in (0, 0).
        status = place_mark(0, 0, player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|_|_
         | |
        */

        // Player1 places an X in (1, 1).
        status = place_mark(1, 1, player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|X|_
         | |
        */

        // Player2 places an O in (2, 1).
        status = place_mark(2, 1, player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|X|_
         |O|
        */

        // Player1 places an X in (2, 0).
        status = place_mark(2, 0, player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|X|_
        X|O|
        */

        // Player2 places an O in (0, 2).
        status = place_mark(0, 2, player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|O
        _|X|_
        X|O|
        */

        // Player1 places an X in (1, 2).
        status = place_mark(1, 2, player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|O
        _|X|X
        X|O|
        */

        // Player2 places an O in (1, 0).
        status = place_mark(1, 0, player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|O
        O|X|X
        X|O|
        */

        // Player1 places an X in (2, 2).
        status = place_mark(2, 2, player_x, scenario);
        /*
        Current game board:
        O|X|O
        O|X|X
        X|O|X
        */

        // We have a draw.
        assert!(status == DRAW, 2);

        // No one has the trophy
        test_scenario::next_tx(scenario, player_x);
        assert!(
            !test_scenario::has_most_recent_for_sender<Trophy>(scenario),
            1
        );
        test_scenario::next_tx(scenario, player_o);
        assert!(
            !test_scenario::has_most_recent_for_sender<Trophy>(scenario),
            1
        );
        test_scenario::end(scenario_val);
    }


    fun place_mark(
        row: u8,
        col: u8,
        player: address,
        scenario: &mut Scenario,
    ): u8  {
        // The gameboard is now a shared object.
        // Any player can place a mark on it directly.
        test_scenario::next_tx(scenario, player);
        let status;
        {
            let game_val = test_scenario::take_shared<TicTacToe>(scenario);
            let game = &mut game_val;
            shared_tic_tac_toe::place_mark(game, row, col, test_scenario::ctx(scenario));
            status = shared_tic_tac_toe::get_status(game);
            test_scenario::return_shared(game_val);
        };
        status
    }
}
