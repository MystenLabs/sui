// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implementation a cross-chain messaging system for Axelar.
///
/// This code is based on the following:
///
/// - When message is sent to Sui, it targets an object and not a module;
/// - To support cross-chain messaging, a Channel object has to be created;
/// - Channel can be either owned or shared but not frozen;
/// - Module developer on the Sui side will have to implement a system to support messaging;
/// - Checks for uniqueness of messages should be done through `Channel`s to avoid big data storage;
///
/// I. Sending messages
///
/// A message is sent through the `send` function, a Channel is supplied to determine the source -> ID.
/// Event is then emitted and Axelar network can operate
///
/// II. Receiving messages
///
/// Message bytes and signatures are passed into `create` function to generate a Message object.
///  - Signatures are checked against the known set of validators.
///  - Message bytes are parsed to determine: source, destination_chain, payload and target_id
///  - `target_id` points to a `Channel` object
///
/// Once created, `Message` needs to be consumed. And the only way to do it is by calling
/// `consume_message` function and pass a correct `Channel` instance alongside the `Message`.
///  - Message is checked for uniqueness (for this channel)
///  - Message is checked to match the `Channel`.id
///
module axelar::gateway {
    use std::vector;

    use axelar::axelar;
    use axelar::axelar::{Axelar, validate_proof};
    use axelar::message;
    use axelar::message::Message;
    use axelar::utils::to_sui_signed;
    use sui::bcs;

    #[test_only]
    use axelar::utils::operators_hash;
    #[test_only]
    use sui::vec_map;

    /// For when message signatures failed verification.
    const ESignatureInvalid: u64 = 1;

    /// For when number of commands does not match number of command ids.
    const EInvalidCommands: u64 = 4;

    /// For when message chainId is not SUI.
    const EInvalidChain: u64 = 3;

    // These are currently supported
    const SELECTOR_APPROVE_CONTRACT_CALL: vector<u8> = b"approveContractCall";
    const SELECTOR_TRANSFER_OPERATORSHIP: vector<u8> = b"transferOperatorship";

    /// The main entrypoint for the external message processing.
    /// Parses data and attaches messages to the Axelar object to be
    /// later picked up and consumed by their corresponding Channel.
    public fun process_messages(
        axelar: &mut Axelar,
        input: vector<u8>
    ) {
        let messages = validate_and_create_messages(axelar, input);
        let (i, len) = (0, vector::length(&messages));

        while (i < len) {
            axelar::add_message(axelar, vector::pop_back(&mut messages));
            i = i + 1;
        };
        vector::destroy_empty(messages);
    }

    /// Processes the data and the signatures generating a vector of
    /// `Message` objects.
    ///
    /// Aborts with multiple error codes, ignores messages which are not
    /// supported by the current implementation of the protocol.
    ///
    /// Input data must be serialized with BCS (see specification here: https://github.com/diem/bcs).
    fun validate_and_create_messages(validators: &mut Axelar, input: vector<u8>): vector<Message> {
        let bytes = bcs::new(input);
        // Split input into:
        // data: vector<u8> (BCS bytes)
        // proof: vector<u8> (BCS bytes)
        let (data, proof) = (
            bcs::peel_vec_u8(&mut bytes),
            bcs::peel_vec_u8(&mut bytes)
        );

        let allow_operatorship_transfer = validate_proof(validators, to_sui_signed(*&data), proof);

        // Treat `data` as BCS bytes.
        let data_bcs = bcs::new(data);

        // Split data into:
        // chain_id: u64,
        // command_ids: vector<vector<u8>> (vector<string>)
        // commands: vector<vector<u8>> (vector<string>)
        // params: vector<vector<u8>> (vector<byteArray>)
        let (chain_id, command_ids, commands, params) = (
            bcs::peel_u64(&mut data_bcs),
            bcs::peel_vec_vec_u8(&mut data_bcs),
            bcs::peel_vec_vec_u8(&mut data_bcs),
            bcs::peel_vec_vec_u8(&mut data_bcs)
        );

        assert!(chain_id == 1, EInvalidChain);

        let (i, commands_len, messages) = (0, vector::length(&commands), vector::empty());

        // make sure number of commands passed matches command IDs
        assert!(vector::length(&command_ids) == commands_len, EInvalidCommands);

        while (i < commands_len) {
            let msg_id = *vector::borrow(&command_ids, i);
            let cmd_selector = vector::borrow(&commands, i);
            let payload = *vector::borrow(&params, i);
            i = i + 1;

            // Build a `Message` object from the `params[i]`. BCS serializes data
            // in order, so field reads have to be done carefully and in order!
            if (cmd_selector == &SELECTOR_APPROVE_CONTRACT_CALL) {
                let payload = bcs::new(payload);
                vector::push_back(&mut messages, message::create(
                    msg_id,
                    bcs::peel_vec_u8(&mut payload),
                    bcs::peel_vec_u8(&mut payload),
                    bcs::peel_address(&mut payload),
                    bcs::peel_vec_u8(&mut payload),
                    bcs::into_remainder_bytes(payload)
                ));
                continue
            } else if (cmd_selector == &SELECTOR_TRANSFER_OPERATORSHIP) {
                if (!allow_operatorship_transfer) {
                    continue
                };
                allow_operatorship_transfer = false;
                axelar::transfer_operatorship(validators, payload)
            } else {
                continue
            };
        };
        messages
    }


