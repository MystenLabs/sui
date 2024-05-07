// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module random_nft::tests {
    use sui::test_scenario;
    use std::string;
    use sui::random;
    use sui::random::{Random, update_randomness_state_for_testing};
    use sui::test_scenario::{ctx, take_from_sender, next_tx, return_to_sender};
    use random_nft::example;

    #[test]
    fun test_e2e() {
        let user0 = @0x0;
        let user1 = @0x1;
        let mut scenario_val = test_scenario::begin(user0);
        let scenario = &mut scenario_val;

        // Setup randomness
        random::create_for_testing(ctx(scenario));
        test_scenario::next_tx(scenario, user0);
        let mut random_state = test_scenario::take_shared<Random>(scenario);
        update_randomness_state_for_testing(
        &mut random_state,
        0,
        x"1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F1F",
        test_scenario::ctx(scenario),
        );

        test_scenario::next_tx(scenario, user1);
        // mint airdrops
        example::test_init(ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let cap = take_from_sender<example::MintingCapability>(scenario);
        let mut nfts = example::mint(&cap, 20, ctx(scenario));

        let mut seen_gold = false;
        let mut seen_silver = false;
        let mut seen_bronze = false;
        let mut i = 0;
        while (i < 20) {
        if (i % 2 == 1) example::reveal(vector::pop_back(&mut nfts), &random_state, ctx(scenario))
        else example::reveal_alternative1(vector::pop_back(&mut nfts), &random_state, ctx(scenario));
        next_tx(scenario, user1);
        let nft = take_from_sender<example::MetalNFT>(scenario);
        let metal = example::metal_string(&nft);
        seen_gold = seen_gold || metal == string::utf8(b"Gold");
        seen_silver = seen_silver || metal == string::utf8(b"Silver");
        seen_bronze = seen_bronze || metal == string::utf8(b"Bronze");
        return_to_sender(scenario, nft);
        i = i + 1;
        };

        assert!(seen_gold && seen_silver && seen_bronze, 1);

        vector::destroy_empty(nfts);
        example::destroy_cap(cap);
        test_scenario::return_shared(random_state);
        test_scenario::end(scenario_val);
    }
}
