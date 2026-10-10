// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Opt-in account policies that bound what a transaction signed by the account key alone may do.
///
/// Policies are dynamic fields of the singleton `AccountPolicyRegistry`, keyed by owner address.
/// Every transaction implicitly reads the registry at the version consensus assigned to it, so
/// execution can look up the sender's policy without the transaction declaring it. While a policy
/// is active, execution enforces a gas budget cap, a per-epoch outflow limit for each coin type
/// (gas included for SUI), a package allowlist, and that the owner's objects stay with the owner
/// unless they go to a listed recipient or are taken by a package with custody permission, itself
/// optionally capped per epoch. Spend is tracked in accumulator counters keyed by owner and epoch,
/// so it needs no sequencing between the owner's transactions. A transaction co-signed by the
/// guardian is exempt.
///
/// A policy only takes effect `ACTIVATION_DELAY_EPOCHS` after it is enabled. Until then the owner
/// can change or cancel it with the key alone, so an attacker holding the key cannot lock the
/// owner out by enabling a policy with their own guardian. Once active, every change needs the
/// guardian's co-signature.
module sui::account_policy;

use std::ascii::String;
use sui::dynamic_field as df;
use sui::vec_map::{Self, VecMap};
use sui::vec_set::{Self, VecSet};

const ACTIVATION_DELAY_EPOCHS: u64 = 1;
/// `activation_epoch` of a cancelled or disabled policy.
const DISABLED: u64 = 18446744073709551615;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> = b"Only the system can create the account policy registry.";
#[error(code = 1)]
const EAlreadyActive: vector<u8> = b"Only a pending policy can be cancelled with the key alone.";
#[error(code = 2)]
const ENotCoSignedByGuardian: vector<u8> = b"The guardian must co-sign this transaction.";

/// Singleton shared object holding every account policy as a dynamic field.
public struct AccountPolicyRegistry has key {
    id: UID,
}

/// Dynamic field key of an owner's policy.
public struct PolicyKey(address) has copy, drop, store;

/// Accumulator type of a policy's per-epoch spend counter for `T`: a coin type for coin outflow,
/// or `Custody` for objects taken by a package. Written by execution, never by Move code.
public struct Spent<phantom T> has drop {}

/// Marker for custody counters.
public struct Custody has drop {}

/// What a listed package may do with the owner's objects. Field layout is mirrored by
/// `sui_types::account_policy::PackagePermission`.
public struct PackagePermission has copy, drop, store {
    /// The package may delete, wrap, or give away the owner's objects.
    custody: bool,
    /// Object types custody is limited to, as type strings; empty means any type.
    custody_types: vector<String>,
    /// Maximum number of the owner's objects the package may take per epoch, if bounded.
    custody_limit: Option<u64>,
}

/// The policy itself. Field layout is mirrored by `sui_types::account_policy::AccountPolicy`.
public struct AccountPolicy has store {
    owner: address,
    guardian: address,
    gas_budget_cap: u64,
    /// First epoch in which the policy is enforced; `DISABLED` if cancelled or disabled.
    activation_epoch: u64,
    /// Per-epoch net outflow limit (in the coin's smallest unit) by coin type string, gas
    /// included for SUI. Types without an entry may not flow out at all.
    coin_limits: VecMap<String, u64>,
    /// Addresses (or object IDs) that coins and objects may be sent to without limit.
    recipients: VecSet<address>,
    /// Packages that may be called, by original package ID. The system package is always
    /// callable.
    packages: VecMap<ID, PackagePermission>,
}

#[allow(unused_function)]
/// Create and share the `AccountPolicyRegistry`. Called exactly once, by genesis.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    transfer::share_object(AccountPolicyRegistry { id: object::account_policy_registry() });
}

/// Opt the sender in with an empty rule set. The policy is enforced from
/// `ACTIVATION_DELAY_EPOCHS` epochs from now; configure it before then with the `set_*` calls.
public fun enable(
    registry: &mut AccountPolicyRegistry,
    guardian: address,
    gas_budget_cap: u64,
    ctx: &TxContext,
) {
    let owner = ctx.sender();
    df::add(
        &mut registry.id,
        PolicyKey(owner),
        AccountPolicy {
            owner,
            guardian,
            gas_budget_cap,
            activation_epoch: ctx.epoch() + ACTIVATION_DELAY_EPOCHS,
            coin_limits: vec_map::empty(),
            recipients: vec_set::empty(),
            packages: vec_map::empty(),
        },
    );
}