    #[test_only]
    /// Test message for the `test_execute` test.
    /// Generated via the `presets` script.
    const MESSAGE: vector<u8> = x"af0101000000000000000209726f6775655f6f6e650a6178656c61725f74776f0213617070726f7665436f6e747261637443616c6c13617070726f7665436f6e747261637443616c6c02310345544803307830000000000000000000000000000000000000000000000000000000000000040000000005000000000034064158454c415203307831000000000000000000000000000000000000000000000000000000000000040000000005000000000087010121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902801640000000000000000000000000000000a0000000000000000000000000000000141dcfc40d95cc89a9c8a0973c3dae95806c5daa5aefe072caafd5541844d62fabf2dc580a8663df7adb846f1ef7d553a13174399e4c4cb55c42bdf7fa8f02c8fa10000";
    const TRANSFER_OPERATORSHIP_MESSAGE: vector<u8> = x"6f01000000000000000109726f6775655f6f6e6501147472616e736665724f70657261746f727368697001440121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902801c80000000000000000000000000000001400000000000000000000000000000087010121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902801640000000000000000000000000000000a00000000000000000000000000000001414b88c29db7550c18fac63470891ddd8460e7d44d8d27bf1528758de03515c2a4327b07bc582732b80b7aa8d15964a4878ce203430661ce3d096afaea791189860000";

    #[test]
    /// Tests execution with a set of validators.
    /// Samples for this test are generated with the `presets/` application.
    fun test_execute() {
        use sui::test_scenario::{Self as ts, ctx};

        // public keys of `operators`
        let epoch = 1;
        let operators = vector[
            x"037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff599028"
        ];

        let epoch_for_hash = vec_map::empty();
        vec_map::insert(&mut epoch_for_hash, operators_hash(&operators, &vector[100u128], 10u128), epoch);

        let test = ts::begin(@0x0);

        // create validators for testing
        let axelar = axelar::new(
            epoch,
            epoch_for_hash,
            ctx(&mut test)
        );

        let messages = validate_and_create_messages(&mut axelar, MESSAGE);
        axelar::delete(axelar);
        message::delete(messages);
        ts::end(test);
    }

    #[test]
    fun test_transfer_operatorship() {
        use sui::test_scenario::{Self as ts, ctx};

        // public keys of `operators`
        let epoch = 1;
        let operators = vector[
            x"037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff599028"
        ];

        let epoch_for_hash = vec_map::empty();
        vec_map::insert(&mut epoch_for_hash, operators_hash(&operators, &vector[100u128], 10u128), epoch);

        let test = ts::begin(@0x0);

        // create validators for testing
        let axelar = axelar::new(
            epoch,
            epoch_for_hash,
            ctx(&mut test)
        );

        let messages = validate_and_create_messages(&mut axelar, TRANSFER_OPERATORSHIP_MESSAGE);

        assert!(axelar::epoch(&axelar) == 2, 0);

        axelar::delete(axelar);
        message::delete(messages);
        ts::end(test);
    }
}

