// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::bridge;

use bridge::chain_ids;
use bridge::committee::{Self, BridgeCommittee};
use bridge::limiter::{Self, TransferLimiter};
use bridge::message::{
    Self,
    BridgeMessage,
    BridgeMessageKey,
    EmergencyOp,
    UpdateAssetPrice,
    UpdateBridgeLimit,
    AddTokenOnSui,
    ParsedTokenTransferMessage,
    to_parsed_token_transfer_message
};
use bridge::message_types;
use bridge::treasury::{Self, BridgeTreasury};
use sui::address;
use sui::clock::Clock;
use sui::coin::{Coin, TreasuryCap, CoinMetadata};
use sui::event;
use sui::linked_table::{Self, LinkedTable};
use sui::package::UpgradeCap;
use sui::vec_map::{Self, VecMap};
use sui::versioned::{Self, Versioned};
use sui_system::sui_system::SuiSystemState;

const MESSAGE_VERSION: u8 = 1;

// Transfer Status
const TRANSFER_STATUS_PENDING: u8 = 0;
const TRANSFER_STATUS_APPROVED: u8 = 1;
const TRANSFER_STATUS_CLAIMED: u8 = 2;
const TRANSFER_STATUS_NOT_FOUND: u8 = 3;

const EVM_ADDRESS_LENGTH: u64 = 20;

//////////////////////////////////////////////////////
// Types
//

public struct Bridge has key {
    id: UID,
    inner: Versioned,
}

public struct BridgeInner has store {
    bridge_version: u64,
    message_version: u8,
    chain_id: u8,
    // nonce for replay protection
    // key: message type, value: next sequence number
    sequence_nums: VecMap<u8, u64>,
    // committee
    committee: BridgeCommittee,
    // Bridge treasury for mint/burn bridged tokens
    treasury: BridgeTreasury,
    token_transfer_records: LinkedTable<BridgeMessageKey, BridgeRecord>,
    limiter: TransferLimiter,
    paused: bool,
}

public struct TokenDepositedEvent has copy, drop {
    seq_num: u64,
    source_chain: u8,
    sender_address: vector<u8>,
    target_chain: u8,
    target_address: vector<u8>,
    token_type: u8,
    amount: u64,
}

public struct EmergencyOpEvent has copy, drop {
    frozen: bool,
}

public struct BridgeRecord has drop, store {
    message: BridgeMessage,
    verified_signatures: Option<vector<vector<u8>>>,
    claimed: bool,
}

const EUnexpectedMessageType: u64 = 0;
const EUnauthorisedClaim: u64 = 1;
const EMalformedMessageError: u64 = 2;
const EUnexpectedTokenType: u64 = 3;
const EUnexpectedChainID: u64 = 4;
const ENotSystemAddress: u64 = 5;
const EUnexpectedSeqNum: u64 = 6;
const EWrongInnerVersion: u64 = 7;
const EBridgeUnavailable: u64 = 8;
const EUnexpectedOperation: u64 = 9;
const EInvariantSuiInitializedTokenTransferShouldNotBeClaimed: u64 = 10;
const EMessageNotFoundInRecords: u64 = 11;
const EUnexpectedMessageVersion: u64 = 12;
const EBridgeAlreadyPaused: u64 = 13;
const EBridgeNotPaused: u64 = 14;
const ETokenAlreadyClaimedOrHitLimit: u64 = 15;
const EInvalidBridgeRoute: u64 = 16;
const EMustBeTokenMessage: u64 = 17;
const EInvalidEvmAddress: u64 = 18;
const ETokenValueIsZero: u64 = 19;

const CURRENT_VERSION: u64 = 1;

public struct TokenTransferApproved has copy, drop {
    message_key: BridgeMessageKey,
}

public struct TokenTransferClaimed has copy, drop {
    message_key: BridgeMessageKey,
}

public struct TokenTransferAlreadyApproved has copy, drop {
    message_key: BridgeMessageKey,
}

public struct TokenTransferAlreadyClaimed has copy, drop {
    message_key: BridgeMessageKey,
}

