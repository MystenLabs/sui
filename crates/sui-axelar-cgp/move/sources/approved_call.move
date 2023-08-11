// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module axelar::approved_call {
    use std::string::String;

    friend axelar::validators;

    struct ApprovedCall {
        /// ID of the call approval, guaranteed to be unique by Axelar.
        cmd_id: address,
        /// The target Channel's UID.
        target_id: address,
        /// Name of the chain where this approval came from.
        source_chain: String,
        /// Address of the source chain (vector used for compatibility).
        /// UTF8 / ASCII encoded string (for 0x0... eth address gonna be 42 bytes with 0x)
        source_address: String,
        /// Hash of the full payload (including source_* fields).
        payload_hash: vector<u8>,
        /// Payload of the command.
        payload: vector<u8>,
    }

    public(friend) fun create(
        cmd_id: address,
        source_chain: String,
        source_address: String,
        target_id: address,
        payload_hash: vector<u8>,
        payload: vector<u8>,
    ): ApprovedCall {
        ApprovedCall {
            cmd_id,
            source_chain,
            source_address,
            target_id,
            payload_hash,
            payload
        }
    }

    public fun cmd_id(msg: &ApprovedCall): address {
        msg.cmd_id
    }

    public fun target_id(msg: &ApprovedCall): address {
        msg.target_id
    }

    public fun source_chain(msg: &ApprovedCall): String {
        msg.source_chain
    }

    public fun source_address(msg: &ApprovedCall): String {
        msg.source_address
    }

    public fun payload_hash(msg: &ApprovedCall): vector<u8> {
        msg.payload_hash
    }

    public fun payload(msg: &ApprovedCall): vector<u8> {
        msg.payload
    }

    #[test_only]
    /// Handy method for burning `vector<CallApproval>` returned by the `execute` function.
    public fun delete_for_test(approved_call: ApprovedCall) {
        let ApprovedCall {
            cmd_id: _,
            target_id: _,
            source_chain: _,
            source_address: _,
            payload_hash: _,
            payload: _,
        } = approved_call;
    }
}
