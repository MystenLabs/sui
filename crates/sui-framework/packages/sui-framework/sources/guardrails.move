// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Guardrails let an account bound how much of each balance its key alone can move per epoch.
/// The rules of an account are its `Policy`, a shared object whose ID is derived from the owner
/// address. Execution takes the sender's policy as an implicit mutable input, checks what left the
/// sender's coins and address balance against the limits, and records the spend in the policy. A
/// transaction sponsored by the policy's guardian is exempt.
module sui::guardrails;

use std::type_name::{Self, TypeName};
use sui::derived_object;
use sui::sui::SUI;
use sui::vec_map::{Self, VecMap};

#[error(code = 0)]
const ENotSystemAddress: vector<u8> = b"Only the system can create the guardrails registry.";
#[error(code = 1)]
const ENotOwner: vector<u8> = b"Only the owner of a policy can change it.";
#[error(code = 2)]
const ENotAuthorized: vector<u8> =
    b"Changing an active policy requires the guardian to sponsor the transaction.";
#[error(code = 3)]
const EGuardianIsOwner: vector<u8> = b"The guardian must differ from the owner.";
#[error(code = 4)]
const ERecoveryDelayOutOfRange: vector<u8> = b"The recovery delay is outside the allowed range.";
#[error(code = 5)]
const ETooManyLimits: vector<u8> = b"The policy limits too many coin types.";
#[error(code = 6)]
const EAlreadyActive: vector<u8> = b"The policy is pending or active.";
#[error(code = 7)]
const ENotPending: vector<u8> = b"The policy is not pending.";
#[error(code = 8)]
const ENotActive: vector<u8> = b"The policy is not active.";
#[error(code = 9)]
const EDisablePending: vector<u8> = b"The policy is already set to become inactive.";
#[error(code = 10)]
const ENoPendingDisable: vector<u8> = b"The policy is not set to become inactive.";
#[error(code = 11)]
const ENotGuardian: vector<u8> = b"Only a transaction sponsored by the guardian can do this.";

/// A policy limits at most this many coin types. The bound keeps the object small, since every
/// transaction of the account rewrites it.
const MAX_LIMITED_TYPES: u64 = 32;
/// A policy cannot choose a shorter recovery delay than this.
const MIN_RECOVERY_DELAY_EPOCHS: u64 = 14;
/// A policy cannot choose a longer recovery delay than this. The bound keeps `request_disable`
/// from overflowing.
const MAX_RECOVERY_DELAY_EPOCHS: u64 = 365;
const NEVER: u64 = 0xFFFF_FFFF_FFFF_FFFF;

/// The registry is a system object created at genesis. It serves only as the parent that policy
/// IDs derive from.
public struct GuardrailsRegistry has key {
    id: UID,
}

/// Derivation key: the policy of `owner` has the ID derived from the registry and
/// `PolicyKey(owner)`.
public struct PolicyKey(address) has copy, drop, store;

/// Each account has at most one policy, which is shared. `sui_types::guardrails::Policy` mirrors
/// its field layout.
public struct Policy has key {
    id: UID,
    owner: address,
    guardian: address,
    /// First epoch in which the policy is enforced. `NEVER` once cancelled.
    active_from: u64,
    /// First epoch in which the policy is no longer enforced. `NEVER` while no end is set.
    active_until: u64,
    /// Most of each coin type that may leave the account per epoch. A type without an entry
    /// cannot leave at all.
    limits: VecMap<TypeName, u64>,
    /// Epochs between `request_disable` and the policy becoming inactive.
    recovery_delay_epochs: u64,
    /// The epoch `spent` belongs to. Written by execution, never by this module.
    spent_epoch: u64,
    /// What left the account in `spent_epoch`, by coin type. Written by execution.
    spent: VecMap<TypeName, u64>,
}

#[allow(unused_function)]
/// Create and share the `GuardrailsRegistry`. Called once, by genesis.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    transfer::share_object(GuardrailsRegistry { id: object::guardrails_registry() });
}

/// Creates the sender's policy. It is enforced from the next epoch on; until then the sender can
/// configure or `cancel` it with the key alone.
public fun enable(
    registry: &mut GuardrailsRegistry,
    guardian: address,
    sui_limit: u64,
    recovery_delay_epochs: u64,
    ctx: &TxContext,
) {
    let owner = ctx.sender();
    assert!(guardian != owner, EGuardianIsOwner);
    assert!(
        recovery_delay_epochs >= MIN_RECOVERY_DELAY_EPOCHS &&
        recovery_delay_epochs <= MAX_RECOVERY_DELAY_EPOCHS,
        ERecoveryDelayOutOfRange,
    );
    let mut limits = vec_map::empty();
    limits.insert(type_name::with_defining_ids<SUI>(), sui_limit);
    transfer::share_object(Policy {
        id: derived_object::claim(&mut registry.id, PolicyKey(owner)),
        owner,
        guardian,
        active_from: ctx.epoch() + 1,
        active_until: NEVER,
        limits,
        recovery_delay_epochs,
        spent_epoch: 0,
        spent: vec_map::empty(),
    });
}

