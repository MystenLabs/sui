// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module nfts::chat_tests {
    use nfts::chat::{Self, Chat};
    use std::ascii::Self;
    use sui::test_scenario::Self;

    const USER1_ADDRESS: address = @0xA001;
    const METADATA: vector<u8> = vector[0u8];
    const HELLO: vector<u8> = vector[72, 101, 108, 108, 111]; // "Hello" in ASCII.

    #[test]
    fun test_chat() {
        let scenario_val = test_scenario::begin(USER1_ADDRESS);
        let scenario = &mut scenario_val;
        {
            chat::post(
                @0xC001, // This should be an application object ID.
                HELLO,
                METADATA, // Some metadata (it could be empty).
                test_scenario::ctx(scenario)
            );
        };

        test_scenario::next_tx(scenario, USER1_ADDRESS);
        {
            assert!(test_scenario::has_most_recent_for_sender<Chat>(scenario), 0);
            let chat = test_scenario::take_from_sender<Chat>(scenario); // if can remove, object exists
            assert!(chat::text(&chat) == ascii::string(HELLO), 0);
            test_scenario::return_to_sender(scenario, chat);
        };
        test_scenario::end(scenario_val);
    }
}
