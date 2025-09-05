// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::message;

use bridge::chain_ids;
use bridge::message_types;
use std::ascii::{Self, String};
use sui::bcs::{Self, BCS};

const CURRENT_MESSAGE_VERSION: u8 = 1;
const ECDSA_ADDRESS_LENGTH: u64 = 20;

const ETrailingBytes: u64 = 0;
const EInvalidAddressLength: u64 = 1;
const EEmptyList: u64 = 2;
const EInvalidMessageType: u64 = 3;
const EInvalidEmergencyOpType: u64 = 4;
const EInvalidPayloadLength: u64 = 5;
const EMustBeTokenMessage: u64 = 6;

// Emergency Op types
const PAUSE: u8 = 0;
const UNPAUSE: u8 = 1;

//////////////////////////////////////////////////////
// Types
//

public struct BridgeMessage has copy, drop, store {
    message_type: u8,
    message_version: u8,
    seq_num: u64,
    source_chain: u8,
    payload: vector<u8>,
}

public struct BridgeMessageKey has copy, drop, store {
    source_chain: u8,
    message_type: u8,
    bridge_seq_num: u64,
}

public struct TokenTransferPayload has drop {
    sender_address: vector<u8>,
    target_chain: u8,
    target_address: vector<u8>,
    token_type: u8,
    amount: u64,
}

public struct EmergencyOp has drop {
    op_type: u8,
}

public struct Blocklist has drop {
    blocklist_type: u8,
    validator_eth_addresses: vector<vector<u8>>,
}

// Update the limit for route from sending_chain to receiving_chain
// This message is supposed to be processed by `chain` or the receiving chain
public struct UpdateBridgeLimit has drop {
    // The receiving chain, also the chain that checks and processes this message
    receiving_chain: u8,
    // The sending chain
    sending_chain: u8,
    limit: u64,
}

public struct UpdateAssetPrice has drop {
    token_id: u8,
    new_price: u64,
}

public struct AddTokenOnSui has drop {
    native_token: bool,
    token_ids: vector<u8>,
    token_type_names: vector<String>,
    token_prices: vector<u64>,
}

// For read
public struct ParsedTokenTransferMessage has drop {
    message_version: u8,
    seq_num: u64,
    source_chain: u8,
    payload: vector<u8>,
    parsed_payload: TokenTransferPayload,
}

//////////////////////////////////////////////////////
// Public functions
//

// Note: `bcs::peel_vec_u8` *happens* to work here because
// `sender_address` and `target_address` are no longer than 255 bytes.
// Therefore their length can be represented by a single byte.
// See `create_token_bridge_message` for the actual encoding rule.
public fun extract_token_bridge_payload(message: &BridgeMessage): TokenTransferPayload {
    let mut bcs = bcs::new(message.payload);
    let sender_address = bcs.peel_vec_u8();
    let target_chain = bcs.peel_u8();
    let target_address = bcs.peel_vec_u8();
    let token_type = bcs.peel_u8();
    let amount = peel_u64_be(&mut bcs);

    chain_ids::assert_valid_chain_id(target_chain);
    assert!(bcs.into_remainder_bytes().is_empty(), ETrailingBytes);

    TokenTransferPayload {
        sender_address,
        target_chain,
        target_address,
        token_type,
        amount,
    }
}

/// Emergency op payload is just a single byte
public fun extract_emergency_op_payload(message: &BridgeMessage): EmergencyOp {
    assert!(message.payload.length() == 1, ETrailingBytes);
    EmergencyOp { op_type: message.payload[0] }
}

public fun extract_blocklist_payload(message: &BridgeMessage): Blocklist {
    // blocklist payload should consist of one byte blocklist type, and list of 20 bytes evm addresses
    // derived from ECDSA public keys
    let mut bcs = bcs::new(message.payload);
    let blocklist_type = bcs.peel_u8();
    let mut address_count = bcs.peel_u8();

    assert!(address_count != 0, EEmptyList);

    let mut validator_eth_addresses = vector[];
    while (address_count > 0) {
        let (mut address, mut i) = (vector[], 0);
        while (i < ECDSA_ADDRESS_LENGTH) {
            address.push_back(bcs.peel_u8());
            i = i + 1;
        };
        validator_eth_addresses.push_back(address);
        address_count = address_count - 1;
    };

    assert!(bcs.into_remainder_bytes().is_empty(), ETrailingBytes);

    Blocklist {
        blocklist_type,
        validator_eth_addresses,
    }
}

public fun extract_update_bridge_limit(message: &BridgeMessage): UpdateBridgeLimit {
    let mut bcs = bcs::new(message.payload);
    let sending_chain = bcs.peel_u8();
    let limit = peel_u64_be(&mut bcs);

    chain_ids::assert_valid_chain_id(sending_chain);
    assert!(bcs.into_remainder_bytes().is_empty(), ETrailingBytes);

    UpdateBridgeLimit {
        receiving_chain: message.source_chain,
        sending_chain,
        limit,
    }
}