public struct TokenTransferLimitExceed has copy, drop {
    message_key: BridgeMessageKey,
}

//////////////////////////////////////////////////////
// Internal initialization functions
//

// this method is called once in end of epoch tx to create the bridge
#[allow(unused_function)]
fun create(id: UID, chain_id: u8, ctx: &mut TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    let bridge_inner = BridgeInner {
        bridge_version: CURRENT_VERSION,
        message_version: MESSAGE_VERSION,
        chain_id,
        sequence_nums: vec_map::empty(),
        committee: committee::create(ctx),
        treasury: treasury::create(ctx),
        token_transfer_records: linked_table::new(ctx),
        limiter: limiter::new(),
        paused: false,
    };
    let bridge = Bridge {
        id,
        inner: versioned::create(CURRENT_VERSION, bridge_inner, ctx),
    };
    transfer::share_object(bridge);
}

#[allow(unused_function)]
fun init_bridge_committee(
    bridge: &mut Bridge,
    active_validator_voting_power: VecMap<address, u64>,
    min_stake_participation_percentage: u64,
    ctx: &TxContext,
) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    let inner = load_inner_mut(bridge);
    if (inner.committee.committee_members().is_empty()) {
        inner
            .committee
            .try_create_next_committee(
                active_validator_voting_power,
                min_stake_participation_percentage,
                ctx,
            )
    }
}

//////////////////////////////////////////////////////
// Public functions
//

public fun committee_registration(
    bridge: &mut Bridge,
    system_state: &mut SuiSystemState,
    bridge_pubkey_bytes: vector<u8>,
    http_rest_url: vector<u8>,
    ctx: &TxContext,
) {
    load_inner_mut(bridge)
        .committee
        .register(system_state, bridge_pubkey_bytes, http_rest_url, ctx);
}

public fun update_node_url(bridge: &mut Bridge, new_url: vector<u8>, ctx: &TxContext) {
    load_inner_mut(bridge).committee.update_node_url(new_url, ctx);
}

public fun register_foreign_token<T>(
    bridge: &mut Bridge,
    tc: TreasuryCap<T>,
    uc: UpgradeCap,
    metadata: &CoinMetadata<T>,
) {
    load_inner_mut(bridge).treasury.register_foreign_token<T>(tc, uc, metadata)
}

// Create bridge request to send token to other chain, the request will be in
// pending state until approved
public fun send_token<T>(
    bridge: &mut Bridge,
    target_chain: u8,
    target_address: vector<u8>,
    token: Coin<T>,
    ctx: &mut TxContext,
) {
    let inner = load_inner_mut(bridge);
    assert!(!inner.paused, EBridgeUnavailable);
    assert!(chain_ids::is_valid_route(inner.chain_id, target_chain), EInvalidBridgeRoute);
    assert!(target_address.length() == EVM_ADDRESS_LENGTH, EInvalidEvmAddress);

    let bridge_seq_num = inner.get_current_seq_num_and_increment(message_types::token());
    let token_id = inner.treasury.token_id<T>();
    let token_amount = token.balance().value();
    assert!(token_amount > 0, ETokenValueIsZero);

    // create bridge message
    let message = message::create_token_bridge_message(
        inner.chain_id,
        bridge_seq_num,
        address::to_bytes(ctx.sender()),
        target_chain,
        target_address,
        token_id,
        token_amount,
    );

    // burn / escrow token, unsupported coins will fail in this step
    inner.treasury.burn(token);

    // Store pending bridge request
    inner
        .token_transfer_records
        .push_back(
            message.key(),
            BridgeRecord {
                message,
                verified_signatures: option::none(),
                claimed: false,
            },
        );

    // emit event
    event::emit(TokenDepositedEvent {
        seq_num: bridge_seq_num,
        source_chain: inner.chain_id,
        sender_address: address::to_bytes(ctx.sender()),
        target_chain,
        target_address,
        token_type: token_id,
        amount: token_amount,
    });
}

