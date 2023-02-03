// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A token bridge implementation example.
/// Allows anyone to create a bridge for a unique symbol by publishing a module.
///
/// Guarantees:
///  - there can only be 1 Supply per symbol ("eth" or "btc" or any other)
///  - users can't cheat on the system and create multiple `Supply`'s or `TreasuryCap`'s
///  - supply is empty by default unless some amount is first transferred to Sui
///
/// Message format:
///  - each message must target the TokenBridge `Channel`
///  - message symbol must must exist in the `TokenBridge` as a dynamic field
///  - fields of the incoming messages are: ( amount, symbol, receiver )
///  - fields of the outcoming messages are identical to the incoming
///
/// Flow:
///  1. user publishes a package and requests for a `AddTokenCap` in the
///  initializer (see `eth` example).
///
///  2. with this capability, the method `add_token` can be called; it makes
///  sure that the symbol is unique in the system and then publishes the `Supply<T>`
///  as a dynamic field of the `TokenBridge`
///
///  3. from this moment mint messages for a symbol T can be received by a TokenBridge, and the
///  receiver specified in the message will receive a Coin<T> once a message is processed
///
/// Potential improvements:
///
///  - the flow with requesting a Capability first is a requirement to bypass
///  current limitations. This step could be removed if module initializer supported
///  external arguments.
///
///  - due to `balance::Supply` not being destructable; if someone creates a bridge
///  with a symbol that was already registered, they won't be able to destroy their
///  `AddTokenCap` - this could either be improved by allowing Supply unpacking in
///  the sui framework or allowing `store` ability on the OTW in the Sui Verifier.
///
module axelar::token_bridge {
    use sui::object::{Self, UID};
    use sui::balance::{Self, Supply};
    use sui::transfer::{share_object, transfer};
    use sui::tx_context::{TxContext};
    use sui::dynamic_field as df;
    use sui::coin::{Self, Coin};
    use sui::bcs;

    use std::ascii::{Self, String};
    use std::type_name;

    use axelar::messenger::{Self, Axelar, Channel};

    /// For when a witness passed is not an OTW.
    const ENotOTW: u64 = 0;

    /// A Capability enabling a bridge creation request. Currently
    /// required, because there's no way to pass `TokenRegistry` as
    /// an argument to the module initializer nor there's a way to
    /// store an OTW.
    struct AddTokenCap<phantom T: drop> has key, store {
        id: UID,
        /// Symbol for the Coin / Token. Read from the type of the T.
        /// For now (and for simplicity's sake) - lowercased.
        symbol: String,
        /// Supply for the future token T. Unfortunately, can not
        /// be destroyed (even empty) yet. That's a possible flow that
        /// could be changed in the `sui::balance` module.
        supply: Supply<T>
    }

    /// The Token bridge. Controls minting and burning of new `Coin`s.
    struct TokenBridge has key {
        id: UID,
        /// Channel for the TokenBridge - messages to mint a Coin need to be targeted at
        /// this channel. Also used when sending `burn` events to the Axelar chain.
        /// Contains a boolean value as a placeholder for missing T in the Channel.
        channel: Channel<bool>
    }

    /// Coin sent data for faster BCS serialization.
    struct CoinSent has drop {
        amount: u64,
        symbol: String,
        receiver: vector<u8>
    }

    /// In the module initializer we create a single `TokenRegistry`.
    fun init(ctx: &mut TxContext) {
        share_object(TokenBridge {
            id: object::new(ctx),
            channel: messenger::create_channel(true, ctx)
        })
    }

    /// TokenBridge creation requires an OTW - can only be called in a module initializer.
    /// Additionally we only count the name of the T as the token symbol.
    public fun get_token_creation_cap<T: drop>(otw: T, ctx: &mut TxContext): AddTokenCap<T> {
        assert!(sui::types::is_one_time_witness(&otw), ENotOTW);

        // lowercase name of the module; due to OTW having the name of
        // the module + uppercase and since we check for an OTW above,
        // this way of getting the symbol can be consirered valid.
        let symbol = type_name::get_module(&type_name::get<T>());
        let supply = balance::create_supply(otw);

        AddTokenCap { id: object::new(ctx), symbol, supply }
    }