public fun extract_update_asset_price(message: &BridgeMessage): UpdateAssetPrice {
    let mut bcs = bcs::new(message.payload);
    let token_id = bcs.peel_u8();
    let new_price = peel_u64_be(&mut bcs);

    assert!(bcs.into_remainder_bytes().is_empty(), ETrailingBytes);

    UpdateAssetPrice {
        token_id,
        new_price,
    }
}

public fun extract_add_tokens_on_sui(message: &BridgeMessage): AddTokenOnSui {
    let mut bcs = bcs::new(message.payload);
    let native_token = bcs.peel_bool();
    let token_ids = bcs.peel_vec_u8();
    let token_type_names_bytes = bcs.peel_vec_vec_u8();
    let token_prices = bcs.peel_vec_u64();

    let mut n = 0;
    let mut token_type_names = vector[];
    while (n < token_type_names_bytes.length()) {
        token_type_names.push_back(ascii::string(*token_type_names_bytes.borrow(n)));
        n = n + 1;
    };
    assert!(bcs.into_remainder_bytes().is_empty(), ETrailingBytes);
    AddTokenOnSui {
        native_token,
        token_ids,
        token_type_names,
        token_prices,
    }
}

public fun serialize_message(message: BridgeMessage): vector<u8> {
    let BridgeMessage {
        message_type,
        message_version,
        seq_num,
        source_chain,
        payload,
    } = message;

    let mut message = vector[message_type, message_version];

    // bcs serializes u64 as 8 bytes
    message.append(reverse_bytes(bcs::to_bytes(&seq_num)));
    message.push_back(source_chain);
    message.append(payload);
    message
}

/// Token Transfer Message Format:
/// [message_type: u8]
/// [version:u8]
/// [nonce:u64]
/// [source_chain: u8]
/// [sender_address_length:u8]
/// [sender_address: byte[]]
/// [target_chain:u8]
/// [target_address_length:u8]
/// [target_address: byte[]]
/// [token_type:u8]
/// [amount:u64]
public fun create_token_bridge_message(
    source_chain: u8,
    seq_num: u64,
    sender_address: vector<u8>,
    target_chain: u8,
    target_address: vector<u8>,
    token_type: u8,
    amount: u64,
): BridgeMessage {
    chain_ids::assert_valid_chain_id(source_chain);
    chain_ids::assert_valid_chain_id(target_chain);

    let mut payload = vector[];

    // sender address should be less than 255 bytes so can fit into u8
    payload.push_back((vector::length(&sender_address) as u8));
    payload.append(sender_address);
    payload.push_back(target_chain);
    // target address should be less than 255 bytes so can fit into u8
    payload.push_back((vector::length(&target_address) as u8));
    payload.append(target_address);
    payload.push_back(token_type);
    // bcs serialzies u64 as 8 bytes
    payload.append(reverse_bytes(bcs::to_bytes(&amount)));

    assert!(vector::length(&payload) == 64, EInvalidPayloadLength);

    BridgeMessage {
        message_type: message_types::token(),
        message_version: CURRENT_MESSAGE_VERSION,
        seq_num,
        source_chain,
        payload,
    }
}

/// Emergency Op Message Format:
/// [message_type: u8]
/// [version:u8]
/// [nonce:u64]
/// [chain_id: u8]
/// [op_type: u8]
public fun create_emergency_op_message(source_chain: u8, seq_num: u64, op_type: u8): BridgeMessage {
    chain_ids::assert_valid_chain_id(source_chain);

    BridgeMessage {
        message_type: message_types::emergency_op(),
        message_version: CURRENT_MESSAGE_VERSION,
        seq_num,
        source_chain,
        payload: vector[op_type],
    }
}

/// Blocklist Message Format:
/// [message_type: u8]
/// [version:u8]
/// [nonce:u64]
/// [chain_id: u8]
/// [blocklist_type: u8]
/// [validator_length: u8]
/// [validator_ecdsa_addresses: byte[][]]
public fun create_blocklist_message(
    source_chain: u8,
    seq_num: u64,
    // 0: block, 1: unblock
    blocklist_type: u8,
    validator_ecdsa_addresses: vector<vector<u8>>,
): BridgeMessage {
    chain_ids::assert_valid_chain_id(source_chain);

    let address_length = validator_ecdsa_addresses.length();
    let mut payload = vector[blocklist_type, (address_length as u8)];
    let mut i = 0;

    while (i < address_length) {
        let address = validator_ecdsa_addresses[i];
        assert!(address.length() == ECDSA_ADDRESS_LENGTH, EInvalidAddressLength);
        payload.append(address);

        i = i + 1;
    };

    BridgeMessage {
        message_type: message_types::committee_blocklist(),
        message_version: CURRENT_MESSAGE_VERSION,
        seq_num,
        source_chain,
        payload,
    }
}