// Record bridge message approvals in Sui, called by the bridge client
// If already approved, return early instead of aborting.
public fun approve_token_transfer(
    bridge: &mut Bridge,
    message: BridgeMessage,
    signatures: vector<vector<u8>>,
) {
    let inner = load_inner_mut(bridge);
    assert!(!inner.paused, EBridgeUnavailable);
    // verify signatures
    inner.committee.verify_signatures(message, signatures);

    assert!(message.message_type() == message_types::token(), EMustBeTokenMessage);
    assert!(message.message_version() == MESSAGE_VERSION, EUnexpectedMessageVersion);
    let token_payload = message.extract_token_bridge_payload();
    let target_chain = token_payload.token_target_chain();
    assert!(
        message.source_chain() == inner.chain_id || target_chain == inner.chain_id,
        EUnexpectedChainID,
    );

    let message_key = message.key();
    // retrieve pending message if source chain is Sui, the initial message
    // must exist on chain
    if (message.source_chain() == inner.chain_id) {
        let record = &mut inner.token_transfer_records[message_key];

        assert!(record.message == message, EMalformedMessageError);
        assert!(!record.claimed, EInvariantSuiInitializedTokenTransferShouldNotBeClaimed);

        // If record already has verified signatures, it means the message has been approved
        // Then we exit early.
        if (record.verified_signatures.is_some()) {
            event::emit(TokenTransferAlreadyApproved { message_key });
            return
        };
        // Store approval
        record.verified_signatures = option::some(signatures)
    } else {
        // At this point, if this message is in token_transfer_records, we know
        // it's already approved because we only add a message to token_transfer_records
        // after verifying the signatures
        if (inner.token_transfer_records.contains(message_key)) {
            event::emit(TokenTransferAlreadyApproved { message_key });
            return
        };
        // Store message and approval
        inner
            .token_transfer_records
            .push_back(
                message_key,
                BridgeRecord {
                    message,
                    verified_signatures: option::some(signatures),
                    claimed: false,
                },
            );
    };

    event::emit(TokenTransferApproved { message_key });
}

// This function can only be called by the token recipient
// Abort if the token has already been claimed or hits limiter currently,
// in which case, no event will be emitted and only abort code will be returned.
public fun claim_token<T>(
    bridge: &mut Bridge,
    clock: &Clock,
    source_chain: u8,
    bridge_seq_num: u64,
    ctx: &mut TxContext,
): Coin<T> {
    let (maybe_token, owner) = bridge.claim_token_internal<T>(
        clock,
        source_chain,
        bridge_seq_num,
        ctx,
    );
    // Only token owner can claim the token
    assert!(ctx.sender() == owner, EUnauthorisedClaim);
    assert!(maybe_token.is_some(), ETokenAlreadyClaimedOrHitLimit);
    maybe_token.destroy_some()
}

// This function can be called by anyone to claim and transfer the token to the recipient
// If the token has already been claimed or hits limiter currently, it will return instead of aborting.
public fun claim_and_transfer_token<T>(
    bridge: &mut Bridge,
    clock: &Clock,
    source_chain: u8,
    bridge_seq_num: u64,
    ctx: &mut TxContext,
) {
    let (token, owner) = bridge.claim_token_internal<T>(clock, source_chain, bridge_seq_num, ctx);
    if (token.is_some()) {
        transfer::public_transfer(token.destroy_some(), owner)
    } else {
        token.destroy_none();
    };
}

public fun execute_system_message(
    bridge: &mut Bridge,
    message: BridgeMessage,
    signatures: vector<vector<u8>>,
) {
    let message_type = message.message_type();

    // TODO: test version mismatch
    assert!(message.message_version() == MESSAGE_VERSION, EUnexpectedMessageVersion);
    let inner = load_inner_mut(bridge);

    assert!(message.source_chain() == inner.chain_id, EUnexpectedChainID);

    // check system ops seq number and increment it
    let expected_seq_num = inner.get_current_seq_num_and_increment(message_type);
    assert!(message.seq_num() == expected_seq_num, EUnexpectedSeqNum);

    inner.committee.verify_signatures(message, signatures);

    if (message_type == message_types::emergency_op()) {
        let payload = message.extract_emergency_op_payload();
        inner.execute_emergency_op(payload);
    } else if (message_type == message_types::committee_blocklist()) {
        let payload = message.extract_blocklist_payload();
        inner.committee.execute_blocklist(payload);
    } else if (message_type == message_types::update_bridge_limit()) {
        let payload = message.extract_update_bridge_limit();
        inner.execute_update_bridge_limit(payload);
    } else if (message_type == message_types::update_asset_price()) {
        let payload = message.extract_update_asset_price();
        inner.execute_update_asset_price(payload);
    } else if (message_type == message_types::add_tokens_on_sui()) {
        let payload = message.extract_add_tokens_on_sui();
        inner.execute_add_tokens_on_sui(payload);
    } else {
        abort EUnexpectedMessageType
    };
}

