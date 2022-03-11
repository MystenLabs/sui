// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::CrossChainAirdropTests {
    use Sui::CrossChainAirdrop::{Self, CrossChainAirdropOracle, NFT};
    use Sui::ID::{VersionedID};
    use Sui::TestScenario::{Self, Scenario};

    // Error codes

    /// Trying to claim a token that has already been claimed
    const ETOKEN_ID_CLAIMED: u64 = 0;
    const EOBJECT_NOT_FOUND: u64 = 1;

    const RECIPIENT_ADDRESS: address = @0x10;
    const SOURCE_CONTRACT_ADDRESS: vector<u8> = x"BC4CA0EdA7647A8aB7C2061c2E118A18a936f13D";
    const SOURCE_TOKEN_ID: u64 = 101;
    const NAME: vector<u8> = b"BoredApeYachtClub";
    const TOKEN_URI: vector<u8> = b"ipfs://QmeSjSinHpPnmXmspMjwiXyN6zS4E9zccariGR3jxcaWtq/101";

    struct Object has key {
        id: VersionedID,
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
        let oracle_address = CrossChainAirdrop::oracle_address();
        let scenario = TestScenario::begin(&oracle_address);
        {
            let ctx = TestScenario::ctx(&mut scenario);
            CrossChainAirdrop::test_init(ctx);
        };
        (scenario, oracle_address)
    }

    fun claim_token(scenario: &mut Scenario, oracle_address: &address, token_id: u64) {
        TestScenario::next_tx(scenario, oracle_address);
        {
            let oracle = TestScenario::remove_object<CrossChainAirdropOracle>(scenario);
            let ctx = TestScenario::ctx(scenario);
            CrossChainAirdrop::claim(
                &mut oracle,
                RECIPIENT_ADDRESS,
                SOURCE_CONTRACT_ADDRESS,
                token_id,
                NAME,
                TOKEN_URI,
                ctx,
            );
            TestScenario::return_object(scenario, oracle);
        };
    }

    fun owns_object(scenario: &mut Scenario, owner: &address): bool{
        // Verify the token has been transfer to the recipient
        TestScenario::next_tx(scenario, owner);
        TestScenario::can_remove_object<NFT>(scenario)
    }
}
