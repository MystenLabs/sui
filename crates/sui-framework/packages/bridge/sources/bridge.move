// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::bridge {
    use std::option;
    use std::option::{none, Option, some};

    use sui::address;
    use sui::balance;
    use sui::clock::Clock;
    use sui::coin::{Self, Coin};
    use sui::event::emit;
    use sui::linked_table::{Self, LinkedTable};
    use sui::object::UID;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::vec_map::{Self, VecMap};
    use sui::versioned::{Self, Versioned};

    use bridge::chain_ids;
    use bridge::committee::{Self, BridgeCommittee};
    use bridge::limiter;
    use bridge::limiter::TransferLimiter;
    use bridge::message::{Self, BridgeMessage, BridgeMessageKey};
    use bridge::message_types;
    use bridge::treasury::{Self, BridgeTreasury};

    struct Bridge has key {
        id: UID,
        inner: Versioned
    }

    struct BridgeInner has store {
        bridge_version: u64,
        chain_id: u8,
        // nonce for replay protection
        sequence_nums: VecMap<u8, u64>,
        // committee
        committee: BridgeCommittee,
        // Bridge treasury for mint/burn bridged tokens
        treasury: BridgeTreasury,
        bridge_records: LinkedTable<BridgeMessageKey, BridgeRecord>,
        limiter: TransferLimiter,
        frozen: bool,
    }

    // Emergency Op types
    const FREEZE: u8 = 0;
    const UNFREEZE: u8 = 1;

    struct TokenBridgeEvent has copy, drop {
        message_type: u8,
        seq_num: u64,
        source_chain: u8,
        sender_address: vector<u8>,
        target_chain: u8,
        target_address: vector<u8>,
        token_type: u8,
        amount: u64
    }

    struct BridgeRecord has store, drop {
        message: BridgeMessage,
        verified_signatures: Option<vector<vector<u8>>>,
        claimed: bool
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
    const ETokenAlreadyClaimed: u64 = 12;

    const CURRENT_VERSION: u64 = 1;

    struct TokenTransferApproved has copy, drop {
        message_key: BridgeMessageKey,
    }

    struct TokenTransferClaimed has copy, drop {
        message_key: BridgeMessageKey,
    }

    struct TokenTransferAlreadyApproved has copy, drop {
        message_key: BridgeMessageKey,
    }

    struct TokenTransferAlreadyClaimed has copy, drop {
        message_key: BridgeMessageKey,
    }

    // this method is called once in end of epoch tx to create the bridge
    #[allow(unused_function)]
    fun create(id: UID, chain_id: u8, ctx: &mut TxContext) {
        assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);
        let bridge_inner = BridgeInner {
            bridge_version: CURRENT_VERSION,
            chain_id,
            sequence_nums: vec_map::empty(),
            committee: committee::create(ctx),
            treasury: treasury::create(ctx),
            bridge_records: linked_table::new(ctx),
            limiter: limiter::new(),
            frozen: false,
        };
        let bridge = Bridge {
            id,
            inner: versioned::create(CURRENT_VERSION, bridge_inner, ctx)
        };
        transfer::share_object(bridge);
    }

    // Create bridge request to send token to other chain, the request will be in pending state until approved
    public fun send_token<T>(
        self: &mut Bridge,
        target_chain: u8,
        target_address: vector<u8>,
        token: Coin<T>,
        ctx: &mut TxContext
    ) {
        let inner = load_inner_mut(self);
        assert!(!inner.frozen, EBridgeUnavailable);
        let amount = balance::value(coin::balance(&token));

        let bridge_seq_num = next_seq_num(inner, message_types::token());
        let token_id = treasury::token_id<T>();
        let token_amount = balance::value(coin::balance(&token));

        // create bridge message
        let message = message::create_token_bridge_message(
            inner.chain_id,
            bridge_seq_num,
            address::to_bytes(tx_context::sender(ctx)),
            target_chain,
            target_address,
            token_id,
            amount,
        );

        // burn / escrow token, unsupported coins will fail in this step
        treasury::burn(&mut inner.treasury, token, ctx);

        // Store pending bridge request
        let key = message::key(&message);
        linked_table::push_back(&mut inner.bridge_records, key, BridgeRecord {
            message,
            verified_signatures: none(),
            claimed: false,
        });

        // emit event
        emit(TokenBridgeEvent {
            message_type: message_types::token(),
            seq_num: bridge_seq_num,
            source_chain: inner.chain_id,
            sender_address: address::to_bytes(tx_context::sender(ctx)),
            target_chain,
            target_address,
            token_type: token_id,
            amount: token_amount,
        });
    }

    // Record bridge message approvals in Sui, called by the bridge client
    // If already approved, return early instead of aborting.
    public fun approve_bridge_message(
        self: &mut Bridge,
        message: BridgeMessage,
        signatures: vector<vector<u8>>,
    ) {
        let inner = load_inner_mut(self);
        let key = message::key(&message);

        // retrieve pending message if source chain is Sui, the initial message must exist on chain.
        if (message::message_type(&message) == message_types::token() && message::source_chain(&message) == inner.chain_id) {
            let record = linked_table::borrow_mut(&mut inner.bridge_records, key);
            assert!(record.message == message, EMalformedMessageError);
            assert!(!record.claimed, EInvariantSuiInitializedTokenTransferShouldNotBeClaimed);

            // If record already has verified signatures, it means the message has been approved.
            // Then we exit early.
            if (option::is_some(&record.verified_signatures)) {
                emit(TokenTransferAlreadyApproved { message_key: key });
                return
            };
            // verify signatures
            committee::verify_signatures(&inner.committee, message, signatures);
            // Store approval
            record.verified_signatures = some(signatures)
        } else {
            // At this point, if this message is in bridge_records, we know it's already approved
            // because we only add a message to bridge_records after verifying the signatures.
            if (linked_table::contains(&inner.bridge_records, key)) {
                emit(TokenTransferAlreadyApproved { message_key: key });
                return
            };
            // verify signatures
            committee::verify_signatures(&inner.committee, message, signatures);
            // Store message and approval
            linked_table::push_back(&mut inner.bridge_records, key, BridgeRecord {
                message,
                verified_signatures: some(signatures),
                claimed: false
            });
        };
        emit(TokenTransferApproved { message_key: key });
    }

    // This function can only be called by the token recipient
    // Abort if the token has already been claimed.
    public fun claim_token<T>(self: &mut Bridge, clock: &Clock, source_chain: u8, bridge_seq_num: u64, ctx: &mut TxContext): Coin<T> {
        let (maybe_token, owner) = claim_token_internal<T>(clock, self, source_chain, bridge_seq_num, ctx);
        // Only token owner can claim the token
        assert!(tx_context::sender(ctx) == owner, EUnauthorisedClaim);
        assert!(option::is_some(&maybe_token), ETokenAlreadyClaimed);
        option::destroy_some(maybe_token)
    }

    // This function can be called by anyone to claim and transfer the token to the recipient
    // If the token has already been claimed, it will return instead of aborting.
    public fun claim_and_transfer_token<T>(
        self: &mut Bridge,
        clock: &Clock,
        source_chain: u8,
        bridge_seq_num: u64,
        ctx: &mut TxContext
    ) {
        let (token, owner) = claim_token_internal<T>(clock, self, source_chain, bridge_seq_num, ctx);
        if (option::is_none(&token)) {
            option::destroy_none(token);
            let key = message::create_key(source_chain, message_types::token(), bridge_seq_num);
            emit(TokenTransferAlreadyClaimed { message_key: key });
            return
        };
        transfer::public_transfer(option::destroy_some(token), owner)
    }

    public fun execute_emergency_op(
        self: &mut Bridge,
        message: BridgeMessage,
        signatures: vector<vector<u8>>,
    ) {
        assert!(message::message_type(&message) == message_types::emergency_op(), EUnexpectedMessageType);
        let inner = load_inner_mut(self);
        // check emergency ops seq number, emergency ops can only be executed in sequence order.
        let emergency_op_seq_num = next_seq_num(inner, message_types::emergency_op());
        assert!(message::seq_num(&message) == emergency_op_seq_num, EUnexpectedSeqNum);
        committee::verify_signatures(&inner.committee, message, signatures);
        let payload = message::extract_emergency_op_payload(&message);

        if (message::emergency_op_type(&payload) == FREEZE) {
            inner.frozen == true;
        } else if (message::emergency_op_type(&payload) == UNFREEZE) {
            inner.frozen == false;
        } else {
            abort EUnexpectedOperation
        };
    }

    fun load_inner_mut(
        self: &mut Bridge,
    ): &mut BridgeInner {
        let version = versioned::version(&self.inner);

        // TODO: Replace this with a lazy update function when we add a new version of the inner object.
        assert!(version == CURRENT_VERSION, EWrongInnerVersion);
        let inner: &mut BridgeInner = versioned::load_value_mut(&mut self.inner);
        assert!(inner.bridge_version == version, EWrongInnerVersion);
        inner
    }

    #[allow(unused_function)] // TODO: remove annotation after implementing user-facing API
    fun load_inner(
        self: &Bridge,
    ): &BridgeInner {
        let version = versioned::version(&self.inner);

        // TODO: Replace this with a lazy update function when we add a new version of the inner object.
        assert!(version == CURRENT_VERSION, EWrongInnerVersion);
        let inner: &BridgeInner = versioned::load_value(&self.inner);
        assert!(inner.bridge_version == version, EWrongInnerVersion);
        inner
    }

    // Claim token from approved bridge message
    // Returns Some(Coin) if coin can be claimed. If already claimed, return None
    fun claim_token_internal<T>(
        clock: &Clock,
        self: &mut Bridge,
        source_chain: u8,
        bridge_seq_num: u64,
        ctx: &mut TxContext
    ): (Option<Coin<T>>, address) {
        let inner = load_inner_mut(self);
        assert!(!inner.frozen, EBridgeUnavailable);

        let key = message::create_key(source_chain, message_types::token(), bridge_seq_num);
        assert!(linked_table::contains(&inner.bridge_records, key), EMessageNotFoundInRecords);

        // retrieve approved bridge message
        let record = linked_table::borrow_mut(&mut inner.bridge_records, key);
        // ensure this is a token bridge message
        assert!(message::message_type(&record.message) == message_types::token(), EUnexpectedMessageType);
        // Ensure it's signed
        assert!(option::is_some(&record.verified_signatures), EUnauthorisedClaim);

        // extract token message
        let token_payload = message::extract_token_bridge_payload(&record.message);
        // get owner address
        let owner = address::from_bytes(message::token_target_address(&token_payload));

        // If already claimed, exit early
        if (record.claimed) {
            return (option::none(), owner)
        };

        let target_chain = message::token_target_chain(&token_payload);
        // ensure target chain matches self.chain_id
        assert!(target_chain == inner.chain_id, EUnexpectedChainID);

        // TODO: why do we check validity of the route here? what if inconsistency?
        // Ensure route is valid
        // TODO: add unit tests
        // `get_route` abort if route is invalid
        let route = chain_ids::get_route(source_chain, target_chain);
        // get owner address
        let owner = address::from_bytes(message::token_target_address(&token_payload));
        // check token type
        assert!(treasury::token_id<T>() == message::token_type(&token_payload), EUnexpectedTokenType);
        let amount = message::token_amount(&token_payload);
        // Make sure transfer is within limit.
        if (!limiter::check_and_record_sending_transfer<T>(&mut inner.limiter, clock, route, amount)) {
            return (option::none(), owner)
        };
        // claim from treasury
        let token = treasury::mint<T>(&mut inner.treasury, amount, ctx);
        // Record changes
        record.claimed = true;
        emit(TokenTransferClaimed { message_key: key });
        (option::some(token), owner)
    }

    fun next_seq_num(self: &mut BridgeInner, msg_type: u8): u64 {
        if (!vec_map::contains(&self.sequence_nums, &msg_type)) {
            vec_map::insert(&mut self.sequence_nums, msg_type, 1);
            return 0
        };
        let (key, seq_num) = vec_map::remove(&mut self.sequence_nums, &msg_type);
        vec_map::insert(&mut self.sequence_nums, key, seq_num + 1);
        seq_num
    }
}