//////////////////////////////////////////////////////
// DevInspect Functions for Read
//

#[allow(unused_function)]
fun get_token_transfer_action_status(bridge: &Bridge, source_chain: u8, bridge_seq_num: u64): u8 {
    let inner = load_inner(bridge);
    let key = message::create_key(
        source_chain,
        message_types::token(),
        bridge_seq_num,
    );

    if (!inner.token_transfer_records.contains(key)) {
        return TRANSFER_STATUS_NOT_FOUND
    };

    let record = &inner.token_transfer_records[key];
    if (record.claimed) {
        return TRANSFER_STATUS_CLAIMED
    };

    if (record.verified_signatures.is_some()) {
        return TRANSFER_STATUS_APPROVED
    };

    TRANSFER_STATUS_PENDING
}

#[allow(unused_function)]
fun get_token_transfer_action_signatures(
    bridge: &Bridge,
    source_chain: u8,
    bridge_seq_num: u64,
): Option<vector<vector<u8>>> {
    let inner = load_inner(bridge);
    let key = message::create_key(
        source_chain,
        message_types::token(),
        bridge_seq_num,
    );

    if (!inner.token_transfer_records.contains(key)) {
        return option::none()
    };

    let record = &inner.token_transfer_records[key];
    record.verified_signatures
}

//////////////////////////////////////////////////////
// Internal functions
//

fun load_inner(bridge: &Bridge): &BridgeInner {
    let version = bridge.inner.version();

    // TODO: Replace this with a lazy update function when we add a new version of the inner object.
    assert!(version == CURRENT_VERSION, EWrongInnerVersion);
    let inner: &BridgeInner = bridge.inner.load_value();
    assert!(inner.bridge_version == version, EWrongInnerVersion);
    inner
}

fun load_inner_mut(bridge: &mut Bridge): &mut BridgeInner {
    let version = bridge.inner.version();
    // TODO: Replace this with a lazy update function when we add a new version of the inner object.
    assert!(version == CURRENT_VERSION, EWrongInnerVersion);
    let inner: &mut BridgeInner = bridge.inner.load_value_mut();
    assert!(inner.bridge_version == version, EWrongInnerVersion);
    inner
}

