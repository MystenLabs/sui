// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::bridge {
    use sui::address;
    use sui::balance;
    use sui::clock::Clock;
    use sui::coin::{Self, Coin, TreasuryCap, CoinMetadata};
    use sui::event::emit;
    use sui::linked_table::{Self, LinkedTable};
    use sui::package::UpgradeCap;
    use sui::vec_map::{Self, VecMap};
    use sui::versioned::{Self, Versioned};
    use sui_system::sui_system::SuiSystemState;

    use bridge::chain_ids;
    use bridge::committee::{Self, BridgeCommittee};
    use bridge::limiter::{Self, TransferLimiter};
    use bridge::message::{Self, BridgeMessage, BridgeMessageKey, EmergencyOp, UpdateAssetPrice,
        UpdateBridgeLimit, AddTokenOnSui
    };
    use bridge::message_types;
    use bridge::treasury::{Self, BridgeTreasury};

    #[test_only]
    use sui::{hex, test_scenario, test_utils::{assert_eq, destroy}};
    #[test_only]
    use bridge::treasury::{BTC, ETH};
    #[test_only]
    use bridge::message::create_blocklist_message;

    const MESSAGE_VERSION: u8 = 1;

    // Transfer Status
    const TRANSFER_STATUS_PENDING: u8 = 0;
    const TRANSFER_STATUS_APPROVED: u8 = 1;
    const TRANSFER_STATUS_CLAIMED: u8 = 2;
    const TRANSFER_STATUS_NOT_FOUND: u8 = 3;

    public struct Bridge has key {
        id: UID,
        inner: Versioned
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

    public struct TokenBridgeEvent has copy, drop {
        // TODO: do we need message_type here?
        message_type: u8,
        seq_num: u64,
        source_chain: u8,
        sender_address: vector<u8>,
        target_chain: u8,
        target_address: vector<u8>,
        token_type: u8,
        amount: u64
    }

    public struct EmergencyOpEvent has copy, drop {
        frozen: bool,
    }

    public struct BridgeRecord has store, drop {
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
    const EUnexpectedMessageVersion: u64 = 12;
    const EBridgeAlreadyPaused: u64 = 13;
    const EBridgeNotPaused: u64 = 14;
    const ETokenAlreadyClaimed: u64 = 15;
    const EInvalidBridgeRoute: u64 = 16;
    const EMustBeTokenMessage: u64 = 17;

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
            inner: versioned::create(CURRENT_VERSION, bridge_inner, ctx)
        };
        transfer::share_object(bridge);
    }

    #[allow(unused_function)]
    fun init_bridge_committee(
        self: &mut Bridge,
        active_validator_voting_power: VecMap<address, u64>,
        min_stake_participation_percentage: u64,
        ctx: &TxContext
    ) {
        assert!(ctx.sender() == @0x0, ENotSystemAddress);
        let inner = load_inner_mut(self);
        if (vec_map::is_empty(committee::committee_members(&inner.committee))) {
            committee::try_create_next_committee(
                &mut inner.committee,
                active_validator_voting_power,
                min_stake_participation_percentage,
                ctx
            )
        }
    }

    public fun committee_registration(
        self: &mut Bridge,
        system_state: &mut SuiSystemState,
        bridge_pubkey_bytes: vector<u8>,
        http_rest_url: vector<u8>,
        ctx: &TxContext
    ) {
        load_inner_mut(self)
            .committee
            .register(system_state, bridge_pubkey_bytes, http_rest_url, ctx);
    }

    public fun register_foreign_token<T>(self: &mut Bridge, tc: TreasuryCap<T>, uc: UpgradeCap, metadata: &CoinMetadata<T>) {
        load_inner_mut(self)
            .treasury
            .register_foreign_token<T>(tc, uc, metadata)
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
        assert!(!inner.paused, EBridgeUnavailable);
        assert!(chain_ids::is_valid_route(inner.chain_id, target_chain), EInvalidBridgeRoute);
        let amount = balance::value(coin::balance(&token));

        let bridge_seq_num = get_current_seq_num_and_increment(inner, message_types::token());
        let token_id = treasury::token_id<T>(&inner.treasury);
        let token_amount = balance::value(coin::balance(&token));

        // create bridge message
        let message = message::create_token_bridge_message(
            inner.chain_id,
            bridge_seq_num,
            address::to_bytes(ctx.sender()),
            target_chain,
            target_address,
            token_id,
            amount,
        );

        // burn / escrow token, unsupported coins will fail in this step
        inner.treasury.burn(token);

        // Store pending bridge request
        inner.token_transfer_records.push_back(message.key(), BridgeRecord {
            message,
            verified_signatures: option::none(),
            claimed: false,
        });

        // emit event
        emit(TokenBridgeEvent {
            message_type: message_types::token(),
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
        self: &mut Bridge,
        message: BridgeMessage,
        signatures: vector<vector<u8>>,
    ) {
        let inner = load_inner_mut(self);
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
        // retrieve pending message if source chain is Sui, the initial message must exist on chain.
        if (message.source_chain() == inner.chain_id) {
            let record = &mut inner.token_transfer_records[message_key];

            assert!(record.message == message, EMalformedMessageError);
            assert!(!record.claimed, EInvariantSuiInitializedTokenTransferShouldNotBeClaimed);

            // If record already has verified signatures, it means the message has been approved.
            // Then we exit early.
            if (record.verified_signatures.is_some()) {
                emit(TokenTransferAlreadyApproved { message_key });
                return
            };
            // Store approval
            record.verified_signatures = option::some(signatures)
        } else {
            // At this point, if this message is in token_transfer_records, we know it's already approved
            // because we only add a message to token_transfer_records after verifying the signatures.
            if (inner.token_transfer_records.contains(message_key)) {
                emit(TokenTransferAlreadyApproved { message_key });
                return
            };
            // Store message and approval
            inner.token_transfer_records.push_back(message_key, BridgeRecord {
                message,
                verified_signatures: option::some(signatures),
                claimed: false
            });
        };

        emit(TokenTransferApproved { message_key });
    }

    // This function can only be called by the token recipient
    // Abort if the token has already been claimed.
    public fun claim_token<T>(self: &mut Bridge, clock: &Clock, source_chain: u8, bridge_seq_num: u64, ctx: &mut TxContext): Coin<T> {
        let (maybe_token, owner) = claim_token_internal<T>(clock, self, source_chain, bridge_seq_num, ctx);
        // Only token owner can claim the token
        assert!(ctx.sender() == owner, EUnauthorisedClaim);
        assert!(maybe_token.is_some(), ETokenAlreadyClaimed);
        maybe_token.destroy_some()
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
        if (token.is_none()) {
            token.destroy_none();
            let key = message::create_key(
                source_chain,
                message_types::token(),
                bridge_seq_num
            );

            emit(TokenTransferAlreadyClaimed { message_key: key });
            return
        };

        transfer::public_transfer(
            token.destroy_some(),
            owner
        )
    }

    fun load_inner_mut(self: &mut Bridge): &mut BridgeInner {
        let version = self.inner.version();
        // TODO: Replace this with a lazy update function when we add a new version of the inner object.
        assert!(version == CURRENT_VERSION, EWrongInnerVersion);
        let inner: &mut BridgeInner = self.inner.load_value_mut();
        assert!(inner.bridge_version == version, EWrongInnerVersion);
        inner
    }

    #[allow(unused_function)] // TODO: remove annotation after implementing user-facing API
    fun load_inner(
        self: &Bridge,
    ): &BridgeInner {
        let version = self.inner.version();

        // TODO: Replace this with a lazy update function when we add a new version of the inner object.
        assert!(version == CURRENT_VERSION, EWrongInnerVersion);
        let inner: &BridgeInner = self.inner.load_value();
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
            return (option::none(), owner)
        };

        let target_chain = token_payload.token_target_chain();
        // ensure target chain matches self.chain_id
        assert!(target_chain == inner.chain_id, EUnexpectedChainID);

        // TODO: why do we check validity of the route here? what if inconsistency?
        // Ensure route is valid
        // TODO: add unit tests
        // `get_route` abort if route is invalid
        let route = chain_ids::get_route(source_chain, target_chain);
        // get owner address
        let owner = address::from_bytes(token_payload.token_target_address());
        // check token type
        assert!(treasury::token_id<T>(&inner.treasury) == token_payload.token_type(), EUnexpectedTokenType);

        let amount = token_payload.token_amount();
        // Make sure transfer is within limit.
        if (!inner.limiter.check_and_record_sending_transfer<T>(&inner.treasury, clock, route, amount)) {
            return (option::none(), owner)
        };

        // claim from treasury
        let token = inner.treasury.mint<T>(amount, ctx);

        // Record changes
        record.claimed = true;
        emit(TokenTransferClaimed { message_key: key });

        (option::some(token), owner)
    }

    public fun execute_system_message(
        self: &mut Bridge,
        message: BridgeMessage,
        signatures: vector<vector<u8>>,
    ) {
        let message_type = message.message_type();

        // TODO: test version mismatch
        assert!(message.message_version() == MESSAGE_VERSION, EUnexpectedMessageVersion);
        let inner = load_inner_mut(self);

        assert!(message.source_chain() == inner.chain_id, EUnexpectedChainID);

        // check system ops seq number and increment it
        let expected_seq_num = get_current_seq_num_and_increment(inner, message_type);
        assert!(message.seq_num() == expected_seq_num, EUnexpectedSeqNum);

        inner.committee.verify_signatures(message, signatures);

        if (message_type == message_types::emergency_op()) {
            let payload = message.extract_emergency_op_payload();
            execute_emergency_op(inner, payload);
        } else if (message_type == message_types::committee_blocklist()) {
            let payload = message.extract_blocklist_payload();
            inner.committee.execute_blocklist(payload);
        } else if (message_type == message_types::update_bridge_limit()) {
            let payload = message.extract_update_bridge_limit();
            execute_update_bridge_limit(inner, payload);
        } else if (message_type == message_types::update_asset_price()) {
            let payload = message.extract_update_asset_price();
            execute_update_asset_price(inner, payload);
        } else if (message_type == message_types::add_tokens_on_sui()) {
            let payload = message::extract_add_tokens_on_sui(&message);
            execute_add_tokens_on_sui(inner, payload);
        } else {
            abort EUnexpectedMessageType
        };
    }

    fun execute_emergency_op(inner: &mut BridgeInner, payload: EmergencyOp) {
        let op = payload.emergency_op_type();
        if (op == message::emergency_op_pause()) {
            assert!(!inner.paused, EBridgeAlreadyPaused);
            inner.paused = true;
            emit(EmergencyOpEvent { frozen: true });
        } else if (op == message::emergency_op_unpause()) {
            assert!(inner.paused, EBridgeNotPaused);
            inner.paused = false;
            emit(EmergencyOpEvent { frozen: false });
        } else {
            abort EUnexpectedOperation
        };
    }

    fun execute_update_bridge_limit(inner: &mut BridgeInner, payload: UpdateBridgeLimit) {
        let receiving_chain = payload.update_bridge_limit_payload_receiving_chain();
        assert!(receiving_chain == inner.chain_id, EUnexpectedChainID);
        let route = chain_ids::get_route(
            payload.update_bridge_limit_payload_sending_chain(),
            receiving_chain
        );

        inner.limiter.update_route_limit(
            &route,
            payload.update_bridge_limit_payload_limit()
        )
    }

    fun execute_update_asset_price(inner: &mut BridgeInner, payload: UpdateAssetPrice) {
        inner.treasury.update_asset_notional_price(
            payload.update_asset_price_payload_token_id(),
            payload.update_asset_price_payload_new_price()
        )
    }

    fun execute_add_tokens_on_sui(inner: &mut BridgeInner, payload: AddTokenOnSui) {
        let native_token = payload.is_native();
        let mut token_ids = payload.token_ids();
        let mut token_type_names = payload.token_type_names();
        let mut token_prices = payload.token_prices();

        // Make sure token data is consistent
        assert!(token_ids.length() == token_type_names.length(), EMalformedMessageError);
        assert!(token_ids.length() == token_prices.length(), EMalformedMessageError);

        while (vector::length(&token_ids) > 0) {
            let token_id = token_ids.pop_back();
            let token_type_name = token_type_names.pop_back();
            let token_price = token_prices.pop_back();
            inner.treasury.add_new_token(token_type_name, token_id, native_token, token_price)
        }
    }

    // Verify seq number matches the next expected seq number for the message type,
    // and increment it.
    fun get_current_seq_num_and_increment(self: &mut BridgeInner, msg_type: u8): u64 {
        if (!self.sequence_nums.contains(&msg_type)) {
            self.sequence_nums.insert(msg_type, 1);
            return 0
        };

        let entry = &mut self.sequence_nums[&msg_type];
        let seq_num = *entry;
        *entry = seq_num + 1;
        seq_num
    }

    public fun get_token_transfer_action_status(
        self: &mut Bridge,
        source_chain: u8,
        bridge_seq_num: u64,
    ): u8 {
        let inner = load_inner_mut(self);
        let key = message::create_key(
            source_chain,
            message_types::token(),
            bridge_seq_num
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

    #[test_only]
    fun new_for_testing(ctx: &mut TxContext, chain_id: u8): Bridge {
        let bridge_inner = BridgeInner {
            bridge_version: CURRENT_VERSION,
            message_version: MESSAGE_VERSION,
            chain_id,
            sequence_nums: vec_map::empty<u8, u64>(),
            committee: committee::create(ctx),
            treasury: treasury::mock_for_test(ctx),
            token_transfer_records: linked_table::new<BridgeMessageKey, BridgeRecord>(ctx),
            limiter: limiter::new(),
            paused: false,
        };
        Bridge {
            id: object::new(ctx),
            inner: versioned::create(CURRENT_VERSION, bridge_inner, ctx)
        }
    }

    #[test]
    #[expected_failure(abort_code = EUnexpectedChainID)]
    fun test_system_msg_incorrect_chain_id() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(ctx, chain_id);
        let blocklist = create_blocklist_message(chain_ids::sui_mainnet(), 0, 0, vector[]);
        execute_system_message(&mut bridge, blocklist, vector[]);
        destroy(bridge);
        scenario.end();
    }

    #[test]
    fun test_get_current_seq_num_and_increment() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(ctx, chain_id);

        let inner = load_inner_mut(&mut bridge);
        assert_eq(get_current_seq_num_and_increment(inner, message_types::committee_blocklist()), 0);
        assert_eq(*vec_map::get(&inner.sequence_nums, &message_types::committee_blocklist()), 1);
        assert_eq(get_current_seq_num_and_increment(inner, message_types::committee_blocklist()), 1);

        // other message type nonce does not change
        assert!(!vec_map::contains(&inner.sequence_nums, &message_types::token()), 99);
        assert!(!vec_map::contains(&inner.sequence_nums, &message_types::emergency_op()), 99);
        assert!(!vec_map::contains(&inner.sequence_nums, &message_types::update_bridge_limit()), 99);
        assert!(!vec_map::contains(&inner.sequence_nums, &message_types::update_asset_price()), 99);

        assert_eq(get_current_seq_num_and_increment(inner, message_types::token()), 0);
        assert_eq(get_current_seq_num_and_increment(inner, message_types::emergency_op()), 0);
        assert_eq(get_current_seq_num_and_increment(inner, message_types::update_bridge_limit()), 0);
        assert_eq(get_current_seq_num_and_increment(inner, message_types::update_asset_price()), 0);

        destroy(bridge);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_execute_update_bridge_limit() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let chain_id = chain_ids::sui_mainnet();
        let mut bridge = new_for_testing(ctx, chain_id);
        let inner = load_inner_mut(&mut bridge);

        // Assert the starting limit is a different value
        assert!(limiter::get_route_limit(&inner.limiter, &chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet())) != 1, 0);
        // now shrink to 1 for SUI mainnet -> ETH mainnet
        let msg = message::create_update_bridge_limit_message(
            chain_ids::sui_mainnet(), // receiving_chain
            0,
            chain_ids::eth_mainnet(), // sending_chain
            1,
        );
        let payload = message::extract_update_bridge_limit(&msg);
        execute_update_bridge_limit(inner, payload);

        // should be 1 now
        assert_eq(limiter::get_route_limit(&inner.limiter, &chain_ids::get_route(chain_ids::eth_mainnet(), chain_ids::sui_mainnet())), 1);
        // other routes are not impacted
        assert!(limiter::get_route_limit(&inner.limiter, &chain_ids::get_route(chain_ids::eth_sepolia(), chain_ids::sui_testnet())) != 1, 0);

        destroy(bridge);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = EUnexpectedChainID)]
    fun test_execute_update_bridge_limit_abort_with_unexpected_chain_id() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(ctx, chain_id);
        let inner = load_inner_mut(&mut bridge);

        // shrink to 1 for SUI mainnet -> ETH mainnet
        let msg = message::create_update_bridge_limit_message(
            chain_ids::sui_mainnet(), // receiving_chain
            0,
            chain_ids::eth_mainnet(), // sending_chain
            1,
        );
        let payload = message::extract_update_bridge_limit(&msg);
        // This abort because the receiving_chain (sui_mainnet) is not the same as the bridge's chain_id (sui_devnet)
        execute_update_bridge_limit(inner, payload);

        destroy(bridge);
        test_scenario::end(scenario);
    }


    #[test]
    fun test_execute_update_asset_price() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(ctx, chain_id);
        let inner = bridge.load_inner_mut();

        // Assert the starting limit is a different value
        assert!(inner.treasury.notional_value<BTC>() != 1_001_000_000, 0);
        // now change it to 100_001_000
        let msg = message::create_update_asset_price_message(
            inner.treasury.token_id<BTC>(),
            chain_ids::sui_mainnet(),
            0,
            1_001_000_000,
        );
        let payload = message::extract_update_asset_price(&msg);
        execute_update_asset_price(inner, payload);

        // should be 1_001_000_000 now
        assert_eq(inner.treasury.notional_value<BTC>(), 1_001_000_000);
        // other assets are not impacted
        assert!(inner.treasury.notional_value<ETH>() != 1_001_000_000, 0);

        destroy(bridge);
        scenario.end();
    }

    #[test_only]
    fun freeze_bridge(bridge: &mut Bridge) {
        let inner = load_inner_mut(bridge);
        // freeze it
        let msg = message::create_emergency_op_message(
            chain_ids::sui_testnet(),
            0, // seq num
            0, // freeze op
        );
        let payload = message::extract_emergency_op_payload(&msg);
        execute_emergency_op(inner, payload);
        assert!(inner.paused, 0);
    }

    #[test_only]
    fun unfreeze_bridge(bridge: &mut Bridge) {
        let inner = load_inner_mut(bridge);
        // unfreeze it
        let msg = message::create_emergency_op_message(
            chain_ids::sui_testnet(),
            1, // seq num, this is supposed to be the next seq num but it's not what we test here
            1, // unfreeze op
        );
        let payload = message::extract_emergency_op_payload(&msg);
        execute_emergency_op(inner, payload);
        assert!(!inner.paused, 0);
    }

    #[test]
    fun test_execute_emergency_op() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(ctx, chain_id);

        assert!(!load_inner_mut(&mut bridge).paused, 0);
        freeze_bridge(&mut bridge);

        assert!(load_inner_mut(&mut bridge).paused, 0);
        unfreeze_bridge(&mut bridge);

        destroy(bridge);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = EBridgeNotPaused)]
    fun test_execute_emergency_op_abort_when_not_frozen() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(ctx, chain_id);

        assert!(!load_inner_mut(&mut bridge).paused, 0);
        // unfreeze it, should abort
        unfreeze_bridge(&mut bridge);

        destroy(bridge);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = EBridgeUnavailable)]
    fun test_execute_send_token_frozen() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(ctx, chain_id);

        assert!(!load_inner_mut(&mut bridge).paused, 0);
        freeze_bridge(&mut bridge);

        let eth_address = b"01234"; // it does not really matter
        let btc: Coin<BTC> = coin::mint_for_testing<BTC>(1, ctx);
        send_token(
            &mut bridge,
            chain_ids::eth_sepolia(), 
            eth_address,
            btc,
            ctx,
        );

        destroy(bridge);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = EInvalidBridgeRoute)]
    fun test_execute_send_token_invalid_route() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(ctx, chain_id);

        let eth_address = b"01234"; // it does not really matter
        let btc: Coin<BTC> = coin::mint_for_testing<BTC>(1, ctx);
        send_token(
            &mut bridge,
            chain_ids::eth_mainnet(), 
            eth_address,
            btc,
            ctx,
        );

        destroy(bridge);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = EBridgeAlreadyPaused)]
    fun test_execute_emergency_op_abort_when_already_frozen() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(ctx, chain_id);
        let inner = load_inner_mut(&mut bridge);

        // initially it's unfrozen
        assert!(!inner.paused, 0);
        // freeze it
        let msg = message::create_emergency_op_message(
            chain_ids::sui_testnet(),
            0, // seq num
            0, // freeze op
        );
        let payload = message::extract_emergency_op_payload(&msg);
        execute_emergency_op(inner, payload);

        // should be frozen now
        assert!(inner.paused, 0);

        // freeze it again, should abort
        let msg = message::create_emergency_op_message(
            chain_ids::sui_testnet(),
            1, // seq num, this is supposed to be the next seq num but it's not what we test here
            0, // unfreeze op
        );
        let payload = message::extract_emergency_op_payload(&msg);
        execute_emergency_op(inner, payload);

        destroy(bridge);
        test_scenario::end(scenario);
    }


    // TODO: Add tests for execute_system_message, including message validation and effects check

    #[test]
    fun test_get_token_transfer_action_status() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(ctx, chain_id);
        let coin = coin::mint_for_testing<ETH>(12345, ctx);

        // Test when pending
        let message = message::create_token_bridge_message(
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            address::to_bytes(ctx.sender()), // sender address
            chain_ids::eth_sepolia(), // target_chain
            hex::decode(b"00000000000000000000000000000000000000c8"), // target_address
            1u8, // token_type
            balance::value(coin::balance(&coin))
        );

        let key = message::key(&message);
        linked_table::push_back(&mut load_inner_mut(&mut bridge).token_transfer_records, key, BridgeRecord {
            message,
            verified_signatures: option::none(),
            claimed: false,
        });
        assert_eq(get_token_transfer_action_status(&mut bridge, chain_id, 10), TRANSFER_STATUS_PENDING);

        // Test when ready for claim
        let message = message::create_token_bridge_message(
            chain_ids::sui_testnet(), // source chain
            11, // seq_num
            address::to_bytes(ctx.sender()), // sender address
            chain_ids::eth_sepolia(), // target_chain
            hex::decode(b"00000000000000000000000000000000000000c8"), // target_address
            1u8, // token_type
            balance::value(coin::balance(&coin))
        );
        let key = message::key(&message);
        linked_table::push_back(&mut load_inner_mut(&mut bridge).token_transfer_records, key, BridgeRecord {
            message,
            verified_signatures: option::some(vector[]),
            claimed: false,
        });
        assert_eq(get_token_transfer_action_status(&mut bridge, chain_id, 11), TRANSFER_STATUS_APPROVED);

        // Test when already claimed
        let message = message::create_token_bridge_message(
            chain_ids::sui_testnet(), // source chain
            12, // seq_num
            address::to_bytes(ctx.sender()), // sender address
            chain_ids::eth_sepolia(), // target_chain
            hex::decode(b"00000000000000000000000000000000000000c8"), // target_address
            1u8, // token_type
            balance::value(coin::balance(&coin))
        );
        let key = message::key(&message);
        linked_table::push_back(&mut load_inner_mut(&mut bridge).token_transfer_records, key, BridgeRecord {
            message,
            verified_signatures: option::some(vector[]),
            claimed: true,
        });
        assert_eq(get_token_transfer_action_status(&mut bridge, chain_id, 12), TRANSFER_STATUS_CLAIMED);

        // Test when message not found
        assert_eq(get_token_transfer_action_status(&mut bridge, chain_id, 13), TRANSFER_STATUS_NOT_FOUND);

        destroy(bridge);
        coin::burn_for_testing(coin);
        test_scenario::end(scenario);
    }
}
