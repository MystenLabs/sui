// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::bridge {
    use std::option;
    use std::option::{none, Option, some};
    use sui::object::{Self, UID};
    use sui::address;
    use sui::balance;
    use sui::coin::{Self, Coin};
    use sui::coin::TreasuryCap;
    use sui::event::emit;
    use sui::linked_table::{Self, LinkedTable};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::vec_map::{Self, VecMap};
    use sui::versioned::{Self, Versioned};

    use bridge::chain_ids::{Self, sui_local_test};
    use bridge::committee::{Self, BridgeCommittee};
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
    // const ENotSystemAddress: u64 = 5;
    const EUnexpectedSeqNum: u64 = 6;
    const EWrongInnerVersion: u64 = 7;
    const EBridgeUnavailable: u64 = 10;
    const EUnexpectedOperation: u64 = 11;
    const EInvalidBridgeRoute: u64 = 12;

    const EInvariantSuiInitializedTokenTransferShouldNotBeClaimed: u64 = 13;
    const EMessageNotFoundInRecords: u64 = 14;

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

    fun init(ctx: &mut TxContext) {

        let treasury = treasury::create(ctx);

        let bridge_inner = BridgeInner {
            bridge_version: CURRENT_VERSION,
            // TODO: how do we make this configurable?
            chain_id: sui_local_test(),
            sequence_nums: vec_map::empty<u8, u64>(),
            committee: committee::create(ctx),
            treasury: treasury,
            bridge_records: linked_table::new<BridgeMessageKey, BridgeRecord>(ctx),
            frozen: false,
        };
        let bridge = Bridge {
            id: object::new(ctx),
            inner: versioned::create(CURRENT_VERSION, bridge_inner, ctx)
        };
        transfer::share_object(bridge);
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

    // THIS IS ONLY FOR TESTING, MUST NOT CHECK INTO PROD
    public fun add_treasury_cap<T>(
        self: &mut Bridge,
        treasury_cap: TreasuryCap<T>,
    ) {
        let inner = load_inner_mut(self);
        treasury::add_treasury_cap(&mut inner.treasury, treasury_cap)
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
        assert!(chain_ids::is_valid_route(inner.chain_id, target_chain), EInvalidBridgeRoute);
        assert!(!inner.frozen, EBridgeUnavailable);
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
            token_amount,
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

        // TODO: use borrow mut

        // retrieve pending message if source chain is Sui, the initial message must exist on chain.
        if (message::message_type(&message) == message_types::token() && message::source_chain(&message) == inner.chain_id) {
            let record = linked_table::remove(&mut inner.bridge_records, key);
            assert!(record.message == message, EMalformedMessageError);
            assert!(!record.claimed, EInvariantSuiInitializedTokenTransferShouldNotBeClaimed);

            // If record already has verified signatures, it means the message has been approved.
            // Then we push this message back to bridge_records and exit early.
            if (option::is_some(&record.verified_signatures)) {
                emit(TokenTransferAlreadyApproved { message_key: key });
                linked_table::push_back(&mut inner.bridge_records, key, record);
                return
            }
        };

        // At this point, if this message is in bridge_records, we know it's already approved
        // because we only add a message to bridge_records after verifying the signatures.
        if (linked_table::contains(&inner.bridge_records, key)) {
            emit(TokenTransferAlreadyApproved { message_key: key });
            return
        };

        // At this point, we know the message has not been approved, hence has not been claimed.
        // verify signatures
        committee::verify_signatures(&inner.committee, message, signatures);

        // Critical: here we set `claimed` as false. It's vitally important to make sure
        // the token transfer has not been claimed already.
        // Store approval
        linked_table::push_back(&mut inner.bridge_records, key, BridgeRecord {
            message,
            verified_signatures: some(signatures),
            claimed: false
        });
        emit(TokenTransferApproved { message_key: key });
    }

    // Claim token from approved bridge message
    // Returns Some(Coin) if coin can be claimed. If already claimed, return None
    fun claim_token_internal<T>(
        self: &mut Bridge,
        source_chain: u8,
        bridge_seq_num: u64,
        ctx: &mut TxContext
    ): (Option<Coin<T>>, address) {
        let inner = load_inner_mut(self);
        let key = message::create_key(source_chain, message_types::token(), bridge_seq_num);
        if (!linked_table::contains(&inner.bridge_records, key)) {
            abort EMessageNotFoundInRecords
        };
        // TODO: use borrow mut
        // retrieve approved bridge message
        let BridgeRecord {
            message,
            verified_signatures: signatures,
            claimed
        } = linked_table::remove(&mut inner.bridge_records, key);
        // ensure this is a token bridge message
        assert!(message::message_type(&message) == message_types::token(), EUnexpectedMessageType);
        // Ensure it's signed
        assert!(option::is_some(&signatures), EUnauthorisedClaim);

        // extract token message
        let token_payload = message::extract_token_bridge_payload(&message);
        // get owner address
        let owner = address::from_bytes(message::token_target_address(&token_payload));

        // If already claimed, exit early
        if (claimed) {
            emit(TokenTransferAlreadyClaimed { message_key: key });
            linked_table::push_back(&mut inner.bridge_records, key, BridgeRecord {
                message,
                verified_signatures: signatures,
                claimed: true // <-- this is important
            });
            return (option::none(), owner)
        };

        let target_chain = message::token_target_chain(&token_payload);
        // ensure target chain matches self.chain_id
        assert!(target_chain == inner.chain_id, EUnexpectedChainID);

        // TODO: why do we check validity of the route here? what if inconsistency?

        // Ensure route is valid
        assert!(chain_ids::is_valid_route(source_chain, target_chain), EInvalidBridgeRoute);
        // check token type
        assert!(treasury::token_id<T>() == message::token_type(&token_payload), EUnexpectedTokenType);
        // claim from treasury
        let token = treasury::mint<T>(&mut inner.treasury, message::token_amount(&token_payload), ctx);
        // Record changes
        linked_table::push_back(&mut inner.bridge_records, key, BridgeRecord {
            message,
            verified_signatures: signatures,
            claimed: true
        });
        emit(TokenTransferClaimed { message_key: key });
        (option::some(token), owner)
    }

    // This function can only be called by the token recipient
    // Returns None if the token has already been claimed.
    public fun claim_token<T>(self: &mut Bridge, source_chain: u8, bridge_seq_num: u64, ctx: &mut TxContext): Option<Coin<T>> {
        let (token, owner) = claim_token_internal<T>(self, source_chain, bridge_seq_num, ctx);
        // Only token owner can claim the token
        assert!(tx_context::sender(ctx) == owner, EUnauthorisedClaim);
        token
    }

    // This function can be called by anyone to claim and transfer the token to the recipient
    // If the token has already been claimed, it will return instead of aborting.
    public fun claim_and_transfer_token<T>(
        self: &mut Bridge,
        source_chain: u8,
        bridge_seq_num: u64,
        ctx: &mut TxContext
    ) {
        let (token, owner) = claim_token_internal<T>(self, source_chain, bridge_seq_num, ctx);
        if (option::is_none(&token)) {
            option::destroy_none(token);
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