// Claim token from approved bridge message
// Returns Some(Coin) if coin can be claimed. If already claimed, return None
fun claim_token_internal<T>(
    bridge: &mut Bridge,
    clock: &Clock,
    source_chain: u8,
    bridge_seq_num: u64,
    ctx: &mut TxContext,
): (Option<Coin<T>>, address) {
    let inner = load_inner_mut(bridge);
    assert!(!inner.paused, EBridgeUnavailable);

    let key = message::create_key(source_chain, message_types::token(), bridge_seq_num);
    assert!(inner.token_transfer_records.contains(key), EMessageNotFoundInRecords);

    // retrieve approved bridge message
    let record = &mut inner.token_transfer_records[key];
    // ensure this is a token bridge message
    assert!(&record.message.message_type() == message_types::token(), EUnexpectedMessageType);
    // Ensure it's signed
    assert!(record.verified_signatures.is_some(), EUnauthorisedClaim);

    // extract token message
    let token_payload = record.message.extract_token_bridge_payload();
    // get owner address
    let owner = address::from_bytes(token_payload.token_target_address());

    // If already claimed, exit early
    if (record.claimed) {
        event::emit(TokenTransferAlreadyClaimed { message_key: key });
        return (option::none(), owner)
    };

    let target_chain = token_payload.token_target_chain();
    // ensure target chain matches bridge.chain_id
    assert!(target_chain == inner.chain_id, EUnexpectedChainID);

    // TODO: why do we check validity of the route here? what if inconsistency?
    // Ensure route is valid
    // TODO: add unit tests
    // `get_route` abort if route is invalid
    let route = chain_ids::get_route(source_chain, target_chain);
    // check token type
    assert!(
        treasury::token_id<T>(&inner.treasury) == token_payload.token_type(),
        EUnexpectedTokenType,
    );

    let amount = token_payload.token_amount();
    // Make sure transfer is within limit.
    if (
        !inner
            .limiter
            .check_and_record_sending_transfer<T>(
                &inner.treasury,
                clock,
                route,
                amount,
            )
    ) {
        event::emit(TokenTransferLimitExceed { message_key: key });
        return (option::none(), owner)
    };

    // claim from treasury
    let token = inner.treasury.mint<T>(amount, ctx);

    // Record changes
    record.claimed = true;
    event::emit(TokenTransferClaimed { message_key: key });

    (option::some(token), owner)
}

fun execute_emergency_op(inner: &mut BridgeInner, payload: EmergencyOp) {
    let op = payload.emergency_op_type();
    if (op == message::emergency_op_pause()) {
        assert!(!inner.paused, EBridgeAlreadyPaused);
        inner.paused = true;
        event::emit(EmergencyOpEvent { frozen: true });
    } else if (op == message::emergency_op_unpause()) {
        assert!(inner.paused, EBridgeNotPaused);
        inner.paused = false;
        event::emit(EmergencyOpEvent { frozen: false });
    } else {
        abort EUnexpectedOperation
    };
}

fun execute_update_bridge_limit(inner: &mut BridgeInner, payload: UpdateBridgeLimit) {
    let receiving_chain = payload.update_bridge_limit_payload_receiving_chain();
    assert!(receiving_chain == inner.chain_id, EUnexpectedChainID);
    let route = chain_ids::get_route(
        payload.update_bridge_limit_payload_sending_chain(),
        receiving_chain,
    );

    inner
        .limiter
        .update_route_limit(
            &route,
            payload.update_bridge_limit_payload_limit(),
        )
}

fun execute_update_asset_price(inner: &mut BridgeInner, payload: UpdateAssetPrice) {
    inner
        .treasury
        .update_asset_notional_price(
            payload.update_asset_price_payload_token_id(),
            payload.update_asset_price_payload_new_price(),
        )
}

fun execute_add_tokens_on_sui(inner: &mut BridgeInner, payload: AddTokenOnSui) {
    // FIXME: assert native_token to be false and add test
    let native_token = payload.is_native();
    let mut token_ids = payload.token_ids();
    let mut token_type_names = payload.token_type_names();
    let mut token_prices = payload.token_prices();

    // Make sure token data is consistent
    assert!(token_ids.length() == token_type_names.length(), EMalformedMessageError);
    assert!(token_ids.length() == token_prices.length(), EMalformedMessageError);

    while (token_ids.length() > 0) {
        let token_id = token_ids.pop_back();
        let token_type_name = token_type_names.pop_back();
        let token_price = token_prices.pop_back();
        inner.treasury.add_new_token(token_type_name, token_id, native_token, token_price)
    }
}

// Verify seq number matches the next expected seq number for the message type,
// and increment it.
fun get_current_seq_num_and_increment(bridge: &mut BridgeInner, msg_type: u8): u64 {
    if (!bridge.sequence_nums.contains(&msg_type)) {
        bridge.sequence_nums.insert(msg_type, 1);
        return 0
    };

    let entry = &mut bridge.sequence_nums[&msg_type];
    let seq_num = *entry;
    *entry = seq_num + 1;
    seq_num
}

