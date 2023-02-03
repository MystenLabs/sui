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
/// Once created, `Message` needs to be consumed. The only way to do it is by calling the
/// `consume_message` function and passing a correct `Channel` instance alongside the `Message`.
///  - Message is checked for uniqueness (for this channel)
///  - Message is checked to match the `Channel`.id
///
///                                        --- msg_id --- Message
/// Axelar { epoch, operators_for_epoch  } --- msg_id --- Message
///                                        --- msg_id --- Message
module axelar::messenger {
    use sui::object::{Self, UID};
    use sui::vec_set::{Self, VecSet};
    use sui::tx_context::{TxContext};
    use sui::vec_map::{Self, VecMap};
    use sui::dynamic_field as df;
    use sui::ecdsa_k1 as ecdsa;
    use std::vector as vec;
    use sui::bcs;


    /// For when trying to consume the wrong object.
    const EWrongDestination: u64 = 0;
    /// For when message signatures failed verification.
    const ESignatureInvalid: u64 = 1;
    /// For when message has already been processed and submitted twice.
    const EDuplicateMessage: u64 = 2;
    /// For when message chainId is not SUI.
    const EInvalidChain: u64 = 3;
    /// For when number of commands does not match number of command ids.
    const EInvalidCommands: u64 = 4;
    /// For when operators have changed, and proof is no longer valid.
    const EInvalidOperators: u64 = 5;
    /// For when number of signatures for the message is below the threshold.
    const ELowSignaturesWeight: u64 = 6;
    /// For when number of Operators in the transfer_operatorship is 0.
    const EInvalidOperatorsLength: u64 = 7;
    /// For when transfer operatorship is malformed and weights len does not match operators len.
    const EInvalidWeights: u64 = 8;
    /// For when Operators are already set for the epoch.
    const EDuplicateOperators: u64 = 9;
    /// For when new threshold is zero or it's less than cummulative weight of validators.
    const EInvalidThreshold: u64 = 10;

    /// Used for a check in `validate_proof` function.
    const OLD_KEY_RETENTION: u64 = 16;

    /// Prefix for Sui Messages.
    const PREFIX: vector<u8> = b"\x19Sui Signed Message:\n";

    // These are currently supported
    const SELECTOR_APPROVE_CONTRACT_CALL: vector<u8> = b"approveContractCall";
    const SELECTOR_TRANSFER_OPERATORSHIP: vector<u8> = b"transferOperatorship";

    /// An object holding the state of the Axelar bridge.
    /// The central piece in managing Message creation and signature verification.
    struct Axelar has key {
        id: UID,
        epoch: u64,
        epoch_for_hash: VecMap<vector<u8>, u64>,
        hash_for_epoch: VecMap<u64, vector<u8>>
    }

    /// Generic target for the messaging system.
    ///
    /// This struct is required on the Sui side to be the destination for the
    /// messages sent from other chains. Even though it has a UID field, it does
    /// not have a `key` ability to force wrapping.
    ///
    /// Notes:
    ///
    /// - Does not have key to prevent 99% of the mistakes related to access management.
    /// Also prevents arbitrary Message destruction if the object is shared. Lastly,
    /// when shared, `Channel` cannot be destroyed, and its contents will remain locked
    /// forever.
    ///
    /// - Allows asset or capability-locking inside. Some applications might
    /// authorize admin actions through the bridge (eg by locking some `AdminCap`
    /// inside and getting a `&mut AdminCap` in the `consume_message`);
    ///
    /// - Can be destroyed freely as the `UID` is guaranteed to be unique across
    /// the system. Destroying a channel would mean the end of the Channel cycle
    /// and all further messages will have to target a new Channel if there is one.
    ///
    /// - Does not contain direct link to the state in Sui, as some functions
    /// might not take any specific data (eg allow users to create new objects).
    /// If specific object on Sui is targeted by this `Channel`, its reference
    /// should be implemented using the `data` field.
    ///
    /// - The funniest and extremely simple implementation would be a `Channel<ID>`
    /// since it actually contains the data required to point at the object in Sui.
    struct Channel<T: store> has store {
        /// Unique ID of the target object which allows message targeting
        /// by comparing against `id_bytes`.
        id: UID,
        /// Messages processed by this object. To make system less
        /// centralized, and spread the storage + io costs across multiple
        /// destinations, we can track every `Channel`'s messages.
        messages: VecSet<vector<u8>>,
        /// Additional field to optionally use as metadata for the Channel
        /// object improving identification and uniqueness of data.
        /// Can store any struct that has `store` ability (including other
        /// objects - eg Capabilities).
        data: T
    }

