// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::tx_context;

#[test_only]
/// Number of bytes in an tx hash (which will be the transaction digest)
const TX_HASH_LENGTH: u64 = 32;

#[test_only]
/// Expected an tx hash of length 32, but found a different length
const EBadTxHashLength: u64 = 0;

#[test_only]
/// Attempt to get the most recent created object ID when none has been created.
const ENoIDsCreated: u64 = 1;

const NATIVE_CONTEXT: bool = false;

const EUnsupportedFunction: u64 = 2;

/// Information about the transaction currently being executed.
/// This cannot be constructed by a transaction--it is a privileged object created by
/// the VM and passed in to the entrypoint of the transaction as `&mut TxContext`.
public struct TxContext has drop {
    /// The address of the user that signed the current transaction
    sender: address,
    /// Hash of the current transaction
    tx_hash: vector<u8>,
    /// The current epoch number
    epoch: u64,
    /// Timestamp that the epoch started at
    epoch_timestamp_ms: u64,
    /// Counter recording the number of fresh id's created while executing
    /// this transaction. Always 0 at the start of a transaction
    ids_created: u64,
}

/// Return the address of the user that signed the current
/// transaction
public fun sender(self: &TxContext): address {
    if (NATIVE_CONTEXT) {
        self.native_sender()
    } else {
        self.sender
    }
}

/// Return the transaction digest (hash of transaction inputs).
/// Please do not use as a source of randomness.
public fun digest(self: &TxContext): &vector<u8> {
    if (NATIVE_CONTEXT) {
        self.native_digest()
    } else {
        &self.tx_hash
    }
}

/// Return the current epoch
public fun epoch(self: &TxContext): u64 {
    if (NATIVE_CONTEXT) {
        self.native_epoch()
    } else {
        self.epoch
    }
}

/// Return the epoch start time as a unix timestamp in milliseconds.
public fun epoch_timestamp_ms(self: &TxContext): u64 {
    if (NATIVE_CONTEXT) {
        self.native_epoch_timestamp_ms()
    } else {
        self.epoch_timestamp_ms
    }
}

public fun sponsor(self: &TxContext): Option<address> {
    assert!(NATIVE_CONTEXT, EUnsupportedFunction);
    self.native_sponsor()
}

/// Create an `address` that has not been used. As it is an object address, it will never
/// occur as the address for a user.
/// In other words, the generated address is a globally unique object ID.
public fun fresh_object_address(ctx: &mut TxContext): address {
    let ids_created = ctx.ids_created();
    let id = derive_id(*ctx.digest(), ids_created);
    ctx.increment_ids_created();
    id
}

fun increment_ids_created(self: &mut TxContext) {
    if (NATIVE_CONTEXT) {
        self.native_inc_ids_created()
    } else {
        self.ids_created = self.ids_created + 1
    }
}

/// Return the number of id's created by the current transaction.
/// Hidden for now, but may expose later
fun ids_created(self: &TxContext): u64 {
    if (NATIVE_CONTEXT) {
        self.native_ids_created()
    } else {
        self.ids_created
    }
}

/// Native function for deriving an ID via hash(tx_hash || ids_created)
native fun derive_id(tx_hash: vector<u8>, ids_created: u64): address;

native fun native_sender(self: &TxContext): address;

native fun native_digest(self: &TxContext): &vector<u8>;

native fun native_epoch(self: &TxContext): u64;

native fun native_epoch_timestamp_ms(self: &TxContext): u64;

native fun native_sponsor(self: &TxContext): Option<address>;

native fun native_ids_created(self: &TxContext): u64;

native fun native_inc_ids_created(self: &mut TxContext);

#[test_only]
native fun native_inc_epoch(self: &mut TxContext);

#[test_only]
native fun native_inc_epoch_timestamp(self: &mut TxContext, delta_ms: u64);

#[test_only]
native fun native_replace(
    sender: address,
    tx_hash: vector<u8>,
    epoch: u64,
    epoch_timestamp_ms: u64,
    ids_created: u64,
);

// ==== test-only functions ====

#[test_only]
/// Create a `TxContext` for testing
public fun new(
    sender: address,
    tx_hash: vector<u8>,
    epoch: u64,
    epoch_timestamp_ms: u64,
    ids_created: u64,
): TxContext {
    assert!(tx_hash.length() == TX_HASH_LENGTH, EBadTxHashLength);
    if (NATIVE_CONTEXT) {
        native_replace(sender, tx_hash, epoch, epoch_timestamp_ms, ids_created);
        TxContext {
            sender: @0x0,
            tx_hash: vector::empty(),
            epoch: 0,
            epoch_timestamp_ms: 0,
            ids_created: 0,
        }
    } else {
        TxContext { sender, tx_hash, epoch, epoch_timestamp_ms, ids_created }
    }
}

#[test_only]
/// Create a `TxContext` for testing, with a potentially non-zero epoch number.
public fun new_from_hint(
    addr: address,
    hint: u64,
    epoch: u64,
    epoch_timestamp_ms: u64,
    ids_created: u64,
): TxContext {
    new(addr, dummy_tx_hash_with_hint(hint), epoch, epoch_timestamp_ms, ids_created)
}

#[test_only]
/// Create a dummy `TxContext` for testing
public fun dummy(): TxContext {
    let tx_hash = x"3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532";
    new(@0x0, tx_hash, 0, 0, 0)
}

#[test_only]
/// Utility for creating 256 unique input hashes.
/// These hashes are guaranteed to be unique given a unique `hint: u64`
fun dummy_tx_hash_with_hint(hint: u64): vector<u8> {
    let mut tx_hash = std::bcs::to_bytes(&hint);
    while (tx_hash.length() < TX_HASH_LENGTH) tx_hash.push_back(0);
    tx_hash
}

#[test_only]
public fun get_ids_created(self: &TxContext): u64 {
    ids_created(self)
}

#[test_only]
/// Return the most recent created object ID.
public fun last_created_object_id(self: &TxContext): address {
    let ids_created = self.ids_created();
    assert!(ids_created > 0, ENoIDsCreated);
    derive_id(*self.digest(), ids_created - 1)
}

#[test_only]
public fun increment_epoch_number(self: &mut TxContext) {
    if (NATIVE_CONTEXT) {
        self.native_inc_epoch()
    } else {
        self.epoch = self.epoch + 1
    }
}

#[test_only]
public fun increment_epoch_timestamp(self: &mut TxContext, delta_ms: u64) {
    if (NATIVE_CONTEXT) {
        self.native_inc_epoch_timestamp(delta_ms)
    } else {
        self.epoch_timestamp_ms = self.epoch_timestamp_ms + delta_ms
    }
}
