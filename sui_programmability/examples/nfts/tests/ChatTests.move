// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module NFTs::ChatTests {
    use NFTs::Chat::{Self, Chat};
    use Std::ASCII::Self;
    use Sui::TestScenario::Self;

    const USER1_ADDRESS: address = @0xA001;
    const METADATA: vector<u8> = vector[0u8];
    const HELLO: vector<u8> = vector[72, 101, 108, 108, 111]; // "Hello" in ASCII.

    #[test]
    public(script) fun test_chat() {
        let scenario = &mut TestScenario::begin(&USER1_ADDRESS);
        {
            Chat::mint(
                @0xC001, // This should be an application object ID.
                HELLO,
                @0x0000, // We're referencing the all-zero bytes object (i.e., it's not a retweet or reply).
                METADATA, // Some metadata (it could be empty).
                TestScenario::ctx(scenario)
            );
        };

        TestScenario::next_tx(scenario, &USER1_ADDRESS);
        {
            assert!(TestScenario::can_take_owned<Chat>(scenario), 0);
            let chat = TestScenario::take_owned<Chat>(scenario); // if can remove, object exists
            assert!(Chat::text(&chat) == ASCII::string(HELLO), 0);
            TestScenario::return_owned(scenario, chat);
        }
    }
}