/// Update bridge limit Message Format:
/// [message_type: u8]
/// [version:u8]
/// [nonce:u64]
/// [receiving_chain_id: u8]
/// [sending_chain_id: u8]
/// [new_limit: u64]
public fun create_update_bridge_limit_message(
    receiving_chain: u8,
    seq_num: u64,
    sending_chain: u8,
    new_limit: u64,
): BridgeMessage {
    chain_ids::assert_valid_chain_id(receiving_chain);
    chain_ids::assert_valid_chain_id(sending_chain);

    let mut payload = vector[sending_chain];
    payload.append(reverse_bytes(bcs::to_bytes(&new_limit)));

    BridgeMessage {
        message_type: message_types::update_bridge_limit(),
        message_version: CURRENT_MESSAGE_VERSION,
        seq_num,
        source_chain: receiving_chain,
        payload,
    }
}

/// Update asset price message
/// [message_type: u8]
/// [version:u8]
/// [nonce:u64]
/// [chain_id: u8]
/// [token_id: u8]
/// [new_price:u64]
public fun create_update_asset_price_message(
    token_id: u8,
    source_chain: u8,
    seq_num: u64,
    new_price: u64,
): BridgeMessage {
    chain_ids::assert_valid_chain_id(source_chain);

    let mut payload = vector[token_id];
    payload.append(reverse_bytes(bcs::to_bytes(&new_price)));
    BridgeMessage {
        message_type: message_types::update_asset_price(),
        message_version: CURRENT_MESSAGE_VERSION,
        seq_num,
        source_chain,
        payload,
    }
}

/// Update Sui token message
/// [message_type:u8]
/// [version:u8]
/// [nonce:u64]
/// [chain_id: u8]
/// [native_token:bool]
/// [token_ids:vector<u8>]
/// [token_type_name:vector<String>]
/// [token_prices:vector<u64>]
public fun create_add_tokens_on_sui_message(
    source_chain: u8,
    seq_num: u64,
    native_token: bool,
    token_ids: vector<u8>,
    type_names: vector<String>,
    token_prices: vector<u64>,
): BridgeMessage {
    chain_ids::assert_valid_chain_id(source_chain);
    let mut payload = bcs::to_bytes(&native_token);
    payload.append(bcs::to_bytes(&token_ids));
    payload.append(bcs::to_bytes(&type_names));
    payload.append(bcs::to_bytes(&token_prices));
    BridgeMessage {
        message_type: message_types::add_tokens_on_sui(),
        message_version: CURRENT_MESSAGE_VERSION,
        seq_num,
        source_chain,
        payload,
    }
}

public fun create_key(source_chain: u8, message_type: u8, bridge_seq_num: u64): BridgeMessageKey {
    BridgeMessageKey { source_chain, message_type, bridge_seq_num }
}

public fun key(self: &BridgeMessage): BridgeMessageKey {
    create_key(self.source_chain, self.message_type, self.seq_num)
}

// BridgeMessage getters
public fun message_version(self: &BridgeMessage): u8 {
    self.message_version
}

public fun message_type(self: &BridgeMessage): u8 {
    self.message_type
}

public fun seq_num(self: &BridgeMessage): u64 {
    self.seq_num
}

public fun source_chain(self: &BridgeMessage): u8 {
    self.source_chain
}

public fun payload(self: &BridgeMessage): vector<u8> {
    self.payload
}

public fun token_target_chain(self: &TokenTransferPayload): u8 {
    self.target_chain
}

public fun token_target_address(self: &TokenTransferPayload): vector<u8> {
    self.target_address
}

public fun token_type(self: &TokenTransferPayload): u8 {
    self.token_type
}

public fun token_amount(self: &TokenTransferPayload): u64 {
    self.amount
}

// EmergencyOpPayload getters
public fun emergency_op_type(self: &EmergencyOp): u8 {
    self.op_type
}

public fun blocklist_type(self: &Blocklist): u8 {
    self.blocklist_type
}

public fun blocklist_validator_addresses(self: &Blocklist): &vector<vector<u8>> {
    &self.validator_eth_addresses
}

public fun update_bridge_limit_payload_sending_chain(self: &UpdateBridgeLimit): u8 {
    self.sending_chain
}

public fun update_bridge_limit_payload_receiving_chain(self: &UpdateBridgeLimit): u8 {
    self.receiving_chain
}

public fun update_bridge_limit_payload_limit(self: &UpdateBridgeLimit): u64 {
    self.limit
}

public fun update_asset_price_payload_token_id(self: &UpdateAssetPrice): u8 {
    self.token_id
}

