// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_use)]
// Module for managing address alias configurations.
module sui::address_alias;

use sui::derived_object;
use sui::party;
use sui::vec_set;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> = b"Only the system can create the alias state object.";
#[error(code = 1)]
const ENoSuchAlias: vector<u8> = b"Given alias does not exist.";
#[error(code = 2)]
const EAliasAlreadyExists: vector<u8> = b"Alias already exists.";
#[error(code = 3)]
const ECannotRemoveLastAlias: vector<u8> = b"Cannot remove the last alias.";
#[error(code = 4)]
const ETooManyAliases: vector<u8> = b"The number of aliases exceeds the maximum allowed.";

const CURRENT_VERSION: u64 = 0;

const MAX_ALIASES: u64 = 8;

/// Singleton shared object which manages creation of AddressAliases state.
/// The actual alias configs are created as derived objects with this object
/// as the parent.
public struct AddressAliasState has key {
    id: UID,
    // versioned to allow for future changes
    version: u64,
}

#[allow(unused_function)]
/// Create and share the AddressAliasState object. This function is called exactly once, when
/// the address alias state object is first created.
/// Can only be called by genesis or change_epoch transactions.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    let self = AddressAliasState {
        id: object::address_alias_state(),
        version: CURRENT_VERSION,
    };
    transfer::share_object(self);
}

/// Tracks the set of addresses allowed to act as a given sender.
///
/// An alias allows transactions signed by the alias address to act as the
/// original address. For example, if address X sets an alias of address Y, then
/// then a transaction signed by Y can set its sender address to X.
public struct AddressAliases has key {
    id: UID,
    aliases: vec_set::VecSet<address>,
}

/// Internal key used for derivation of AddressAliases object addresses.
public struct AliasKey(address) has copy, drop, store;

/// Enables address alias configuration for the sender address.
///
/// By default, an address is its own alias. The provided `AddressAliases`
/// object can be used to change the set of allowed aliases after enabling.
entry fun enable(address_alias_state: &mut AddressAliasState, ctx: &TxContext) {
    assert!(
        !derived_object::exists(&address_alias_state.id, AliasKey(ctx.sender())),
        EAliasAlreadyExists,
    );
    transfer::party_transfer(
        AddressAliases {
            id: derived_object::claim(&mut address_alias_state.id, AliasKey(ctx.sender())),
            aliases: vec_set::singleton(ctx.sender()),
        },
        party::single_owner(ctx.sender()),
    );
}

/// Adds the provided address to the set of aliases for the sender.
entry fun add(aliases: &mut AddressAliases, alias: address) {
    assert!(!aliases.aliases.contains(&alias), EAliasAlreadyExists);
    aliases.aliases.insert(alias);
    assert!(aliases.aliases.length() <= MAX_ALIASES, ETooManyAliases);
}

/// Overwrites the aliases for the sender's address with the given set.
entry fun replace_all(aliases: &mut AddressAliases, new_aliases: vector<address>) {
    let new_aliases = vec_set::from_keys(new_aliases);
    assert!(new_aliases.length() > 0, ECannotRemoveLastAlias);
    assert!(new_aliases.length() <= MAX_ALIASES, ETooManyAliases);
    aliases.aliases = new_aliases;
}

/// Removes the given alias from the set of aliases for the sender's address.
entry fun remove(aliases: &mut AddressAliases, alias: address) {
    assert!(aliases.aliases.contains(&alias), ENoSuchAlias);
    assert!(aliases.aliases.length() > 1, ECannotRemoveLastAlias);
    aliases.aliases.remove(&alias);
}

#[test_only]
public fun create_for_testing(ctx: &TxContext) {
    create(ctx);
}
