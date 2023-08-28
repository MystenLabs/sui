// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The token-linker application. Allows permission-less registering of new
/// tokens and coins and validates the symbol of the token. Only available once
/// the message is received and a new token can be added.
///
///
/// Prior to registering a Token, a correct message with Symbol and Decimals
/// needs to be passed. For that to happen, the TokenRegistry has a Channel
/// inside.
///
/// Fields in the message (https://github.com/axelarnetwork/interchain-token-service/blob/main/contracts/interchain-token-service/InterchainTokenService.sol#L780):
///
/// bytes32 tokenId,
/// string memory name,
/// string memory symbol,
/// uint8 decimals,
/// bytes memory distributor, # irrelevant
/// bytes memory mintTo,      # irrelevant
/// uint256 mintAmount,       # irrelevant
/// bytes memory operator,
/// string calldata destinationChain,
/// uint256 gasValue
///
///
module axelar::token_registry {
    use std::string::{Self, String};
    use std::ascii;

    use sui::coin::{Self, Coin, TreasuryCap, CoinMetadata};
    use sui::tx_context::{sender, TxContext};
    use sui::dynamic_field as df;
    use sui::object::{Self, UID};
    use sui::bcs;

    use axelar::channel::{Self, Channel};

    /// Trying to register a token with non-zero supply.
    const ESupplyNotEmpty: u64 = 0;
    /// Token ID and Type do not match the stored Token.
    const ETokenIdMismatch: u64 = 1;
    /// Trying to register a token that already exists.
    const ETokenAlreadyExists: u64 = 2;
    /// Trying to register a token that does not match the expected name.
    const ENameMismatch: u64 = 3;
    /// Trying to register a token that does not match the expectation.
    const ETokenExpectationNotFound: u64 = 4;
    /// Trying to send a token that does not exist.
    const ETokenNotRegistered: u64 = 5;

    /// The registry of all tokens and Coins. New tokens can only be added in a
    /// verified way - the name, symbol and decimals are checked against the
    /// received 'deployStandardizedToken' message.
    struct TokenRegistry has key {
        id: UID,
        /// The channel to receive messages. Using the `bool` type as there's
        /// nothing to approve in this channel and it does not wrap any logic.
        channel: Channel<bool>,
    }

    /// The registered token. Contains Axelar-specific identification as well
    /// as the `TreasuryCap` object and the `CoinMetadata`.
    struct RegisteredToken<phantom T> has store {
        treasury_cap: TreasuryCap<T>,
        metadata: CoinMetadata<T>,
        /// TODO: consider making it the df key for easier lookup and validation.
        token_id: vector<u8>
    }

    /// After a "deploy" message has been delivered, expect a token with the
    /// given `name`, `symbol` and `decimals`.
    struct TokenExpectation has store {
        name: String,
        symbol: ascii::String,
        decimals: u8,
    }

    struct ExpectationKey has copy, store, drop { token_id: vector<u8> }

    /// The key for the `RegisteredToken` object.
    struct TokenKey<phantom T> has copy, store, drop {}

    /// The key for the `TokenExpectation` object.
    public fun add_token_expectation(
        self: &mut TokenRegistry,
        // Delivered message.
        payload: vector<u8>,
        _ctx: &mut TxContext,
    ) {
        let bytes = bcs::new(payload);
        let (token_id, name, symbol, decimals) = (
            bcs::peel_vec_u8(&mut bytes),
            bcs::peel_vec_u8(&mut bytes),
            bcs::peel_vec_u8(&mut bytes),
            bcs::peel_u8(&mut bytes),
        );

        df::add(&mut self.id, ExpectationKey { token_id }, TokenExpectation {
            name: string::utf8(name),
            symbol: string::to_ascii(string::utf8(symbol)),
            decimals
        });
    }

    /// Add a new token to the `TokenRegistry`. Once added, the token can be
    /// used by the Interchain Token Service.
    public fun add_token<T>(
        self: &mut TokenRegistry,
        treasury_cap: TreasuryCap<T>,
        metadata: CoinMetadata<T>,
        token_id: vector<u8>,
        _ctx: &mut TxContext,
    ) {
        assert!(coin::total_supply(&treasury_cap) == 0, ESupplyNotEmpty);
        assert!(!df::exists_with_type<TokenKey<T>, RegisteredToken<T>>(&self.id, TokenKey {}), ETokenAlreadyExists);
        assert!(df::exists_with_type<ExpectationKey, TokenExpectation>(&self.id, ExpectationKey { token_id }), ETokenExpectationNotFound);

        let TokenExpectation {
            name,
            symbol,
            decimals,
        } = df::remove(&mut self.id, ExpectationKey { token_id });

        assert!(coin::get_name(&metadata) == name, ENameMismatch);
        assert!(coin::get_symbol(&metadata) == symbol, ENameMismatch);
        assert!(coin::get_decimals(&metadata) == decimals, ENameMismatch);

        // assert: T matches metadata.symbol

        df::add(&mut self.id, TokenKey<T> {}, RegisteredToken {
            treasury_cap,
            metadata,
            token_id
        });
    }

    /// Send a token to another chain. The token is burned from the treasury
    /// and the amount is sent to the destination chain.
    public fun send_token<T>(
        self: &mut TokenRegistry,
        coin: Coin<T>,
        destination_chain: vector<u8>,
        destination_address: vector<u8>,
        ctx: &mut TxContext,
    ) {
        let token_data: &mut RegisteredToken<T> = df::borrow_mut(&mut self.id, TokenKey<T> {});
        let amount = coin::burn(&mut token_data.treasury_cap, coin);

        // currently using BCS to serialize the payload
        let payload = bcs::to_bytes(&vector[
            token_data.token_id,
            bcs::to_bytes(&amount),
            bcs::to_bytes(&sender(ctx)) // tx sender? or channel?
        ]);

        channel::call_contract(
            &mut self.channel,
            destination_chain,
            destination_address,
            payload,
            ctx
        )
    }

    /// Mint a new token and transfer it to the recipient.
    public fun receive_token<T>(
        self: &mut TokenRegistry,
        // message: vector<u8>
        token_id: vector<u8>,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        // parse the message
        // validate the message + match against the channel
        // get the treasury cap
        // mint and transfer to recipient specified in the message

        assert!(df::exists_with_type<TokenKey<T>, RegisteredToken<T>>(&self.id, TokenKey {}), ETokenNotRegistered);

        let token_data: &mut RegisteredToken<T> = df::borrow_mut(&mut self.id, TokenKey<T> {});
        let coin = coin::mint(&mut token_data.treasury_cap, amount, ctx);

        assert!(token_data.token_id == token_id, ETokenIdMismatch);

        sui::transfer::public_transfer(coin, recipient);
    }

    /// In the module initializer we create a single instance of the
    /// `TokenRegistry` and share it to make publicly available.
    fun init(ctx: &mut TxContext) {
        sui::transfer::share_object(TokenRegistry {
            id: object::new(ctx),
            channel: channel::create_channel(true, ctx)
        });
    }
}