public fun update_asset_price_payload_new_price(self: &UpdateAssetPrice): u64 {
    self.new_price
}

public fun is_native(self: &AddTokenOnSui): bool {
    self.native_token
}

public fun token_ids(self: &AddTokenOnSui): vector<u8> {
    self.token_ids
}

public fun token_type_names(self: &AddTokenOnSui): vector<String> {
    self.token_type_names
}

public fun token_prices(self: &AddTokenOnSui): vector<u64> {
    self.token_prices
}

public fun emergency_op_pause(): u8 {
    PAUSE
}

public fun emergency_op_unpause(): u8 {
    UNPAUSE
}

/// Return the required signature threshold for the message, values are voting power in the scale of 10000
public fun required_voting_power(self: &BridgeMessage): u64 {
    let message_type = message_type(self);

    if (message_type == message_types::token()) {
        3334
    } else if (message_type == message_types::emergency_op()) {
        let payload = extract_emergency_op_payload(self);
        if (payload.op_type == PAUSE) {
            450
        } else if (payload.op_type == UNPAUSE) {
            5001
        } else {
            abort EInvalidEmergencyOpType
        }
    } else if (message_type == message_types::committee_blocklist()) {
        5001
    } else if (message_type == message_types::update_asset_price()) {
        5001
    } else if (message_type == message_types::update_bridge_limit()) {
        5001
    } else if (message_type == message_types::add_tokens_on_sui()) {
        5001
    } else {
        abort EInvalidMessageType
    }
}

// Convert BridgeMessage to ParsedTokenTransferMessage
public fun to_parsed_token_transfer_message(message: &BridgeMessage): ParsedTokenTransferMessage {
    assert!(message.message_type() == message_types::token(), EMustBeTokenMessage);
    let payload = message.extract_token_bridge_payload();
    ParsedTokenTransferMessage {
        message_version: message.message_version(),
        seq_num: message.seq_num(),
        source_chain: message.source_chain(),
        payload: message.payload(),
        parsed_payload: payload,
    }
}

//////////////////////////////////////////////////////
// Internal functions
//

fun reverse_bytes(mut bytes: vector<u8>): vector<u8> {
    vector::reverse(&mut bytes);
    bytes
}

fun peel_u64_be(bcs: &mut BCS): u64 {
    let (mut value, mut i) = (0u64, 64u8);
    while (i > 0) {
        i = i - 8;
        let byte = (bcs::peel_u8(bcs) as u64);
        value = value + (byte << i);
    };
    value
}

//////////////////////////////////////////////////////
// Test functions
//

#[test_only]
public(package) fun peel_u64_be_for_testing(bcs: &mut BCS): u64 {
    peel_u64_be(bcs)
}

#[test_only]
public(package) fun make_generic_message(
    message_type: u8,
    message_version: u8,
    seq_num: u64,
    source_chain: u8,
    payload: vector<u8>,
): BridgeMessage {
    BridgeMessage {
        message_type,
        message_version,
        seq_num,
        source_chain,
        payload,
    }
}

#[test_only]
public(package) fun make_payload(
    sender_address: vector<u8>,
    target_chain: u8,
    target_address: vector<u8>,
    token_type: u8,
    amount: u64,
): TokenTransferPayload {
    TokenTransferPayload {
        sender_address,
        target_chain,
        target_address,
        token_type,
        amount,
    }
}

#[test_only]
public(package) fun deserialize_message_test_only(message: vector<u8>): BridgeMessage {
    let mut bcs = bcs::new(message);
    let message_type = bcs::peel_u8(&mut bcs);
    let message_version = bcs::peel_u8(&mut bcs);
    let seq_num = peel_u64_be_for_testing(&mut bcs);
    let source_chain = bcs::peel_u8(&mut bcs);
    let payload = bcs::into_remainder_bytes(bcs);
    make_generic_message(
        message_type,
        message_version,
        seq_num,
        source_chain,
        payload,
    )
}

#[test_only]
public(package) fun reverse_bytes_test(bytes: vector<u8>): vector<u8> {
    reverse_bytes(bytes)
}

#[test_only]
public(package) fun set_payload(message: &mut BridgeMessage, bytes: vector<u8>) {
    message.payload = bytes;
}

#[test_only]
public(package) fun make_add_token_on_sui(
    native_token: bool,
    token_ids: vector<u8>,
    token_type_names: vector<String>,
    token_prices: vector<u64>,
): AddTokenOnSui {
    AddTokenOnSui {
        native_token,
        token_ids,
        token_type_names,
        token_prices,
    }
}

#[test_only]
public(package) fun unpack_message(msg: BridgeMessageKey): (u8, u8, u64) {
    (msg.source_chain, msg.message_type, msg.bridge_seq_num)
}
