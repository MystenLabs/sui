// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implementation a cross-chain messaging system for Axelar.
///
/// This code is based on the following:
///
/// - When call approvals is sent to Sui, it targets an object and not a module;
/// - To support cross-chain messaging, a Channel object has to be created;
/// - Channel can be either owned or shared but not frozen;
/// - Module developer on the Sui side will have to implement a system to support messaging;
/// - Checks for uniqueness of approvals should be done through `Channel`s to avoid big data storage;
///
/// I. Sending call approvals
///
/// A approval is sent through the `send` function, a Channel is supplied to determine the source -> ID.
/// Event is then emitted and Axelar network can operate
///
/// II. Receiving call approvals
///
/// Approval bytes and signatures are passed into `create` function to generate a CallApproval object.
///  - Signatures are checked against the known set of validators.
///  - CallApproval bytes are parsed to determine: source, destination_chain, payload and target_id
///  - `target_id` points to a `Channel` object
///
/// Once created, `CallApproval` needs to be consumed. And the only way to do it is by calling
/// `consume_call_approval` function and pass a correct `Channel` instance alongside the `CallApproval`.
///  - CallApproval is checked for uniqueness (for this channel)
///  - CallApproval is checked to match the `Channel`.id
///
module axelar::gateway {
    use std::string::{Self, String};
    use std::vector;

    use sui::bcs;

    use axelar::utils::to_sui_signed;
    use axelar::channel::{Self, Channel, ApprovedCall};
    use axelar::validators::{Self, AxelarValidators, validate_proof};

    /// For when approval signatures failed verification.
    const ESignatureInvalid: u64 = 1;

    /// For when number of commands does not match number of command ids.
    const EInvalidCommands: u64 = 4;

    /// For when approval chainId is not SUI.
    const EInvalidChain: u64 = 3;

    // These are currently supported
    const SELECTOR_APPROVE_CONTRACT_CALL: vector<u8> = b"approveContractCall";
    const SELECTOR_TRANSFER_OPERATORSHIP: vector<u8> = b"transferOperatorship";

    /// Emitted when a new message is sent from the SUI network.
    public struct ContractCall has copy, drop {
        source: vector<u8>,
        destination: vector<u8>,
        destination_address: vector<u8>,
        payload: vector<u8>,
    }

    /// The main entrypoint for the external approval processing.
    /// Parses data and attaches call approvals to the Axelar object to be
    /// later picked up and consumed by their corresponding Channel.
    ///
    /// Aborts with multiple error codes, ignores call approval which are not
    /// supported by the current implementation of the protocol.
    ///
    /// Input data must be serialized with BCS (see specification here: https://github.com/diem/bcs).
    entry fun process_commands(
        validators: &mut AxelarValidators,
        input: vector<u8>
    ) {
        let mut bytes = bcs::new(input);
        // Split input into:
        // data: vector<u8> (BCS bytes)
        // proof: vector<u8> (BCS bytes)
        let (data, proof) = (
            bcs::peel_vec_u8(&mut bytes),
            bcs::peel_vec_u8(&mut bytes)
        );

        let mut allow_operatorship_transfer = validate_proof(validators, to_sui_signed(*&data), proof);

        // Treat `data` as BCS bytes.
        let mut data_bcs = bcs::new(data);

        // Split data into:
        // chain_id: u64,
        // command_ids: vector<vector<u8>> (vector<string>)
        // commands: vector<vector<u8>> (vector<string>)
        // params: vector<vector<u8>> (vector<byteArray>)
        let (chain_id, command_ids, commands, params) = (
            bcs::peel_u64(&mut data_bcs),
            bcs::peel_vec_address(&mut data_bcs),
            bcs::peel_vec_vec_u8(&mut data_bcs),
            bcs::peel_vec_vec_u8(&mut data_bcs)
        );

        assert!(chain_id == 1, EInvalidChain);

        let (mut i, commands_len) = (0, vector::length(&commands));

        // make sure number of commands passed matches command IDs
        assert!(vector::length(&command_ids) == commands_len, EInvalidCommands);
        // make sure number of commands passed matches params
        assert!(vector::length(&params) == commands_len, EInvalidCommands);

        while (i < commands_len) {
            let msg_id = *vector::borrow(&command_ids, i);
            let cmd_selector = vector::borrow(&commands, i);
            let payload = *vector::borrow(&params, i);
            i = i + 1;

            // Build a `CallApproval` object from the `params[i]`. BCS serializes data
            // in order, so field reads have to be done carefully and in order!
            if (cmd_selector == &SELECTOR_APPROVE_CONTRACT_CALL) {
                let mut payload = bcs::new(payload);
                validators::add_approval(validators,
                    msg_id,
                    string::utf8(bcs::peel_vec_u8(&mut payload)),
                    string::utf8(bcs::peel_vec_u8(&mut payload)),
                    bcs::peel_address(&mut payload),
                    bcs::peel_vec_u8(&mut payload)
                );
                continue
            } else if (cmd_selector == &SELECTOR_TRANSFER_OPERATORSHIP) {
                if (!allow_operatorship_transfer) {
                    continue
                };
                allow_operatorship_transfer = false;
                validators::transfer_operatorship(validators, payload)
            } else {
                continue
            };
        };
    }