    /// Add a token to the `TokenBridge` using a `AddTokenCap` (previously acquired
    /// through module publishing).
    ///
    /// TODO (DevX):
    ///  does not check if a key exists and aborts with `df::EFieldAlreadyExists` if
    ///  it does; add a custom check + custom abort code for the scenario
    public entry fun add_token<T: drop>(
        registry: &mut TokenBridge,
        cap: AddTokenCap<T>,
        _ctx: &mut TxContext
    ) {
        let AddTokenCap { id: cap_id, symbol, supply } = cap;
        df::add<String, Supply<T>>(&mut registry.id, symbol, supply);
        object::delete(cap_id);
    }

    /// Process a mint message from the Axelar chain.
    ///
    /// If a message was targeted to this channel's bridge and contains the
    /// correct symbol (matches the `TokenBridge`), mint some `Coin<T>` based on
    /// the message data (custom payload):
    ///  - amount
    ///  - symbol
    ///  - receiver (20 bytes in Sui)
    public entry fun process_mint_message<T: drop>(
        axelar: &mut Axelar,
        bridge: &mut TokenBridge,
        msg_id: vector<u8>,
        ctx: &mut TxContext
    ) {
        let (
            _channel_data,
            _source_chain,
            _source_address,
            _payload_hash,
            payload
        ) = messenger::consume_message(axelar, &mut bridge.channel, msg_id);

        let bcs = bcs::new(payload);
        let (amount, symbol, receiver) = (
            bcs::peel_u64(&mut bcs),
            bcs::peel_vec_u8(&mut bcs),
            bcs::peel_address(&mut bcs)
        );

        let supply_mut = df::borrow_mut<String, Supply<T>>(&mut bridge.id, ascii::string(symbol));
        let balance = balance::increase_supply(supply_mut, amount);
        let coin = coin::from_balance(balance, ctx);

        transfer(coin, receiver)
    }

    /// Send the Coin<T> from this `TokenBridge` to some network X.
    ///
    /// Effectively burns the Coin and emits a message authorized by a bridge Channel.
    /// The message cointains: `amount`, `symbol` and `receiver` (as vector<u8>).
    public entry fun send_coin<T: drop>(
        bridge: &mut TokenBridge,
        coin: Coin<T>,
        destination: vector<u8>,
        destination_address: vector<u8>,
        receiver: vector<u8>
    ) {
        let symbol = type_name::get_module(&type_name::get<T>());
        let supply_mut = df::borrow_mut<String, Supply<T>>(&mut bridge.id, symbol);
        let balance = coin::into_balance(coin);
        let amount = balance::decrease_supply(supply_mut, balance);

        messenger::send_message(
            &mut bridge.channel,
            destination,
            destination_address,
            bcs::to_bytes(&CoinSent {
                amount,
                symbol,
                receiver
            })
        )
    }
}

/// An example module for adding an ETH bridge on Sui.
/// The module name will be used a symbol in the bridge.
module axelar::eth {
    use sui::tx_context::{TxContext, sender};
    use sui::transfer::transfer;
    use axelar::token_bridge;

    /// The type for the bridge.
    struct ETH has drop {}

    /// Create an OTW (ETH) in the initializer and get a Cap
    /// which then can be used to create an actual bridge.
    fun init(eth: ETH, ctx: &mut TxContext) {
        let cap = token_bridge::get_token_creation_cap(eth, ctx);
        transfer(cap, sender(ctx))
    }

    // Second transaction after publishing this module will be:
    // sui client call \
    //      --package axelar \
    //      --module token_bridge \
    //      --function add_token \
    //      --args \
    //          <token_registry> \
    //          <bridge_cap_id>
}