module axelar::axelar {
    use std::vector;

    use axelar::message;
    use axelar::message::Message;
    use axelar::utils::{normalize_signature, operators_hash};
    use sui::bcs;
    use sui::dynamic_field as df;
    use sui::ecdsa_k1 as ecdsa;
    use sui::object::UID;
    use sui::vec_map;
    use sui::vec_map::VecMap;

    #[test_only]
    use axelar::utils::to_sui_signed;
    #[test_only]
    use sui::object;
    #[test_only]
    use sui::tx_context::TxContext;

    friend axelar::gateway;

    const EInvalidWeights: u64 = 0;
    const EInvalidThreshold: u64 = 1;
    /// For when operators have changed, and proof is no longer valid.
    const EInvalidOperators: u64 = 2;
    const EDuplicateOperators: u64 = 3;
    /// For when number of signatures for the message is below the threshold.
    const ELowSignaturesWeight: u64 = 4;

    /// Used for a check in `validate_proof` function.
    const OLD_KEY_RETENTION: u64 = 16;

    /// An object holding the state of the Axelar bridge.
    /// The central piece in managing Message creation and signature verification.
    struct Axelar has key {
        // Auth Weighted
        id: UID,
        epoch: u64,
        epoch_for_hash: VecMap<vector<u8>, u64>,
    }

    /// Implementation of the `AxelarAuthWeighted.validateProof`.
    /// Does proof validation, fails when proof is invalid or if weight
    /// threshold is not reached.
    public fun validate_proof(
        validators: &Axelar,
        message_hash: vector<u8>,
        proof: vector<u8>
    ): bool {
        // Turn everything into bcs bytes and split data.
        let proof = bcs::new(proof);
        let (operators, weights, threshold, signatures) = (
            bcs::peel_vec_vec_u8(&mut proof),
            bcs::peel_vec_u128(&mut proof),
            bcs::peel_u128(&mut proof),
            bcs::peel_vec_vec_u8(&mut proof)
        );

        let operators_length = vector::length(&operators);
        let operators_epoch = *vec_map::get(
            &validators.epoch_for_hash,
            &operators_hash(&operators, &weights, threshold)
        );
        let epoch = validators.epoch;

        assert!(operators_epoch != 0 && epoch - operators_epoch < OLD_KEY_RETENTION, EInvalidOperators);

        let (i, weight, operator_index) = (0, 0, 0);
        let total_signatures = vector::length(&signatures);

        while (i < total_signatures) {
            let signature = *vector::borrow(&signatures, i);
            normalize_signature(&mut signature);

            let signed_by: vector<u8> = ecdsa::secp256k1_ecrecover(&signature, &message_hash, 0);
            while (operator_index < operators_length && &signed_by != vector::borrow(&operators, operator_index)) {
                operator_index = operator_index + 1;
            };

            // assert!(operator_index == operators_length, 0); // EMalformedSigners

            weight = weight + *vector::borrow(&weights, operator_index);
            if (weight >= threshold) { return true };
            operator_index = operator_index + 1;
        };

        abort ELowSignaturesWeight
    }

    public(friend) fun transfer_operatorship(axelar: &mut Axelar, payload: vector<u8>) {
        let bcs = bcs::new(payload);
        let new_operators = bcs::peel_vec_vec_u8(&mut bcs);
        let new_weights = bcs::peel_vec_u128(&mut bcs);
        let new_threshold = bcs::peel_u128(&mut bcs);

        let operators_length = vector::length(&new_operators);
        let weight_length = vector::length(&new_weights);

        assert!(operators_length != 0, EInvalidOperators);
        // TODO: implement `_isSortedAscAndContainsNoDuplicate` function.

        assert!(weight_length == operators_length, EInvalidWeights);
        let (total_weight, i) = (0, 0);
        while (i < weight_length) {
            total_weight = total_weight + *vector::borrow(&new_weights, i);
            i = i + 1;
        };
        assert!(!(new_threshold == 0 || total_weight < new_threshold), EInvalidThreshold);

        // TODO: Can we assume that the new operators hash won't collide with the old ones?
        let new_operators_hash = operators_hash(&new_operators, &new_weights, new_threshold);

        assert!(!vec_map::contains(&axelar.epoch_for_hash, &new_operators_hash), EDuplicateOperators);

        let epoch = axelar.epoch + 1;
        axelar.epoch = epoch;

        // clean up old epoch
        if (epoch >= OLD_KEY_RETENTION) {
            let old_epoch = epoch - OLD_KEY_RETENTION;
            let (_, epoch) = vec_map::get_entry_by_idx(&mut axelar.epoch_for_hash, 0);
            if (*epoch <= old_epoch) {
                vec_map::remove_entry_by_idx(&mut axelar.epoch_for_hash, 0);
            };
        };

        vec_map::insert(&mut axelar.epoch_for_hash, new_operators_hash, epoch);
    }

