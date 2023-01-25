// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module nfts::random_word_tests {
    use nfts::random_word;
    use std::option;
    use std::string;
    use sui::coin::Coin;
    use sui::coin;
    use sui::randomness::Randomness;
    use sui::randomness;
    use sui::sui::SUI;
    use sui::test_scenario::Self;

    const SHOP_ADDRESS: address = @0xA001;
    const USER_ADDRESS: address = @0xA002;

    #[test]
    fun test_random_word() {
        let scenario_val = test_scenario::begin(SHOP_ADDRESS);
        let scenario = &mut scenario_val;

        // Create the shop
        let words = vector[
            string::utf8(b"Hello"),
            string::utf8(b"World"),
            string::utf8(b"!")
        ];
        let weights = vector[3, 10, 2];
        random_word::create(words, weights, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, SHOP_ADDRESS);
        assert!(test_scenario::has_most_recent_immutable<random_word::Shop>(), 0);
        let shop = test_scenario::take_immutable<random_word::Shop>(scenario);

        // Mint 1 SUI.
        sui::transfer::transfer(coin::mint_for_testing<SUI>(1, test_scenario::ctx(scenario)), USER_ADDRESS);
        test_scenario::next_tx(scenario, USER_ADDRESS);
        let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);

        // Buy a ticket.
        random_word::buy(&shop, coin, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, USER_ADDRESS);
        let t = test_scenario::take_from_sender<random_word::Ticket>(scenario);
        let r = test_scenario::take_from_sender<Randomness<random_word::RANDOMNESS_WITNESS>>(scenario);
        assert!(option::is_none(randomness::value(&r)), 0);
        let sig = randomness::sign(&r);

        // Get the 'word'.
        random_word::mint(&shop, t, r, sig, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, USER_ADDRESS);
        assert!(test_scenario::has_most_recent_for_sender<random_word::Word>(scenario), 0);

        test_scenario::return_immutable(shop);
        test_scenario::end(scenario_val);
    }
}
