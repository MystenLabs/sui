// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module slot_machine::tests;

use slot_machine::example;
use sui::{
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
fun test_game() {
    let user1 = @0x0;
    let user2 = @0x1;
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
    let coin = ts.take_from_sender<Coin<SUI>>();
    example::create(coin, ts.ctx());
    ts.next_tx(user1);
    let mut game = ts.take_shared<example::Game>();
    assert!(game.balance() == 1000, 1);
    assert!(game.epoch() == 0, 1);

    // Play 4 turns (everything here is deterministic)
    ts.next_tx(user2);
    mint(user2, 100, &mut ts);
    let mut coin: Coin<SUI> = ts.take_from_sender();
    game.play(&random_state, &mut coin, ts.ctx());
    assert!(game.balance() == 1100, 1); // lost 100
    assert!(coin.value() == 0, 1);
    ts.return_to_sender(coin);

    ts.next_tx(user2);
    mint(user2, 200, &mut ts);
    let mut coin: Coin<SUI> = ts.take_from_sender();
    game.play(&random_state, &mut coin, ts.ctx());
    assert!(game.balance() == 900, 1); // won 200
    // check that received the right amount
    assert!(coin.value() == 400, 1);
    ts.return_to_sender(coin);

    ts.next_tx(user2);
    mint(user2, 300, &mut ts);
    let mut coin: Coin<SUI> = ts.take_from_sender();
    game.play(&random_state, &mut coin, ts.ctx());
    assert!(game.balance() == 600, 1); // won 300
    // check that received the remaining amount
    assert!(coin.value() == 600, 1);
    ts.return_to_sender(coin);

    ts.next_tx(user2);
    mint(user2, 200, &mut ts);
    let mut coin: Coin<SUI> = ts.take_from_sender();
    game.play(&random_state, &mut coin, ts.ctx());
    assert!(game.balance() == 800, 1); // lost 200
    // check that received the right amount
    assert!(coin.value() == 0, 1);
    ts.return_to_sender(coin);

    // TODO: test also that the last coin is taken

    // Take remaining balance
    ts.next_epoch(user1);
    let coin = game.close(ts.ctx());
    assert!(coin.value() == 800, 1);
    coin.burn_for_testing();

    ts::return_shared(random_state);
    ts.end();
}
