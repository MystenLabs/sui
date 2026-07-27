// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#spending_mandate
module example::spending_mandate;

use sui::clock::Clock;
use sui::coin::Coin;
use sui::event;

// === Errors ===

const EExceedsPerTxLimit: u64 = 0;
const EExceedsTotalCap: u64 = 1;
const ERecipientNotAllowed: u64 = 2;
const EMandateExpired: u64 = 3;
const ENotMandateOwner: u64 = 4;

// === Events ===

public struct MandateCreated has copy, drop {
    mandate_id: ID,
    owner: address,
    agent: address,
    total_cap: u64,
    expires_at_ms: u64,
}

public struct SpendExecuted has copy, drop {
    mandate_id: ID,
    agent: address,
    recipient: address,
    amount: u64,
    remaining_cap: u64,
}

public struct MandateRevoked has copy, drop {
    mandate_id: ID,
    owner: address,
}

// === Objects ===

/// Capability held by the mandate owner. Required to revoke or update the mandate.
public struct MandateOwnerCap has key, store {
    id: UID,
    mandate_id: ID,
}

/// The spending mandate object, transferred to the agent.
public struct SpendingMandate has key, store {
    id: UID,
    owner: address,
    agent: address,
    max_per_tx: u64,
    total_cap: u64,
    spent: u64,
    allowed_recipients: vector<address>,
    expires_at_ms: u64,
}

// === Public functions ===

/// Create a new spending mandate. The mandate is transferred to the agent.
/// The owner receives a `MandateOwnerCap` for administrative control.
public fun create_mandate(
    agent: address,
    max_per_tx: u64,
    total_cap: u64,
    allowed_recipients: vector<address>,
    expires_at_ms: u64,
    ctx: &mut TxContext,
): MandateOwnerCap {
    let owner = ctx.sender();
    let mandate = SpendingMandate {
        id: object::new(ctx),
        owner,
        agent,
        max_per_tx,
        total_cap,
        spent: 0,
        allowed_recipients,
        expires_at_ms,
    };
    let mandate_id = object::id(&mandate);

    event::emit(MandateCreated {
        mandate_id,
        owner,
        agent,
        total_cap,
        expires_at_ms,
    });

    transfer::transfer(mandate, agent);

    MandateOwnerCap {
        id: object::new(ctx),
        mandate_id,
    }
}

/// Execute a spend against the mandate. The agent calls this within a PTB
/// alongside the actual coin transfer.
public fun execute_spend<T>(
    mandate: &mut SpendingMandate,
    payment: Coin<T>,
    recipient: address,
    clock: &Clock,
    ctx: &TxContext,
) {
    let amount = payment.value();

    // Validate constraints
    assert!(clock.timestamp_ms() < mandate.expires_at_ms, EMandateExpired);
    assert!(amount <= mandate.max_per_tx, EExceedsPerTxLimit);
    assert!(mandate.spent + amount <= mandate.total_cap, EExceedsTotalCap);
    assert!(mandate.allowed_recipients.contains(&recipient), ERecipientNotAllowed);

    // Update spent counter
    mandate.spent = mandate.spent + amount;

    let mandate_id = object::id(mandate);

    event::emit(SpendExecuted {
        mandate_id,
        agent: ctx.sender(),
        recipient,
        amount,
        remaining_cap: mandate.total_cap - mandate.spent,
    });

    // Transfer the coin to the recipient
    transfer::public_transfer(payment, recipient);
}

/// Revoke a mandate. The owner destroys the mandate, preventing further use.
/// The agent must present the mandate object for destruction.
public fun revoke_mandate(
    cap: &MandateOwnerCap,
    mandate: SpendingMandate,
) {
    assert!(cap.mandate_id == object::id(&mandate), ENotMandateOwner);

    let mandate_id = object::id(&mandate);
    let owner = mandate.owner;

    event::emit(MandateRevoked { mandate_id, owner });

    let SpendingMandate {
        id,
        owner: _,
        agent: _,
        max_per_tx: _,
        total_cap: _,
        spent: _,
        allowed_recipients: _,
        expires_at_ms: _,
    } = mandate;
    id.delete();
}

/// Update the total cap on an existing mandate. Only the owner can call this.
public fun update_cap(
    cap: &MandateOwnerCap,
    mandate: &mut SpendingMandate,
    new_total_cap: u64,
) {
    assert!(cap.mandate_id == object::id(mandate), ENotMandateOwner);
    mandate.total_cap = new_total_cap;
}

/// Add a recipient to the allowlist.
public fun add_recipient(
    cap: &MandateOwnerCap,
    mandate: &mut SpendingMandate,
    recipient: address,
) {
    assert!(cap.mandate_id == object::id(mandate), ENotMandateOwner);
    if (!mandate.allowed_recipients.contains(&recipient)) {
        mandate.allowed_recipients.push_back(recipient);
    };
}

// === View functions ===

public fun remaining_cap(mandate: &SpendingMandate): u64 {
    mandate.total_cap - mandate.spent
}

public fun is_expired(mandate: &SpendingMandate, clock: &Clock): bool {
    clock.timestamp_ms() >= mandate.expires_at_ms
}
// docs::/spending_mandate
