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
    use std::vector;
    use sui::bcs;

    use axelar::validators;
    use axelar::validators::{AxelarValidators, validate_proof};
    use axelar::messaging;
    use axelar::messaging::CallApproval;
    use axelar::utils::to_sui_signed;

    /// For when approval signatures failed verification.
    const ESignatureInvalid: u64 = 1;

    /// For when number of commands does not match number of command ids.
    const EInvalidCommands: u64 = 4;

    /// For when approval chainId is not SUI.
    const EInvalidChain: u64 = 3;

    // These are currently supported
    const SELECTOR_APPROVE_CONTRACT_CALL: vector<u8> = b"approveContractCall";
    const SELECTOR_TRANSFER_OPERATORSHIP: vector<u8> = b"transferOperatorship";

    /// The main entrypoint for the external approval processing.
    /// Parses data and attaches call approvals to the Axelar object to be
    /// later picked up and consumed by their corresponding Channel.
    entry fun process_commands(
        axelar: &mut AxelarValidators,
        input: vector<u8>
    ) {
        let call_approvals = validate_commands(axelar, input);
        let (i, len) = (0, vector::length(&call_approvals));

        while (i < len) {
            validators::add_call_approval(axelar, vector::pop_back(&mut call_approvals));
            i = i + 1;
        };
        vector::destroy_empty(call_approvals);
    }

    /// Processes the data and the signatures generating a vector of
    /// `CallApproval` objects.
    ///
    /// Aborts with multiple error codes, ignores call approval which are not
    /// supported by the current implementation of the protocol.
    ///
    /// Input data must be serialized with BCS (see specification here: https://github.com/diem/bcs).
    fun validate_commands(validators: &mut AxelarValidators, input: vector<u8>): vector<CallApproval> {
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

        let (i, commands_len, approvals) = (0, vector::length(&commands), vector::empty());

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
                let payload = bcs::new(payload);
                vector::push_back(&mut approvals, messaging::create(
                    msg_id,
                    bcs::peel_vec_u8(&mut payload),
                    bcs::peel_vec_u8(&mut payload),
                    bcs::peel_address(&mut payload),
                    bcs::peel_vec_u8(&mut payload)
                ));
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
        approvals
    }

    #[test_only] use axelar::utils::operators_hash;
    #[test_only] use sui::vec_map;

    #[test_only]
    /// Test call approval for the `test_execute` test.
    /// Generated via the `presets` script.
    const CALL_APPROVAL: vector<u8> = x"af0101000000000000000209726f6775655f6f6e650a6178656c61725f74776f0213617070726f7665436f6e747261637443616c6c13617070726f7665436f6e747261637443616c6c02310345544803307830000000000000000000000000000000000000000000000000000000000000040000000005000000000034064158454c415203307831000000000000000000000000000000000000000000000000000000000000040000000005000000000087010121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902801640000000000000000000000000000000a0000000000000000000000000000000141dcfc40d95cc89a9c8a0973c3dae95806c5daa5aefe072caafd5541844d62fabf2dc580a8663df7adb846f1ef7d553a13174399e4c4cb55c42bdf7fa8f02c8fa10000";

    #[test_only]
    const TRANSFER_OPERATORSHIP_APPROVAL: vector<u8> = x"6f01000000000000000109726f6775655f6f6e6501147472616e736665724f70657261746f727368697001440121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902801c80000000000000000000000000000001400000000000000000000000000000087010121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902801640000000000000000000000000000000a00000000000000000000000000000001414b88c29db7550c18fac63470891ddd8460e7d44d8d27bf1528758de03515c2a4327b07bc582732b80b7aa8d15964a4878ce203430661ce3d096afaea791189860000";

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
        let axelar = validators::new(
            epoch,
            epoch_for_hash,
            ctx(&mut test)
        );

        let call_approvals = validate_commands(&mut axelar, CALL_APPROVAL);
        validators::delete(axelar);
        messaging::delete(call_approvals);
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
        let axelar = validators::new(
            epoch,
            epoch_for_hash,
            ctx(&mut test)
        );

        let call_approvals = validate_commands(&mut axelar, TRANSFER_OPERATORSHIP_APPROVAL);

        assert!(validators::epoch(&axelar) == 2, 0);

        validators::delete(axelar);
        messaging::delete(call_approvals);
        ts::end(test);
    }
}