#[allow(unused_function)]
fun get_parsed_token_transfer_message(
    bridge: &Bridge,
    source_chain: u8,
    bridge_seq_num: u64,
): Option<ParsedTokenTransferMessage> {
    let inner = load_inner(bridge);
    let key = message::create_key(
        source_chain,
        message_types::token(),
        bridge_seq_num,
    );

    if (!inner.token_transfer_records.contains(key)) {
        return option::none()
    };

    let record = &inner.token_transfer_records[key];
    let message = &record.message;
    option::some(to_parsed_token_transfer_message(message))
}

//////////////////////////////////////////////////////
// Test functions
//

#[test_only]
public fun create_bridge_for_testing(id: UID, chain_id: u8, ctx: &mut TxContext) {
    create(id, chain_id, ctx);
}

#[test_only]
public fun new_for_testing(chain_id: u8, ctx: &mut TxContext): Bridge {
    let id = object::new(ctx);
    let bridge_inner = BridgeInner {
        bridge_version: CURRENT_VERSION,
        message_version: MESSAGE_VERSION,
        chain_id,
        sequence_nums: vec_map::empty(),
        committee: committee::create(ctx),
        treasury: treasury::create(ctx),
        token_transfer_records: linked_table::new(ctx),
        limiter: limiter::new(),
        paused: false,
    };
    let mut bridge = Bridge {
        id,
        inner: versioned::create(CURRENT_VERSION, bridge_inner, ctx),
    };
    bridge.setup_treasury_for_testing();
    bridge
}

#[test_only]
public fun setup_treasury_for_testing(bridge: &mut Bridge) {
    bridge.load_inner_mut().treasury.setup_for_testing();
}

#[test_only]
public fun test_init_bridge_committee(
    bridge: &mut Bridge,
    active_validator_voting_power: VecMap<address, u64>,
    min_stake_participation_percentage: u64,
    ctx: &TxContext,
) {
    init_bridge_committee(
        bridge,
        active_validator_voting_power,
        min_stake_participation_percentage,
        ctx,
    );
}

#[test_only]
public fun new_bridge_record_for_testing(
    message: BridgeMessage,
    verified_signatures: Option<vector<vector<u8>>>,
    claimed: bool,
): BridgeRecord {
    BridgeRecord {
        message,
        verified_signatures,
        claimed,
    }
}

#[test_only]
public fun test_load_inner_mut(bridge: &mut Bridge): &mut BridgeInner {
    bridge.load_inner_mut()
}

#[test_only]
public fun test_load_inner(bridge: &Bridge): &BridgeInner {
    bridge.load_inner()
}

#[test_only]
public fun test_get_token_transfer_action_status(
    bridge: &mut Bridge,
    source_chain: u8,
    bridge_seq_num: u64,
): u8 {
    bridge.get_token_transfer_action_status(source_chain, bridge_seq_num)
}

#[test_only]
public fun test_get_token_transfer_action_signatures(
    bridge: &mut Bridge,
    source_chain: u8,
    bridge_seq_num: u64,
): Option<vector<vector<u8>>> {
    bridge.get_token_transfer_action_signatures(source_chain, bridge_seq_num)
}

#[test_only]
public fun test_get_parsed_token_transfer_message(
    bridge: &Bridge,
    source_chain: u8,
    bridge_seq_num: u64,
): Option<ParsedTokenTransferMessage> {
    bridge.get_parsed_token_transfer_message(source_chain, bridge_seq_num)
}

#[test_only]
public fun inner_limiter(bridge_inner: &BridgeInner): &TransferLimiter {
    &bridge_inner.limiter
}

#[test_only]
public fun inner_treasury(bridge_inner: &BridgeInner): &BridgeTreasury {
    &bridge_inner.treasury
}

#[test_only]
public fun inner_treasury_mut(bridge_inner: &mut BridgeInner): &mut BridgeTreasury {
    &mut bridge_inner.treasury
}

#[test_only]
public fun inner_paused(bridge_inner: &BridgeInner): bool {
    bridge_inner.paused
}

#[test_only]
public fun inner_token_transfer_records(
    bridge_inner: &BridgeInner,
): &LinkedTable<BridgeMessageKey, BridgeRecord> {
    &bridge_inner.token_transfer_records
}

