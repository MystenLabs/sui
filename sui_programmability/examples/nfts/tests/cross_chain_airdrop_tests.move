// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module nfts::cross_chain_airdrop_tests {
    use nfts::cross_chain_airdrop::{Self, CrossChainAirdropOracle, ERC721};
    use sui::object::{UID};
    use sui::test_scenario::{Self, Scenario};

    // Error codes

    /// Trying to claim a token that has already been claimed
    const ETOKEN_ID_CLAIMED: u64 = 0;
    const EOBJECT_NOT_FOUND: u64 = 1;

    const ORACLE_ADDRESS: address = @0x1000;
    const RECIPIENT_ADDRESS: address = @0x10;
    const SOURCE_CONTRACT_ADDRESS: vector<u8> = x"BC4CA0EdA7647A8aB7C2061c2E118A18a936f13D";
    const SOURCE_TOKEN_ID: u64 = 101;
    const NAME: vector<u8> = b"BoredApeYachtClub";
    const TOKEN_URI: vector<u8> = b"ipfs://QmeSjSinHpPnmXmspMjwiXyN6zS4E9zccariGR3jxcaWtq/101";

    struct Object has key {
        id: UID,
    }

    #[test]
    fun test_claim_airdrop() {
        let (scenario, oracle_address) = init();

        // claim a token
        claim_token(&mut scenario, &oracle_address, SOURCE_TOKEN_ID);

        // verify that the recipient has received the nft
        assert!(owns_object(&mut scenario, &RECIPIENT_ADDRESS), EOBJECT_NOT_FOUND);
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_double_claim() {
        let (scenario, oracle_address) = init();

        // claim a token
        claim_token(&mut scenario, &oracle_address, SOURCE_TOKEN_ID);

        // claim the same token again
        claim_token(&mut scenario, &oracle_address, SOURCE_TOKEN_ID);
    }

    fun init(): (Scenario, address) {
        let scenario = test_scenario::begin(&ORACLE_ADDRESS);
        {
            let ctx = test_scenario::ctx(&mut scenario);
            cross_chain_airdrop::test_init(ctx);
        };
        (scenario, ORACLE_ADDRESS)
    }

    fun claim_token(scenario: &mut Scenario, oracle_address: &address, token_id: u64) {
        test_scenario::next_tx(scenario, oracle_address);
        {
            let oracle = test_scenario::take_owned<CrossChainAirdropOracle>(scenario);
            let ctx = test_scenario::ctx(scenario);
            cross_chain_airdrop::claim(
                &mut oracle,
                RECIPIENT_ADDRESS,
                SOURCE_CONTRACT_ADDRESS,
                token_id,
                NAME,
                TOKEN_URI,
                ctx,
            );
            test_scenario::return_owned(scenario, oracle);
        };
    }

    fun owns_object(scenario: &mut Scenario, owner: &address): bool{
        // Verify the token has been transfer to the recipient
        test_scenario::next_tx(scenario, owner);
        test_scenario::can_take_owned<ERC721>(scenario)
    }
}