    #[test_only]
    public fun new(epoch: u64, epoch_for_hash: VecMap<vector<u8>, u64>, ctx: &mut TxContext): Axelar {
        Axelar {
            id: object::new(ctx),
            epoch_for_hash,
            epoch,
        }
    }

    #[test_only]
    public fun delete(self: Axelar) {
        // validator cleanup
        let Axelar { id, epoch: _, epoch_for_hash: _ } = self;
        object::delete(id);
    }

    #[test_only]
    /// Test message for the `test_execute` test.
    /// Generated via the `presets` script.
    const MESSAGE: vector<u8> = x"af0101000000000000000209726f6775655f6f6e650a6178656c61725f74776f0213617070726f7665436f6e747261637443616c6c13617070726f7665436f6e747261637443616c6c02310345544803307830000000000000000000000000000000000000000000000000000000000000040000000005000000000034064158454c4152033078310000000000000000000000000000000000000000000000000000000000000400000000050000000000770121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff5990280164000000000000000a000000000000000141dcfc40d95cc89a9c8a0973c3dae95806c5daa5aefe072caafd5541844d62fabf2dc580a8663df7adb846f1ef7d553a13174399e4c4cb55c42bdf7fa8f02c8fa10000";

    #[test_only]
    /// Signer PubKey.
    /// Expected to be returned from ecrecover.
    const SIGNER: vector<u8> = x"037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff599028";

    #[test]
    /// Tests `ecrecover`, makes sure external signing process works with Sui ecrecover.
    /// Samples for this test are generated with the `presets/` application.
    fun test_ecrecover() {
        let message = x"68656c6c6f20776f726c64"; // hello world
        let signature = x"0e88ac153a06d86f28dc0f946654d02302099c0c6558806b569d43f8bd062d5c295beb095e9cc396cd68a6b18daa0f1c0489b778831c4b3bb46f7aa1171c23b101";

        normalize_signature(&mut signature);
        let pubkey = ecdsa::secp256k1_ecrecover(&signature, &to_sui_signed(message), 0);

        assert!(pubkey == SIGNER, 0);
    }

    #[test]
    /// Tests "Sui Signed Message" prefix addition ecrecover.
    /// Checks if the signature generated outside matches the message generated in this module.
    /// Samples for this test are generated with the `presets/` application.
    fun test_to_signed() {
        let message = b"hello world";
        let signature = x"0e88ac153a06d86f28dc0f946654d02302099c0c6558806b569d43f8bd062d5c295beb095e9cc396cd68a6b18daa0f1c0489b778831c4b3bb46f7aa1171c23b101";
        normalize_signature(&mut signature);

        let pub_key = ecdsa::secp256k1_ecrecover(&signature, &to_sui_signed(message), 0);
        assert!(pub_key == SIGNER, 0);
    }

    public fun add_message(axelar: &mut Axelar, msg: Message) {
        df::add(&mut axelar.id, message::msg_id(&msg), msg);
    }

    public fun remove_message(axelar: &mut Axelar, msg_id: vector<u8>): Message {
        df::remove(&mut axelar.id, msg_id)
    }

    public fun epoch_for_hash(axelar: &Axelar): &VecMap<vector<u8>, u64> {
        &axelar.epoch_for_hash
    }

    public fun epoch(axelar: &Axelar): u64 {
        axelar.epoch
    }

    public fun set_epoch(axelar: &mut Axelar, epoch: u64) {
        axelar.epoch = epoch
    }
}