/// Enables a policy that was cancelled or has become inactive, keeping its limits.
public fun re_enable(policy: &mut Policy, guardian: address, ctx: &TxContext) {
    let epoch = ctx.epoch();
    policy.assert_authorized(ctx);
    assert!(guardian != policy.owner, EGuardianIsOwner);
    assert!(policy.active_from == NEVER || policy.active_until <= epoch, EAlreadyActive);
    policy.guardian = guardian;
    policy.active_from = epoch + 1;
    policy.active_until = NEVER;
}

/// Cancels a policy before it becomes active.
public fun cancel(policy: &mut Policy, ctx: &TxContext) {
    policy.assert_authorized(ctx);
    assert!(policy.active_from != NEVER && policy.active_from > ctx.epoch(), ENotPending);
    policy.active_from = NEVER;
}

/// Ends an active policy at the next epoch. The guardian must sponsor the transaction.
public fun disable(policy: &mut Policy, ctx: &TxContext) {
    policy.assert_authorized(ctx);
    assert!(policy.is_active(ctx.epoch()), ENotActive);
    policy.active_until = ctx.epoch() + 1;
}

/// Sets the per-epoch limit of coin type `T`.
public fun set_limit<T>(policy: &mut Policy, limit: u64, ctx: &TxContext) {
    policy.assert_authorized(ctx);
    let coin_type = type_name::with_defining_ids<T>();
    if (policy.limits.contains(&coin_type)) {
        *policy.limits.get_mut(&coin_type) = limit;
    } else {
        assert!(policy.limits.length() < MAX_LIMITED_TYPES, ETooManyLimits);
        policy.limits.insert(coin_type, limit);
    }
}

/// Removes the limit of coin type `T`, after which `T` cannot leave the account at all.
public fun remove_limit<T>(policy: &mut Policy, ctx: &TxContext) {
    policy.assert_authorized(ctx);
    let (_, _) = policy.limits.remove(&type_name::with_defining_ids<T>());
}

public fun set_guardian(policy: &mut Policy, guardian: address, ctx: &TxContext) {
    policy.assert_authorized(ctx);
    assert!(guardian != policy.owner, EGuardianIsOwner);
    policy.guardian = guardian;
}

/// Ends an active policy after its recovery delay, with the key alone. This is the way out for
/// an owner who lost the guardian. Only the guardian can undo it, so a stolen key that requests
/// it cannot also hold it open, and cannot push an owner's own request further out.
public fun request_disable(policy: &mut Policy, ctx: &TxContext) {
    assert!(policy.owner == ctx.sender(), ENotOwner);
    assert!(policy.is_active(ctx.epoch()), ENotActive);
    assert!(policy.active_until == NEVER, EDisablePending);
    policy.active_until = ctx.epoch() + policy.recovery_delay_epochs;
}

/// Keeps an active policy that `disable` or `request_disable` set to end. The guardian must
/// sponsor the transaction.
public fun cancel_disable(policy: &mut Policy, ctx: &TxContext) {
    assert!(policy.owner == ctx.sender(), ENotOwner);
    assert!(ctx.sponsor() == option::some(policy.guardian), ENotGuardian);
    assert!(policy.active_until != NEVER && policy.active_until > ctx.epoch(), ENoPendingDisable);
    policy.active_until = NEVER;
}

fun is_active(policy: &Policy, epoch: u64): bool {
    policy.active_from <= epoch && epoch < policy.active_until
}

/// Returns the policy ID of `owner`, whether or not the policy exists.
public fun policy_address(registry: &GuardrailsRegistry, owner: address): address {
    derived_object::derive_address(object::id(registry), PolicyKey(owner))
}

/// Every change comes from the owner, and a change to an active policy needs the guardian as the
/// transaction's sponsor.
fun assert_authorized(policy: &Policy, ctx: &TxContext) {
    assert!(policy.owner == ctx.sender(), ENotOwner);
    assert!(
        !policy.is_active(ctx.epoch()) || ctx.sponsor() == option::some(policy.guardian),
        ENotAuthorized,
    );
}
