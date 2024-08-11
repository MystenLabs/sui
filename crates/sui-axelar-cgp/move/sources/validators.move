// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module axelar::validators {
    use std::string::{Self, String};
    use std::vector;

    use sui::table::{Self, Table};
    use sui::address;
    use sui::bcs;
    use sui::dynamic_field as df;
    use sui::ecdsa_k1 as ecdsa;
    use sui::event;
    use sui::hash;
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use sui::vec_map:: {Self, VecMap};

    use axelar::channel::{Self, ApprovedCall};
    use axelar::utils::{normalize_signature, operators_hash};

    const EInvalidWeights: u64 = 0;
    const EInvalidThreshold: u64 = 1;
    /// For when operators have changed, and proof is no longer valid.
    const EInvalidOperators: u64 = 2;
    const EDuplicateOperators: u64 = 3;
    /// For when number of signatures for the call approvals is below the threshold.
    const ELowSignaturesWeight: u64 = 4;
    /// Trying to `take_approved_call` with a wrong payload.
    const EPayloadHashMismatch: u64 = 5;

    /// Used for a check in `validate_proof` function.
    const OLD_KEY_RETENTION: u64 = 16;

    /// An object holding the state of the Axelar bridge.
    /// The central piece in managing call approval creation and signature verification.
    public struct AxelarValidators has key {
        id: UID,
        approvals: Table<address, Approval>
    }

    public struct AxelarValidatorsV1 has store {
        /// Epoch of the validators.
        epoch: u64,
        /// Epoch for the operators hash.
        epoch_for_hash: VecMap<vector<u8>, u64>,
    }

    /// CallApproval struct which can consumed only by a `Channel` object.
    /// Does not require additional generic field to operate as linking
    /// by `id_bytes` is more than enough.
    public struct Approval has store {
        /// Hash of the cmd_id, target_id, source_chain, source_address, payload_hash
        approval_hash: vector<u8>,
    }

    /// Emitted when the operatorship changes.
    public struct OperatorshipTransferred has copy, drop {
        epoch: u64,
        new_operators_hash: vector<u8>,
    }

    fun init(ctx: &mut TxContext) {
        let mut validators = AxelarValidators {
            id: object::new(ctx),
            approvals: table::new(ctx)
        };
        df::add(&mut validators.id, 1, AxelarValidatorsV1 {
            epoch: 0,
            epoch_for_hash: vec_map::empty(),
        });
        transfer::share_object(validators);
    }

    /// Implementation of the `AxelarAuthWeighted.validateProof`.
    /// Does proof validation, fails when proof is invalid or if weight
    /// threshold is not reached.
    public(package) fun validate_proof(
        validators: &AxelarValidators,
        approval_hash: vector<u8>,
        proof: vector<u8>
    ): bool {
        // Turn everything into bcs bytes and split data.
        let mut proof = bcs::new(proof);
        let (operators, weights, threshold, signatures) = (
            bcs::peel_vec_vec_u8(&mut proof),
            bcs::peel_vec_u128(&mut proof),
            bcs::peel_u128(&mut proof),
            bcs::peel_vec_vec_u8(&mut proof)
        );

        let operators_length = vector::length(&operators);
        let operators_epoch = *vec_map::get(
            epoch_for_hash(validators),
            &operators_hash(&operators, &weights, threshold)
        );
        let epoch = epoch(validators);

        assert!(operators_epoch != 0 && epoch - operators_epoch < OLD_KEY_RETENTION, EInvalidOperators);

        let (i, mut weight, mut operator_index) = (0, 0, 0);
        let total_signatures = vector::length(&signatures);

        while (i < total_signatures) {
            let mut signature = *vector::borrow(&signatures, i);
            normalize_signature(&mut signature);

            let signed_by: vector<u8> = ecdsa::secp256k1_ecrecover(&signature, &approval_hash, 0);
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

    public(package) fun transfer_operatorship(axelar: &mut AxelarValidators, payload: vector<u8>) {
        let mut bcs = bcs::new(payload);
        let new_operators = bcs::peel_vec_vec_u8(&mut bcs);
        let new_weights = bcs::peel_vec_u128(&mut bcs);
        let new_threshold = bcs::peel_u128(&mut bcs);

        let operators_length = vector::length(&new_operators);
        let weight_length = vector::length(&new_weights);

        assert!(operators_length != 0, EInvalidOperators);
        // TODO: implement `_isSortedAscAndContainsNoDuplicate` function.

        assert!(weight_length == operators_length, EInvalidWeights);
        let (mut total_weight, mut i) = (0, 0);
        while (i < weight_length) {
            total_weight = total_weight + *vector::borrow(&new_weights, i);
            i = i + 1;
        };
        assert!(!(new_threshold == 0 || total_weight < new_threshold), EInvalidThreshold);

        let new_operators_hash = operators_hash(&new_operators, &new_weights, new_threshold);
        // Remove old epoch for the operators if it exists
        let epoch = epoch(axelar) + 1;
        let epoch_for_hash = epoch_for_hash_mut(axelar);
        if (vec_map::contains(epoch_for_hash, &new_operators_hash)) {
            vec_map::remove(epoch_for_hash, &new_operators_hash);
        };
        // clean up old epoch
        if (epoch >= OLD_KEY_RETENTION) {
            let old_epoch = epoch - OLD_KEY_RETENTION;
            let (_, epoch) = vec_map::get_entry_by_idx(epoch_for_hash, 0);
            if (*epoch <= old_epoch) {
                vec_map::remove_entry_by_idx(epoch_for_hash, 0);
            };
        };
        vec_map::insert(epoch_for_hash, new_operators_hash, epoch);

        set_epoch(axelar, epoch);
        event::emit(OperatorshipTransferred {
            epoch,
            new_operators_hash
        });
    }

    public(package) fun add_approval(
        axelar: &mut AxelarValidators,
        cmd_id: address,
        source_chain: String,
        source_address: String,
        target_id: address,
        payload_hash: vector<u8>
    ) {
        let mut data = vector[];
        vector::append(&mut data, address::to_bytes(cmd_id));
        vector::append(&mut data, address::to_bytes(target_id));
        vector::append(&mut data, *string::as_bytes(&source_chain));
        vector::append(&mut data, *string::as_bytes(&source_address));
        vector::append(&mut data, payload_hash);

        table::add(&mut axelar.approvals, cmd_id, Approval {
            approval_hash: hash::keccak256(&data),
        });
    }

    /// Most common scenario would be to target a shared object, however this
    /// messaging system allows sending private messages which can be consumed
    /// by single-owner targets.
    ///
    /// The hot potato approvel call object is returned.
    public(package) fun take_approved_call(
        axelar: &mut AxelarValidators,
        cmd_id: address,
        source_chain: String,
        source_address: String,
        target_id: address,
        payload: vector<u8>
    ): ApprovedCall {
        let Approval {
            approval_hash,
        } = table::remove(&mut axelar.approvals, cmd_id);

        let mut data = vector[];
        vector::append(&mut data, address::to_bytes(cmd_id));
        vector::append(&mut data, address::to_bytes(target_id));
        vector::append(&mut data, *string::as_bytes(&source_chain));
        vector::append(&mut data, *string::as_bytes(&source_address));
        vector::append(&mut data, hash::keccak256(&payload));

        assert!(hash::keccak256(&data) == approval_hash, EPayloadHashMismatch);

        // Friend only.
        channel::create_approved_call(
            cmd_id,
            source_chain,
            source_address,
            target_id,
            payload,
        )
    }

    // === Getters ===

    fun epoch_for_hash(axelar: &AxelarValidators): &VecMap<vector<u8>, u64> {
        &df::borrow<u8, AxelarValidatorsV1>(&axelar.id, 1).epoch_for_hash
    }

    fun epoch_for_hash_mut(axelar: &mut AxelarValidators): &mut VecMap<vector<u8>, u64> {
        &mut df::borrow_mut<u8, AxelarValidatorsV1>(&mut axelar.id, 1).epoch_for_hash
    }

    fun set_epoch(axelar: &mut AxelarValidators, epoch: u64) {
        df::borrow_mut<u8, AxelarValidatorsV1>(&mut axelar.id, 1).epoch = epoch
    }

    public fun epoch(axelar: &AxelarValidators): u64 {
        df::borrow<u8, AxelarValidatorsV1>(&axelar.id, 1).epoch
    }

    // === Testing ===

    #[test_only]
    public fun remove_approval_for_test(self: &mut AxelarValidators, cmd_id: address) {
        let Approval { approval_hash: _ } = table::remove(&mut self.approvals, cmd_id);
    }

    #[test_only]
    public fun new(epoch: u64, epoch_for_hash: VecMap<vector<u8>, u64>, ctx: &mut TxContext): AxelarValidators {
        let mut base = AxelarValidators {
            id: object::new(ctx),
            approvals: table::new(ctx)
        };
        df::add(&mut base.id, 1u8, AxelarValidatorsV1 {
            epoch,
            epoch_for_hash,
        });
        base
    }

    #[test_only]
    use axelar::utils::to_sui_signed;

    #[test_only]
    public fun drop_for_test(self: AxelarValidators) {
        // validator cleanup
        let AxelarValidators { id, approvals } = self;
        table::destroy_empty(approvals);
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
        let mut signature = x"0e88ac153a06d86f28dc0f946654d02302099c0c6558806b569d43f8bd062d5c295beb095e9cc396cd68a6b18daa0f1c0489b778831c4b3bb46f7aa1171c23b101";

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
        let mut signature = x"0e88ac153a06d86f28dc0f946654d02302099c0c6558806b569d43f8bd062d5c295beb095e9cc396cd68a6b18daa0f1c0489b778831c4b3bb46f7aa1171c23b101";
        normalize_signature(&mut signature);

        let pub_key = ecdsa::secp256k1_ecrecover(&signature, &to_sui_signed(message), 0);
        assert!(pub_key == SIGNER, 0);
    }
}