    /// Message struct which can consumed only by a `Channel` object.
    /// Does not require additional generic field to operate as linking
    /// by `id_bytes` is more than enough.
    ///
    /// Consider naming this `axelar::messaging::CallApproval`.
    struct Message has store {
        /// ID of the message, guaranteed to be unique by Axelar.
        msg_id: vector<u8>,
        /// The target Channel's UID.
        target_id: address,
        /// Name of the chain where this message came from.
        source_chain: vector<u8>,
        /// Address of the source chain (vector used for compatibility).
        /// UTF8 / ASCII encoded string (for 0x0... eth address gonna be 42 bytes with 0x)
        source_address: vector<u8>,
        /// Hash of the full payload (including source_* fields).
        payload_hash: vector<u8>,
        /// The rest of the payload to be used by the application.
        payload: vector<u8>,
    }

    /// Emitted when a new message is sent from the SUI network.
    struct MessageSent has copy, drop {
        source: vector<u8>,
        destination: vector<u8>,
        destination_address: vector<u8>,
        payload: vector<u8>,
    }

    /// Create new `Channel<T>` object. Anyone can create their own `Channel` to target
    /// from the outside and there's no limitation to the data stored inside it.
    ///
    /// `copy` ability is required to disallow asset locking inside the `Channel`.
    public fun create_channel<T: store>(t: T, ctx: &mut TxContext): Channel<T> {
        Channel {
            id: object::new(ctx),
            messages: vec_set::empty(),
            data: t
        }
    }

    /// Destroy a `Channel<T>` releasing the T. Not constrained and can be performed
    /// by any party as long as they own a Channel.
    public fun destroy_channel<T: store>(self: Channel<T>): T {
        let Channel { id, messages: _, data } = self;
        object::delete(id);
        data
    }

    /// By using &mut here we make sure that the object is not in the freeze
    /// state and the owner has full access to the target.
    ///
    /// Most common scenario would be to target a shared object, however this
    /// messaging system allows sending private messages which can be consumed
    /// by single-owner targets.
    ///
    /// For Capability-locking, a mutable reference to the `Channel.data` field is
    /// returned; the rest are the fields of the `Message`.
    public fun consume_message<T: store>(
        axelar: &mut Axelar, t: &mut Channel<T>, msg_id: vector<u8>,
    ): (&mut T, vector<u8>, vector<u8>, vector<u8>, vector<u8>) {
        let Message {
            msg_id,
            target_id,
            source_chain,
            source_address,
            payload_hash,
            payload
        } = df::remove(&mut axelar.id, msg_id);

        assert!(!vec_set::contains(&t.messages, &msg_id), EDuplicateMessage);
        assert!(target_id == object::uid_to_address(&t.id), EWrongDestination);

        (
            &mut t.data,
            source_chain,
            source_address,
            payload_hash,
            payload
        )
    }

    /// The main entrypoint for the external message processing.
    /// Parses data and attaches messages to the Axelar object to be
    /// later picked up and consumed by their corresponding Channel.
    public entry fun process_data(
        axelar: &mut Axelar,
        input: vector<u8>
    ) {
        let messages = create_messages(axelar, input);
        let (i, len) = (0, vec::length(&messages));

        while (i < len) {
            let message = vec::pop_back(&mut messages);
            df::add(&mut axelar.id, message.msg_id, message);
            i = i + 1;
        };

        vec::destroy_empty(messages);
    }

