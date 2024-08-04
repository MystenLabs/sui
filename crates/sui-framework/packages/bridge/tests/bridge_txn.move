// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module bridge::bridge_txn {
    use bridge::bridge_setup::{create_bridge_default, create_env, create_validator};
    use bridge::chain_ids;
    use bridge::crypto::ecdsa_pub_key_to_eth_address;
    use bridge::eth::ETH;

    #[test]
    fun test_limits() {
        let mut env = create_env(chain_ids::sui_custom());
        env.create_bridge_default();

        let source_chain = chain_ids::eth_custom();
        let sui_address = @0xABCDEF;
        let eth_address = x"0000000000000000000000000000000000001234";

        // lower limits
        let chain_id = env.chain_id();
        env.update_bridge_limit(@0x0, chain_id, source_chain, 3000);
        let transfer_id1 = env.bridge_to_sui<ETH>(
            source_chain,
            eth_address,
            sui_address,
            4000000000,
        );
        let transfer_id2 = env.bridge_to_sui<ETH>(source_chain, eth_address, sui_address, 1000);
        assert!(!env.claim_and_transfer_token<ETH>(source_chain, transfer_id1));
        assert!(env.claim_and_transfer_token<ETH>(source_chain, transfer_id2));
        // double claim is ok and it is a no-op
        assert!(!env.claim_and_transfer_token<ETH>(source_chain, transfer_id2));

        // up limits to allow claim
        env.update_bridge_limit(@0x0, chain_id, source_chain, 4000);
        assert!(env.claim_and_transfer_token<ETH>(source_chain, transfer_id1));

        env.destroy_env();
    }

    #[test]
    fun test_bridge_and_claim() {
        let mut env = create_env(chain_ids::sui_custom());
        env.create_bridge_default();

        let source_chain = chain_ids::eth_custom();
        let sui_address = @0xABCDEF;
        let eth_address = x"0000000000000000000000000000000000001234";
        let amount = 1000;

        //
        // move from eth and transfer to sui account
        let transfer_id1 = env.bridge_to_sui<ETH>(source_chain, eth_address, sui_address, amount);
        env.claim_and_transfer_token<ETH>(source_chain, transfer_id1);
        let transfer_id2 = env.bridge_to_sui<ETH>(source_chain, eth_address, sui_address, amount);
        env.claim_and_transfer_token<ETH>(source_chain, transfer_id2);
        // double claim is ok and it is a no-op
        assert!(!env.claim_and_transfer_token<ETH>(source_chain, transfer_id2));

        //
        // change order
        let transfer_id1 = env.bridge_to_sui<ETH>(source_chain, eth_address, sui_address, amount);
        let transfer_id2 = env.bridge_to_sui<ETH>(source_chain, eth_address, sui_address, amount);
        env.claim_and_transfer_token<ETH>(source_chain, transfer_id1);
        env.claim_and_transfer_token<ETH>(source_chain, transfer_id2);

        //
        // move from eth and send it back
        let transfer_id = env.bridge_to_sui<ETH>(source_chain, eth_address, sui_address, amount);
        let token = env.claim_token<ETH>(sui_address, source_chain, transfer_id);
        env.send_token<ETH>(
            sui_address,
            source_chain,
            eth_address,
            token,
        );

        //
        // approve with subset of signatures
        let message = env.bridge_in_message<ETH>(
            source_chain,
            eth_address,
            sui_address,
            amount,
        );
        let signatures = env.sign_message_with(message, vector[0, 2]);
        let transfer_id = message.seq_num();
        env.approve_token_transfer(message, signatures);
        env.claim_and_transfer_token<ETH>(source_chain, transfer_id);

        //
        // multiple approve with subset of signatures
        let message = env.bridge_in_message<ETH>(
            source_chain,
            eth_address,
            sui_address,
            amount,
        );
        let signatures = env.sign_message_with(message, vector[0, 2]);
        let transfer_id = message.seq_num();
        env.approve_token_transfer(message, signatures);
        env.approve_token_transfer(message, signatures);
        env.approve_token_transfer(message, signatures);
        let token = env.claim_token<ETH>(sui_address, source_chain, transfer_id);
        let send_token_id = env.send_token<ETH>(
            sui_address,
            source_chain,
            eth_address,
            token,
        );
        let message = env.bridge_out_message<ETH>(
            source_chain,
            eth_address,
            sui_address,
            amount,
            send_token_id,
        );
        let signatures = env.sign_message_with(message, vector[1, 2]);
        env.approve_token_transfer(message, signatures);
        let signatures = env.sign_message_with(message, vector[0, 2]);
        env.approve_token_transfer(message, signatures);

        //
        // multiple approve with different subset of signatures
        let message = env.bridge_in_message<ETH>(
            source_chain,
            eth_address,
            sui_address,
            amount,
        );
        let transfer_id = message.seq_num();
        let signatures = env.sign_message_with(message, vector[0, 2]);
        env.approve_token_transfer(message, signatures);
        let signatures = env.sign_message_with(message, vector[0, 1]);
        env.approve_token_transfer(message, signatures);
        let signatures = env.sign_message_with(message, vector[1, 2]);
        env.approve_token_transfer(message, signatures);
        let token = env.claim_token<ETH>(sui_address, source_chain, transfer_id);
        env.send_token<ETH>(
            sui_address,
            source_chain,
            eth_address,
            token,
        );

        env.destroy_env();
    }

    #[test]
    #[expected_failure(abort_code = bridge::committee::ESignatureBelowThreshold)]
    fun test_blocklist() {
        let mut env = create_env(chain_ids::sui_custom());
        let validators = vector[
            create_validator(@0xAAAA, 100, &b"1234567890_1234567890_1234567890"),
            create_validator(@0xBBBB, 100, &b"234567890_1234567890_1234567890_"),
            create_validator(@0xCCCC, 100, &b"34567890_1234567890_1234567890_1"),
            create_validator(@0xDDDD, 100, &b"4567890_1234567890_1234567890_12"),
        ];
        env.setup_validators(validators);

        let sender = @0x0;
        env.create_bridge(sender);
        env.register_committee();
        env.init_committee(sender);
        env.setup_treasury(sender);

        let source_chain = chain_ids::eth_custom();
        let sui_address = @0xABCDEF;
        let eth_address = x"0000000000000000000000000000000000001234";
        let amount = 1000;

        // bridging in and out works
        let message = env.bridge_in_message<ETH>(
            source_chain,
            eth_address,
            sui_address,
            amount,
        );
        let signatures = env.sign_message_with(message, vector[0, 2]);
        let transfer_id = message.seq_num();
        env.approve_token_transfer(message, signatures);
        env.claim_and_transfer_token<ETH>(source_chain, transfer_id);

        // block bridge node 0
        let chain_id = env.chain_id();
        let node_key = ecdsa_pub_key_to_eth_address(env.validators()[0].public_key());
        env.execute_blocklist(@0x0, chain_id, 0, vector[node_key]);

        // signing with 2 valid bridge nodes works
        let message = env.bridge_in_message<ETH>(
            source_chain,
            eth_address,
            sui_address,
            amount,
        );
        let signatures = env.sign_message_with(message, vector[1, 2]);
        env.approve_token_transfer(message, signatures);
        env.approve_token_transfer(message, signatures);

        // signing with blocked node fails
        let message = env.bridge_in_message<ETH>(
            source_chain,
            eth_address,
            sui_address,
            amount,
        );
        let signatures = env.sign_message_with(message, vector[0, 2]);
        env.approve_token_transfer(message, signatures);

        env.destroy_env();
    }
}
