// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::bridge {
    use std::option;
    use std::option::{none, Option, some};

    use sui::address;
    use sui::balance;
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
    use bridge::message::{Self, BridgeMessage, BridgeMessageKey, extract_token_bridge_payload};
    use bridge::message_types;
    use bridge::treasury::{Self, BridgeTreasury, token_id};

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

    struct BridgeEvent has copy, drop {
        message: BridgeMessage,
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
    const EAlreadyClaimed: u64 = 8;
    const ERecordAlreadyExists: u64 = 9;
    const EBridgeUnavailable: u64 = 10;
    const EUnexpectedOperation: u64 = 11;
    const EInvalidBridgeRoute: u64 = 12;

    const CURRENT_VERSION: u64 = 1;

    // this method is called once in end of epoch tx to create the bridge
    #[allow(unused_function)]
    fun create(id: UID, chain_id: u8, ctx: &mut TxContext) {
        assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);
        let bridge_inner = BridgeInner {
            bridge_version: CURRENT_VERSION,
            chain_id,
            sequence_nums: vec_map::empty<u8, u64>(),
            committee: committee::create(ctx),
            treasury: treasury::create(ctx),
            bridge_records: linked_table::new<BridgeMessageKey, BridgeRecord>(ctx),
            frozen: false,
        };
        let bridge = Bridge {
            id,
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
        // create bridge message

        let message = message::create_token_bridge_message(
            inner.chain_id,
            bridge_seq_num,
            address::to_bytes(tx_context::sender(ctx)),
            target_chain,
            target_address,
            token_id<T>(),
            balance::value(coin::balance(&token))
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
        emit(BridgeEvent { message });
    }

    // Record bridge message approvals in Sui, called by the bridge client
    public fun approve_bridge_message(
        self: &mut Bridge,
        message: BridgeMessage,
        signatures: vector<vector<u8>>,
    ) {
        let inner = load_inner_mut(self);
        let key = message::key(&message);

        // retrieve pending message if source chain is Sui, the initial message must exist on chain.
        if (message::message_type(&message) == message_types::token()) {
            let payload = extract_token_bridge_payload(&message);
            if (message::token_source_chain(&payload) == inner.chain_id) {
                let record = linked_table::remove(&mut inner.bridge_records, key);
                assert!(record.message == message, EMalformedMessageError);
                // The message should be in pending state (no approval and not claimed)
                assert!(option::is_none(&record.verified_signatures), ERecordAlreadyExists);
                assert!(!record.claimed, EAlreadyClaimed)
            }
        };

        // ensure bridge massage not exist
        assert!(!linked_table::contains(&inner.bridge_records, key), ERecordAlreadyExists);

        // verify signatures
        committee::verify_signatures(&inner.committee, message, signatures);
        // Store approval
        linked_table::push_back(&mut inner.bridge_records, key, BridgeRecord {
            message,
            verified_signatures: some(signatures),
            claimed: false
        })
    }

    // Claim token from approved bridge message
    fun claim_token_internal<T>(
        self: &mut Bridge,
        source_chain: u8,
        bridge_seq_num: u64,
        ctx: &mut TxContext
    ): (Coin<T>, address) {
        let inner = load_inner_mut(self);
        let key = message::create_key(source_chain, message_types::token(), bridge_seq_num);
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
        // Ensure it is not claimed already
        assert!(!claimed, EAlreadyClaimed);
        // TODO: check approved_epoch and reject old approvals?
        // extract token message
        let token_payload = message::extract_token_bridge_payload(&message);
        let target_chain = message::token_target_chain(&token_payload);
        // ensure target chain is matches self.chain_id
        assert!(target_chain == inner.chain_id, EUnexpectedChainID);
        // Ensure route is valid
        assert!(chain_ids::is_valid_route(source_chain, target_chain), EInvalidBridgeRoute);
        // get owner address
        let owner = address::from_bytes(message::token_target_address(&token_payload));
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
        (token, owner)
    }

    // This function can only be called by the token recipient
    public fun claim_token<T>(self: &mut Bridge, source_chain: u8, bridge_seq_num: u64, ctx: &mut TxContext): Coin<T> {
        let (token, owner) = claim_token_internal<T>(self, source_chain, bridge_seq_num, ctx);
        // Only token owner can claim the token
        assert!(tx_context::sender(ctx) == owner, EUnauthorisedClaim);
        token
    }

    // This function can be called by anyone to claim and transfer the token to the recipient
    public fun claim_and_transfer_token<T>(
        self: &mut Bridge,
        source_chain: u8,
        bridge_seq_num: u64,
        ctx: &mut TxContext
    ) {
        let (token, owner) = claim_token_internal<T>(self, source_chain, bridge_seq_num, ctx);
        transfer::public_transfer(token, owner)
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