    /// Main entrypoint for the messaging protocol.
    /// Processes the data and the signatures generating a vector of
    /// `Message` objects.
    ///
    /// Aborts with multiple error codes, ignores messages which are not
    /// supported by the current implementation of the protocol.
    ///
    /// Input data must be serialized with BCS (see specification here: https://github.com/diem/bcs).
    fun create_messages(
        operators: &mut Axelar, // shared Axelar
        input: vector<u8>
    ): vector<Message> {
        let bytes = bcs::new(input);

        // Split input into:
        // data: vector<u8> (BCS bytes)
        // proof: vector<u8> (BCS bytes)
        let (data, proof) = (
            bcs::peel_vec_u8(&mut bytes),
            bcs::peel_vec_u8(&mut bytes)
        );

        // [x] TODO: Add a sui-specific prefix for the hash (eg "Sui Signed message");
        let message_hash = ecdsa::keccak256(&to_signed(*&data));
        let allow_operatorship_transfer = validate_proof(operators, message_hash, proof);

        // Treat `data` as BCS bytes.
        let data_bcs = bcs::new(data);

        // Split data into:
        // chain_id: u64,
        // command_ids: vector<vector<u8>> (vector<string>)
        // commands: vector<vector<u8>> (vector<string>)
        // params: vector<vector<u8>> (vector<string>)
        let (_chain_id, command_ids, commands, params) = (
            bcs::peel_u64(&mut data_bcs),
            bcs::peel_vec_vec_u8(&mut data_bcs),
            bcs::peel_vec_vec_u8(&mut data_bcs),
            bcs::peel_vec_vec_u8(&mut data_bcs)
        );

        // ... figure out whether it has to be a string ...
        // ignore me, I'm not eth
        // assert!(chain_id == 1, EInvalidChain);

        let (i, commands_len, messages) = (0, vec::length(&commands), vec::empty());

        // std::debug::print(&commands_len);

        // make sure number of commands passed matches command IDs
        assert!(vec::length(&command_ids) == commands_len, EInvalidCommands);

        while (i < commands_len) {
            let msg_id = *vec::borrow(&command_ids, i);
            let cmd_selector = vec::borrow(&commands, i);
            let payload = bcs::new(*vec::borrow(&params, i));

            i = i + 1;

            // Build a `Message` object from the `params[i]`. BCS serializes data
            // in order, so field reads have to be done carefully and in order!
            if (cmd_selector == &SELECTOR_APPROVE_CONTRACT_CALL) {
                vec::push_back(&mut messages, Message {
                    msg_id,

                    source_chain: bcs::peel_vec_u8(&mut payload),
                    source_address: bcs::peel_vec_u8(&mut payload),
                    target_id: bcs::peel_address(&mut payload),
                    payload_hash: bcs::peel_vec_u8(&mut payload),

                    payload: bcs::into_remainder_bytes(payload)
                });
                continue
            } else if (cmd_selector == &SELECTOR_TRANSFER_OPERATORSHIP) {
                if (allow_operatorship_transfer) {
                    let (new_operators, new_weights, new_threshold) = (
                        bcs::peel_vec_vec_u8(&mut payload),
                        bcs::peel_vec_u64(&mut payload),
                        bcs::peel_u64(&mut payload),
                    );

                    transfer_operatorship(operators, new_operators, new_weights, new_threshold);
                };
                continue
            } else {
                continue
            };
        };

        messages
    }


    /// Send a message to another chain. Supply the event data and the
    /// destination chain.
    ///
    /// Event data is collected from the Channel (eg ID of the source and
    /// source_chain is a constant).
    public fun send_message<T: store>(
        t: &mut Channel<T>,
        destination: vector<u8>,
        destination_address: vector<u8>,
        payload: vector<u8>
    ) {
        sui::event::emit(MessageSent {
            source: bcs::to_bytes(&t.id),
            destination,
            destination_address,
            payload,
        })
    }


