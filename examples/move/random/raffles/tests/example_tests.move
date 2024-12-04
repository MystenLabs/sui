// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module raffles::tests;

use raffles::{example1, example2};
use sui::{
    clock,
    coin::{Self, Coin},
    random::{Self, update_randomness_state_for_testing, Random},
    sui::SUI,
    test_scenario as ts
};

fun mint(addr: address, amount: u64, scenario: &mut ts::Scenario) {
    transfer::public_transfer(coin::mint_for_testing<SUI>(amount, scenario.ctx()), addr);
    scenario.next_tx(addr);
}

#[test]
fun test_example1() {
    let user1 = @0x0;
    let user2 = @0x1;
    let user3 = @0x2;
    let user4 = @0x3;
    let mut ts = ts::begin(user1);

    // Setup randomness
    random::create_for_testing(ts.ctx());
    ts.next_tx(user1);
    let mut random_state: Random = ts.take_shared();
    random_state.update_randomness_state_for_testing(
        0,
        x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
        ts.ctx(),
    );

    // Create the game and get back the output objects.
    mint(user1, 1000, &mut ts);
    let end_time = 100;
    example1::create(end_time, 10, ts.ctx());
    ts.next_tx(user1);
    let mut game: example1::Game = ts.take_shared();
    assert!(game.cost_in_sui() == 10, 1);
    assert!(game.participants() == 0, 1);
    assert!(game.end_time() == end_time, 1);
    assert!(game.winner() == option::none(), 1);
    assert!(game.balance() == 0, 1);

    let mut clock = clock::create_for_testing(ts.ctx());
    clock.set_for_testing(10);

    // Play with 4 users (everything here is deterministic)
    ts.next_tx(user1);
    mint(user1, 10, &mut ts);
    let coin: Coin<SUI> = ts.take_from_sender();
    let t1 = game.buy_ticket(coin, &clock, ts.ctx());
    assert!(game.participants() == 1, 1);
    t1.destroy(); // loser

    ts.next_tx(user2);
    mint(user2, 10, &mut ts);
    let coin: Coin<SUI> = ts.take_from_sender();
    let t2 = game.buy_ticket(coin, &clock, ts.ctx());
    assert!(game.participants() == 2, 1);
    t2.destroy(); // loser

    ts.next_tx(user3);
    mint(user3, 10, &mut ts);
    let coin: Coin<SUI> = ts.take_from_sender();
    let t3 = game.buy_ticket(coin, &clock, ts.ctx());
    assert!(game.participants() == 3, 1);
    t3.destroy(); // loser

    ts.next_tx(user4);
    mint(user4, 10, &mut ts);
    let coin: Coin<SUI> = ts.take_from_sender();
    let t4 = game.buy_ticket(coin, &clock, ts.ctx());
    assert!(game.participants() == 4, 1);
    // this is the winner

    // Determine the winner (-> user3)
    clock.set_for_testing(101);
    game.determine_winner(&random_state, &clock, ts.ctx());
    assert!(game.winner() == option::some(4), 1);
    assert!(game.balance() == 40, 1);
    clock.destroy_for_testing();

    // Take the reward
    let coin = t4.redeem(game, ts.ctx());
    assert!(coin.value() == 40, 1);
    coin.burn_for_testing();

    ts::return_shared(random_state);
    ts.end();
}

#[test]
fun test_example2() {
    let user1 = @0x0;
    let user2 = @0x1;
    let user3 = @0x2;
    let user4 = @0x3;

    let mut ts = ts::begin(user1);

    // Setup randomness
    random::create_for_testing(ts.ctx());
    ts.next_tx(user1);
    let mut random_state: Random = ts.take_shared();
    random_state.update_randomness_state_for_testing(
        0,
        x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
        ts.ctx(),
    );

    // Create the game and get back the output objects.
    mint(user1, 1000, &mut ts);
    let end_time = 100;
    example2::create(end_time, 10, ts.ctx());
    ts.next_tx(user1);
    let mut game: example2::Game = ts.take_shared();
    assert!(game.cost_in_sui() == 10, 1);
    assert!(game.participants() == 0, 1);
    assert!(game.end_time() == end_time, 1);
    assert!(game.balance() == 0, 1);

    let mut clock = clock::create_for_testing(ts.ctx());
    clock.set_for_testing(10);

    // Play with 4 users (everything here is deterministic)
    ts.next_tx(user1);
    mint(user1, 10, &mut ts);
    let coin: Coin<SUI> = ts.take_from_sender();
    game.play(coin, &clock, ts.ctx());
    assert!(game.participants() == 1, 1);

    ts.next_tx(user2);
    mint(user2, 10, &mut ts);
    let coin: Coin<SUI> = ts.take_from_sender();
    game.play(coin, &clock, ts.ctx());
    assert!(game.participants() == 2, 1);

    ts.next_tx(user3);
    mint(user3, 10, &mut ts);
    let coin: Coin<SUI> = ts.take_from_sender();
    game.play(coin, &clock, ts.ctx());
    assert!(game.participants() == 3, 1);

    ts.next_tx(user4);
    mint(user4, 10, &mut ts);
    let coin: Coin<SUI> = ts.take_from_sender();
    game.play(coin, &clock, ts.ctx());
    assert!(game.participants() == 4, 1);

    // Determine the winner (-> user4)
    clock.set_for_testing(101);
    game.close(&random_state, &clock, ts.ctx());
    clock.destroy_for_testing();

    // Check that received the reward
    ts.next_tx(user4);
    let coin: Coin<SUI> = ts.take_from_sender();
    assert!(coin.value() == 40, 1);
    coin.burn_for_testing();

    ts::return_shared(random_state);
    ts.end();
}
