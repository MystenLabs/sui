// Copyright (c) 2022, Mysten Labs, Inc.
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
        let scenario = &mut test_scenario::begin(&USER1_ADDRESS);
        {
            chat::post(
                @0xC001, // This should be an application object ID.
                HELLO,
                METADATA, // Some metadata (it could be empty).
                test_scenario::ctx(scenario)
            );
        };

        test_scenario::next_tx(scenario, &USER1_ADDRESS);
        {
            assert!(test_scenario::can_take_owned<Chat>(scenario), 0);
            let chat = test_scenario::take_owned<Chat>(scenario); // if can remove, object exists
            assert!(chat::text(&chat) == ascii::string(HELLO), 0);
            test_scenario::return_owned(scenario, chat);
        }
    }
}
