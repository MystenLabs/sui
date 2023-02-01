// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const { BCS, getSuiMoveConfig } = require("@mysten/bcs");
const bcs = new BCS(getSuiMoveConfig());

module.exports = bcs
    .registerStructType("Input", {
        data: "vector<u8>",
        proof: "vector<u8>",
    })
    .registerStructType("Proof", {
        // operators is a 33 byte / for now at least
        operators: "vector<vector<u8>>",
        weights: "vector<u64>",
        threshold: "u64",
        signatures: "vector<vector<u8>>",
    })
    // internals of the message
    .registerStructType("AxelarMessage", {
        chain_id: "u64",
        command_ids: "vector<string>",
        commands: "vector<string>",
        params: "vector<vector<u8>>",
    })
    // defines channel target
    .registerStructType("GenericMessage", {
        source_chain: "string",
        source_address: "string",
        target_id: "address",
        payload_hash: "vector<u8>",
        payload: "vector<u8>",
  });