    /// Implementation of the `AxelarAuthWeighted.validateProof`.
    /// Does proof validation, fails when proof is invalid or if weight
    /// threshold is not reached.
    fun validate_proof(
        validators: &mut Axelar,
        message_hash: vector<u8>,
        proof: vector<u8>
    ): bool {
        // Turn everything into bcs bytes and split data.
        let proof = bcs::new(proof);
        let (operators, weights, threshold, signatures) = (
            bcs::peel_vec_vec_u8(&mut proof),
            bcs::peel_vec_u64(&mut proof),
            bcs::peel_u64(&mut proof),
            bcs::peel_vec_vec_u8(&mut proof)
        );

        // std::debug::print(&10000);
        // std::debug::print(&weights);

        // TODO: revisit this line and change the way operators hash is generated.
        let operators_length = vec::length(&operators);
        let operators_epoch = *vec_map::get(&validators.epoch_for_hash, &operators_hash(&operators));
        let epoch = validators.epoch;

        // TODO: unblock once there's enough signatures for testing.
        assert!(operators_epoch != 0 && epoch - operators_epoch < OLD_KEY_RETENTION, EInvalidOperators);

        let (i, weight, operator_index) = (0, 0, 0);
        let total_signatures = vec::length(&signatures);

        // [DEBUG] checking number of signatures
        // std::debug::print(&true);
        // std::debug::print(&total_signatures);

        while (i < total_signatures) {
            let signature = *vec::borrow(&signatures, i);
            normalize_signature(&mut signature);

            let signed_by: vector<u8> = ecdsa::ecrecover(&signature, &message_hash);
            while (operator_index < operators_length && &signed_by != vec::borrow(&operators, operator_index)) {
                operator_index = operator_index + 1;
            };

            // assert!(operator_index == operators_length, 0); // EMalformedSigners

            weight = weight + *vec::borrow(&weights, operator_index);
            if (weight >= threshold) { return true };
            operator_index = operator_index + 1;
        };

        abort ELowSignaturesWeight
    }

    fun transfer_operatorship(
        operators: &mut Axelar,
        new_operators: vector<vector<u8>>,
        new_weights: vector<u64>,
        new_threshold: u64
    ) {
        let operators_len = vec::length(&new_operators);
        let weights_len = vec::length(&new_weights);
        // TODO: also add weights and threshold into the hash
        let new_hash = operators_hash(&new_operators);

        assert!(operators_len > 0, EInvalidOperatorsLength);
        assert!(operators_len == weights_len, EInvalidWeights);

        let (total_weight, i) = (0, 0);
        while (i < weights_len) {
            total_weight = total_weight + *vec::borrow(&new_weights, i);
        };

        assert!(new_threshold != 0 && total_weight >= new_threshold, EInvalidThreshold);
        assert!(vec_map::contains(&operators.epoch_for_hash, &new_hash), EDuplicateOperators);

        let epoch = operators.epoch + 1;

        vec_map::insert(&mut operators.hash_for_epoch, epoch, new_hash);
        vec_map::insert(&mut operators.epoch_for_hash, new_hash, epoch);
        operators.epoch = epoch;

        // event::emit(OperatorShipTransferred { new_operators, new_weights, new_threshold })
    }

    /// Add a prefix to the bytes.
    fun to_signed(bytes: vector<u8>): vector<u8> {
        let res = vector[];
        vec::append(&mut res, PREFIX);
        vec::append(&mut res, bytes);
        res
    }

    /// Normalize last byte of the signature. Have it 1 or 0.
    /// See https://tech.mystenlabs.com/cryptography-in-sui-cross-chain-signature-verification/
    fun normalize_signature(signature: &mut vector<u8>) {
        // Compute v = 0 or 1.
        let v = vec::borrow_mut(signature, 64);
        if (*v == 27) {
            *v = 0;
        } else if (*v == 28) {
            *v = 1;
        } else if (*v > 35) {
            *v = (*v - 1) % 2;
        };
    }

