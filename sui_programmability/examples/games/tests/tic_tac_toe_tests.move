// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::tic_tac_toe_tests {
    use sui::test_scenario::{Self, Scenario};
    use games::tic_tac_toe::{Self, Mark, MarkMintCap, TicTacToe, Trophy};

    const SEND_MARK_FAILED: u64 = 0;
    const UNEXPECTED_WINNER: u64 = 1;
    const MARK_PLACEMENT_FAILED: u64 = 2;
    const IN_PROGRESS: u8 = 0;
    const X_WIN: u8 = 1;
    const DRAW: u8 = 3;

    #[test]
    fun play_tictactoe() {
        let admin = @0x0;
        let player_x = @0x1;
        let player_o = @0x2;

        let scenario_val = test_scenario::begin(admin);
        let scenario = &mut scenario_val;
        // Admin creates a game
        tic_tac_toe::create_game(copy player_x, copy player_o, test_scenario::ctx(scenario));
        // Player1 places an X in (1, 1).
        place_mark(1, 1, admin, player_x, scenario);
        /*
        Current game board:
        _|_|_
        _|X|_
         | |
        */

        // Player2 places an O in (0, 0).
        place_mark(0, 0, admin, player_o, scenario);
        /*
        Current game board:
        O|_|_
        _|X|_
         | |
        */

        // Player1 places an X in (0, 2).
        place_mark(0, 2, admin, player_x, scenario);
        /*
        Current game board:
        O|_|X
        _|X|_
         | |
        */

        // Player2 places an O in (1, 0).
        let status = place_mark(1, 0, admin, player_o, scenario);
        /*
        Current game board:
        O|_|X
        O|X|_
         | |
        */

        // Opportunity for Player1! Player1 places an X in (2, 0).
        assert!(status == IN_PROGRESS, 1);
        status = place_mark(2, 0, admin, player_x, scenario);

        /*
        Current game board:
        O|_|X
        O|X|_
        X| |
        */

        // Check that X has won!
        assert!(status == X_WIN, 2);

        // X has the trophy
        test_scenario::next_tx(scenario, player_x);
        assert!(
            test_scenario::has_most_recent_for_sender<Trophy>(scenario),
            1
        );

        test_scenario::next_tx(scenario, player_o);
        // O has no Trophy
        assert!(
            !test_scenario::has_most_recent_for_sender<Trophy>(scenario),
            1
        );
        test_scenario::end(scenario_val);
    }


    #[test]
    fun play_tictactoe_draw() {
        let admin = @0x0;
        let player_x = @0x1;
        let player_o = @0x2;

        let scenario_val = test_scenario::begin(admin);
        let scenario = &mut scenario_val;

        tic_tac_toe::create_game(copy player_x, copy player_o, test_scenario::ctx(scenario));
        // Player1 places an X in (0, 1).
        let status = place_mark(0, 1, admin, player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        _|X|_
        _|_|_
         | |
        */

        // Player2 places an O in (0, 0).
        status = place_mark(0, 0, admin, player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|_|_
         | |
        */

        // Player1 places an X in (1, 1).
        status = place_mark(1, 1, admin, player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|X|_
         | |
        */

        // Player2 places an O in (2, 1).
        status = place_mark(2, 1, admin, player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|X|_
         |O|
        */

        // Player1 places an X in (2, 0).
        status = place_mark(2, 0, admin, player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|_
        _|X|_
        X|O|
        */

        // Player2 places an O in (0, 2).
        status = place_mark(0, 2, admin, player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|O
        _|X|_
        X|O|
        */

        // Player1 places an X in (1, 2).
        status = place_mark(1, 2, admin, player_x, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|O
        _|X|X
        X|O|
        */

        // Player2 places an O in (1, 0).
        status = place_mark(1, 0, admin, player_o, scenario);
        assert!(status == IN_PROGRESS, 1);
        /*
        Current game board:
        O|X|O
        O|X|X
        X|O|
        */

        // Player1 places an X in (2, 2).
        status = place_mark(2, 2, admin, player_x, scenario);
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
        row: u64,
        col: u64,
        admin: address,
        player: address,
        scenario: &mut Scenario,
    ): u8  {
        // Step 1: player creates a mark and sends it to the game.
        test_scenario::next_tx(scenario, player);
        {
            let cap = test_scenario::take_from_sender<MarkMintCap>(scenario);
            tic_tac_toe::send_mark_to_game(&mut cap, admin, row, col, test_scenario::ctx(scenario));
            test_scenario::return_to_sender(scenario, cap);
        };
        // Step 2: Admin places the received mark on the game board.
        test_scenario::next_tx(scenario, admin);
        let status;
        {
            let game = test_scenario::take_from_sender<TicTacToe>(scenario);
            let mark = test_scenario::take_from_sender<Mark>(scenario);
            assert!(tic_tac_toe::mark_player(&mark) == &player, 0);
            assert!(tic_tac_toe::mark_row(&mark) == row, 1);
            assert!(tic_tac_toe::mark_col(&mark) == col, 2);
            tic_tac_toe::place_mark(&mut game, mark, test_scenario::ctx(scenario));
            status = tic_tac_toe::get_status(&game);
            test_scenario::return_to_sender(scenario, game);
        };
        // return the game status
        status
    }
}