    /// Creates a new `ApprovedCall` object which must be delivered to the
    /// matching `Channel`.
    public fun take_approved_call(
        axelar: &mut AxelarValidators,
        cmd_id: address,
        source_chain: String,
        source_address: String,
        target_id: address,
        payload: vector<u8>
    ): ApprovedCall {
        validators::take_approved_call(
            axelar, cmd_id, source_chain, source_address, target_id, payload
        )
    }

    /// Call a contract on the destination chain by sending an event from an
    /// authorized Channel. Currently we require Channel to be mutable to prevent
    /// frozen object scenario or when someone exposes the Channel to the outer
    /// world. However, this restriction may be lifted in the future, and having
    /// an immutable reference should be enough.
    public fun call_contract<T: store>(
        channel: &mut Channel<T>,
        destination: vector<u8>,
        destination_address: vector<u8>,
        payload: vector<u8>
    ) {
        sui::event::emit(ContractCall {
            source: channel::source_id(channel),
            destination,
            destination_address,
            payload,
        })
    }

    #[test_only]
    use axelar::utils::operators_hash;
    #[test_only]
    use sui::vec_map;

    #[test_only]
    /// Test call approval for the `test_execute` test.
    /// Generated via the `presets` script.
    const CALL_APPROVAL: vector<u8> = x"ce01010000000000000002000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020213617070726f7665436f6e747261637443616c6c13617070726f7665436f6e747261637443616c6c022b034554480330783000000000000000000000000000000000000000000000000000000000000004000000002e064158454c415203307831000000000000000000000000000000000000000000000000000000000000040000000087010121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902801640000000000000000000000000000000a00000000000000000000000000000001410359561d86366875003ace8879abf953972034221461896d5098873ebe0b30ed6ef06560cc0adccedc8dd09d2a2bca7bfd22ca09d53c034a1aacfffefad0a6000000";

    #[test_only]
    const TRANSFER_OPERATORSHIP_APPROVAL: vector<u8> = x"8501010000000000000001000000000000000000000000000000000000000000000000000000000000000101147472616e736665724f70657261746f727368697001440121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902801c80000000000000000000000000000001400000000000000000000000000000087010121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902801640000000000000000000000000000000a000000000000000000000000000000014198b04944e2009969c93226ec6c97a7b9cc655b4ac52f7eeefd6cf107981c063a56a419cb149ea8a9cd49e8c745c655c5ccc242d35a9bebe7cebf6751121092a30100";

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

        let mut epoch_for_hash = vec_map::empty();
        vec_map::insert(&mut epoch_for_hash, operators_hash(&operators, &vector[100u128], 10u128), epoch);

        let mut test = ts::begin(@0x0);

        // create validators for testing
        let mut validators = validators::new(
            epoch,
            epoch_for_hash,
            ctx(&mut test)
        );

        process_commands(&mut validators, CALL_APPROVAL);

        validators::remove_approval_for_test(&mut validators, @0x1);
        validators::remove_approval_for_test(&mut validators, @0x2);
        validators::drop_for_test(validators);
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

        let mut epoch_for_hash = vec_map::empty();
        vec_map::insert(&mut epoch_for_hash, operators_hash(&operators, &vector[100u128], 10u128), epoch);

        let mut test = ts::begin(@0x0);

        // create validators for testing
        let mut validators = validators::new(
            epoch,
            epoch_for_hash,
            ctx(&mut test)
        );
        process_commands(&mut validators, TRANSFER_OPERATORSHIP_APPROVAL);
        assert!(validators::epoch(&validators) == 2, 0);

        validators::drop_for_test(validators);
        ts::end(test);
    }
}