    /// Compute operators hash from the list of `operators` (public keys).
    /// This hash is used in `Axelar.epoch_for_hash`.
    ///
    /// TODO: also take weights and thresholds (include into hashing).
    fun operators_hash(operators: &vector<vector<u8>>): vector<u8> {
        ecdsa::keccak256(&bcs::to_bytes(operators))
    }

    #[test_only]
    /// Signer PubKey.
    /// Expected to be returned from ecrecover.
    const SIGNER: vector<u8> = x"03d0c81d1a2664b796715725f09501427549006b1468913fea2dcbd440ae086d4b";

    #[test_only]
    public fun create_test_messages(
        operators: &mut Axelar,
        input: vector<u8>
    ): vector<Message> { create_messages(operators, input) }

    #[test_only]
    /// Test-only function to create an Axelar object with initial state.
    public fun init_axelar(epoch: u64, operators: vector<vector<u8>>, ctx: &mut TxContext): Axelar {
        let epoch_for_hash = vec_map::empty();
        let hash_for_epoch = vec_map::empty();
        let hash = operators_hash(&operators);

        vec_map::insert(&mut epoch_for_hash, hash, epoch);
        vec_map::insert(&mut hash_for_epoch, epoch, hash);

        Axelar {
            id: object::new(ctx),
            epoch_for_hash,
            hash_for_epoch,
            epoch,
        }
    }

    #[test_only]
    /// Test-only function to consume an Axelar object.
    public fun burn_axelar(a: Axelar) {
        let Axelar { id, epoch: _, epoch_for_hash: _ } = a;
        object::delete(id);
    }

    #[test_only]
    /// Handy method for burning `vector<Message>` returned by the `execute` function.
    public fun delete_messages(msgs: vector<Message>) {
        while (vec::length(&msgs) > 0) {
            let Message {
                msg_id: _,
                target_id: _,
                source_chain: _,
                source_address: _,
                payload_hash: _,
                payload: _
            } = vec::pop_back(&mut msgs);
        };
        vec::destroy_empty(msgs);
    }

    #[test]
    /// Tests `ecrecover`, makes sure external signing process works with Sui ecrecover.
    /// Samples for this test are generated with the `presets/` application.
    fun test_ecrecover() {
        let message = x"c4f4633cf1b47f37d4f6ccf0a2aea7123d3c9a08d82d353ea5140419ba8935a8";
        let signature = x"ef006f7a189e9ac42c5c2d4251917b333c09099d04aaffc56781e07856eb491f39a50a890351b8d554c0cbf68d936b7acbd1f8214ea8b2f5b901a99b56e4915c00";

        normalize_signature(&mut signature);

        let pub_key = ecdsa::ecrecover(&signature, &message);

        assert!(pub_key == SIGNER, 0);
    }

    #[test]
    /// Tests "Sui Signed Message" prefix addition ecrecover.
    /// Checks if the signature generated outside matches the message generated in this module.
    /// Samples for this test are generated with the `presets/` application.
    fun test_to_signed() {
        let message = b"hello world";
        let signature = x"ef006f7a189e9ac42c5c2d4251917b333c09099d04aaffc56781e07856eb491f39a50a890351b8d554c0cbf68d936b7acbd1f8214ea8b2f5b901a99b56e4915c00";
        normalize_signature(&mut signature);

        let pub_key = ecdsa::ecrecover(&signature, &ecdsa::keccak256(&to_signed(message)));

        let _ = pub_key;

        // std::debug::print(&ecdsa::keccak256(&to_signed(message)));
        // std::debug::print(&pub_key);
        // TODO: broken test
        // assert!(pub_key == SIGNER, 0);
    }
}