#[test_only]
public fun inner_token_transfer_records_mut(
    bridge_inner: &mut BridgeInner,
): &mut LinkedTable<BridgeMessageKey, BridgeRecord> {
    &mut bridge_inner.token_transfer_records
}

#[test_only]
public fun test_execute_emergency_op(bridge_inner: &mut BridgeInner, payload: EmergencyOp) {
    bridge_inner.execute_emergency_op(payload)
}

#[test_only]
public fun sequence_nums(bridge_inner: &BridgeInner): &VecMap<u8, u64> {
    &bridge_inner.sequence_nums
}

#[test_only]
public fun assert_paused(bridge_inner: &BridgeInner, error: u64) {
    assert!(bridge_inner.paused, error);
}

#[test_only]
public fun assert_not_paused(bridge_inner: &BridgeInner, error: u64) {
    assert!(!bridge_inner.paused, error);
}

#[test_only]
public fun test_get_current_seq_num_and_increment(
    bridge_inner: &mut BridgeInner,
    msg_type: u8,
): u64 {
    get_current_seq_num_and_increment(bridge_inner, msg_type)
}

#[test_only]
public fun test_execute_update_bridge_limit(inner: &mut BridgeInner, payload: UpdateBridgeLimit) {
    execute_update_bridge_limit(inner, payload)
}

#[test_only]
public fun test_execute_update_asset_price(inner: &mut BridgeInner, payload: UpdateAssetPrice) {
    execute_update_asset_price(inner, payload)
}

#[test_only]
public fun transfer_status_pending(): u8 {
    TRANSFER_STATUS_PENDING
}

#[test_only]
public fun transfer_status_approved(): u8 {
    TRANSFER_STATUS_APPROVED
}

#[test_only]
public fun transfer_status_claimed(): u8 {
    TRANSFER_STATUS_CLAIMED
}

#[test_only]
public fun transfer_status_not_found(): u8 {
    TRANSFER_STATUS_NOT_FOUND
}

#[test_only]
public fun test_execute_add_tokens_on_sui(bridge: &mut Bridge, payload: AddTokenOnSui) {
    let inner = load_inner_mut(bridge);
    inner.execute_add_tokens_on_sui(payload);
}

#[test_only]
public fun get_seq_num_for(bridge: &mut Bridge, message_type: u8): u64 {
    let inner = load_inner_mut(bridge);
    let seq_num = if (inner.sequence_nums.contains(&message_type)) {
        inner.sequence_nums[&message_type]
    } else {
        inner.sequence_nums.insert(message_type, 0);
        0
    };
    seq_num
}

#[test_only]
public fun get_seq_num_inc_for(bridge: &mut Bridge, message_type: u8): u64 {
    let inner = load_inner_mut(bridge);
    inner.get_current_seq_num_and_increment(message_type)
}

#[test_only]
public fun transfer_approve_key(event: TokenTransferApproved): BridgeMessageKey {
    event.message_key
}

#[test_only]
public fun transfer_claimed_key(event: TokenTransferClaimed): BridgeMessageKey {
    event.message_key
}

#[test_only]
public fun transfer_already_approved_key(event: TokenTransferAlreadyApproved): BridgeMessageKey {
    event.message_key
}

#[test_only]
public fun transfer_already_claimed_key(event: TokenTransferAlreadyClaimed): BridgeMessageKey {
    event.message_key
}

#[test_only]
public fun transfer_limit_exceed_key(event: TokenTransferLimitExceed): BridgeMessageKey {
    event.message_key
}

#[test_only]
public fun unwrap_deposited_event(
    event: TokenDepositedEvent,
): (u64, u8, vector<u8>, u8, vector<u8>, u8, u64) {
    (
        event.seq_num,
        event.source_chain,
        event.sender_address,
        event.target_chain,
        event.target_address,
        event.token_type,
        event.amount,
    )
}

#[test_only]
public fun unwrap_emergency_op_event(event: EmergencyOpEvent): bool {
    event.frozen
}
