#[test_only]
module Games::RockPaperScissorsTests {
    use Games::RockPaperScissors::{Self as Game, Game, PlayerTurn, Secret, ThePrize};
    use Sui::TestScenario::{Self};
    use Std::Vector;
    use Std::Hash;

    #[test]
    public fun play_rock_paper_scissors() {
        // So these are our heros.
        let the_main_guy = @0xA1C05;
        let mr_lizard = @0xA55555;
        let mr_spock = @0x590C;

        let scenario = &mut TestScenario::begin(&the_main_guy);

        // Let the game begin!
        Game::new_game(mr_spock, mr_lizard, TestScenario::ctx(scenario));        

        // Mr Spock makes his move. He does it secretly and hashes the gesture with a salt
        // so that only he knows what it is. 
        TestScenario::next_tx(scenario, &mr_spock);
        {
            let hash = hash(Game::rock(), b"my_phaser_never_failed_me!");
            Game::player_turn(the_main_guy, hash, TestScenario::ctx(scenario));
        };

        // Now it's time for The Main Guy to accept his turn.
        TestScenario::next_tx(scenario, &the_main_guy);
        {
            let game = TestScenario::remove_object<Game>(scenario);
            let cap = TestScenario::remove_object<PlayerTurn>(scenario);
            
            assert!(Game::status(&game) == 0, 0); // STATUS_READY
            
            Game::add_hash(&mut game, cap, TestScenario::ctx(scenario));
            
            assert!(Game::status(&game) == 1, 0); // STATUS_HASH_SUBMISSION

            TestScenario::return_object(scenario, game);
        };

        // Same for Mr Lizard. He uses his secret phrase to encode his turn.
        TestScenario::next_tx(scenario, &mr_lizard);
        {
            let hash = hash(Game::scissors(), b"sssssss_you_are_dead!");
            Game::player_turn(the_main_guy, hash, TestScenario::ctx(scenario));
        };

        TestScenario::next_tx(scenario, &the_main_guy);
        {
            let game = TestScenario::remove_object<Game>(scenario);
            let cap = TestScenario::remove_object<PlayerTurn>(scenario);
            Game::add_hash(&mut game, cap, TestScenario::ctx(scenario));

            assert!(Game::status(&game) == 2, 0); // STATUS_HASHES_SUBMITTED

            TestScenario::return_object(scenario, game);
        };

        // Now that both sides made their moves, it's time for  Mr Spock and Mr Lizard to
        // reveal their secrets. The Main Guy will then be able to determine the winner. Who's
        // gonna win The Prize? We'll see in a bit!
        TestScenario::next_tx(scenario, &mr_spock);
        Game::reveal(the_main_guy, b"my_phaser_never_failed_me!", TestScenario::ctx(scenario));

        TestScenario::next_tx(scenario, &the_main_guy);
        {
            let game = TestScenario::remove_object<Game>(scenario);
            let secret = TestScenario::remove_object<Secret>(scenario);
            Game::match_secret(&mut game, secret, TestScenario::ctx(scenario));

            assert!(Game::status(&game) == 3, 0); // STATUS_REVEALING

            TestScenario::return_object(scenario, game);
        };

        TestScenario::next_tx(scenario, &mr_lizard);
        Game::reveal(the_main_guy, b"sssssss_you_are_dead!", TestScenario::ctx(scenario));

        // The final step. The Main Guy matches and reveals the secret of the Mr Lizard and
        // calls the [`select_winner`] function to release The Prize.
        TestScenario::next_tx(scenario, &the_main_guy);
        {
            let game = TestScenario::remove_object<Game>(scenario);
            let secret = TestScenario::remove_object<Secret>(scenario);
            Game::match_secret(&mut game, secret, TestScenario::ctx(scenario));

            assert!(Game::status(&game) == 4, 0); // STATUS_REVEALED

            Game::select_winner(game, TestScenario::ctx(scenario));
        };

        TestScenario::next_tx(scenario, &mr_spock);
        // If it works, then MrSpock is in posession of the prize;
        let prize = TestScenario::remove_object<ThePrize>(scenario); 
        // Don't forget to give it back!
        TestScenario::return_object(scenario, prize); 
    }

    // Copy of the hashing function from the main module.
    fun hash(gesture: u8, salt: vector<u8>): vector<u8> {
        Vector::push_back(&mut salt, gesture);
        Hash::sha2_256(salt)
    }
}