/// Cancel the sender's policy before it becomes active. Needs only the owner's key, so an
/// attacker who enabled a policy on a stolen key cannot lock the owner out.
public fun cancel(registry: &mut AccountPolicyRegistry, ctx: &TxContext) {
    let policy = registry.policy_mut(ctx.sender());
    assert!(ctx.epoch() < policy.activation_epoch, EAlreadyActive);
    policy.activation_epoch = DISABLED;
}

/// Stop enforcing the sender's active policy. The guardian must co-sign.
public fun disable(registry: &mut AccountPolicyRegistry, ctx: &TxContext) {
    let policy = registry.policy_mut(ctx.sender());
    policy.authorize(ctx);
    policy.activation_epoch = DISABLED;
}

public fun set_guardian(registry: &mut AccountPolicyRegistry, guardian: address, ctx: &TxContext) {
    let policy = registry.policy_mut(ctx.sender());
    policy.authorize(ctx);
    policy.guardian = guardian;
}

public fun set_gas_budget_cap(
    registry: &mut AccountPolicyRegistry,
    gas_budget_cap: u64,
    ctx: &TxContext,
) {
    let policy = registry.policy_mut(ctx.sender());
    policy.authorize(ctx);
    policy.gas_budget_cap = gas_budget_cap;
}

/// Set the per-epoch outflow limit of `coin_type` (e.g. `0x2::sui::SUI`).
public fun set_coin_limit(
    registry: &mut AccountPolicyRegistry,
    coin_type: String,
    limit: u64,
    ctx: &TxContext,
) {
    let policy = registry.policy_mut(ctx.sender());
    policy.authorize(ctx);
    if (policy.coin_limits.contains(&coin_type)) {
        *policy.coin_limits.get_mut(&coin_type) = limit;
    } else {
        policy.coin_limits.insert(coin_type, limit);
    }
}

public fun add_recipient(registry: &mut AccountPolicyRegistry, recipient: address, ctx: &TxContext) {
    let policy = registry.policy_mut(ctx.sender());
    policy.authorize(ctx);
    if (!policy.recipients.contains(&recipient)) {
        policy.recipients.insert(recipient);
    };
}

public fun remove_recipient(
    registry: &mut AccountPolicyRegistry,
    recipient: address,
    ctx: &TxContext,
) {
    let policy = registry.policy_mut(ctx.sender());
    policy.authorize(ctx);
    if (policy.recipients.contains(&recipient)) {
        policy.recipients.remove(&recipient);
    };
}

/// Allow calling `package` (its original ID), optionally with custody of the owner's objects,
/// limited to `custody_types` (empty for any) and to `custody_limit` objects per epoch.
public fun set_package(
    registry: &mut AccountPolicyRegistry,
    package: ID,
    custody: bool,
    custody_types: vector<String>,
    custody_limit: Option<u64>,
    ctx: &TxContext,
) {
    let policy = registry.policy_mut(ctx.sender());
    policy.authorize(ctx);
    let permission = PackagePermission { custody, custody_types, custody_limit };
    if (policy.packages.contains(&package)) {
        *policy.packages.get_mut(&package) = permission;
    } else {
        policy.packages.insert(package, permission);
    }
}

public fun remove_package(registry: &mut AccountPolicyRegistry, package: ID, ctx: &TxContext) {
    let policy = registry.policy_mut(ctx.sender());
    policy.authorize(ctx);
    if (policy.packages.contains(&package)) {
        policy.packages.remove(&package);
    };
}

public fun exists(registry: &AccountPolicyRegistry, owner: address): bool {
    df::exists(&registry.id, PolicyKey(owner))
}

public fun activation_epoch(registry: &AccountPolicyRegistry, owner: address): u64 {
    df::borrow<PolicyKey, AccountPolicy>(&registry.id, PolicyKey(owner)).activation_epoch
}

fun policy_mut(registry: &mut AccountPolicyRegistry, owner: address): &mut AccountPolicy {
    df::borrow_mut(&mut registry.id, PolicyKey(owner))
}

/// A pending policy is the owner's to shape; an active one changes only with the guardian.
fun authorize(policy: &AccountPolicy, ctx: &TxContext) {
    if (ctx.epoch() >= policy.activation_epoch) {
        assert!(ctx.co_signers().contains(&policy.guardian), ENotCoSignedByGuardian);
    }
}
