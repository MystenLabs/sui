// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Opt-in account policies that bound what a transaction signed by the account key alone may do.
///
/// Policies are dynamic fields of the singleton `AccountPolicyRegistry`, keyed by owner address.
/// Every transaction implicitly reads the registry at the version consensus assigned to it, so
/// execution can look up the sender's policy without the transaction declaring it. While a policy
/// is active, execution enforces a per-transaction SUI outflow limit, a gas budget cap, a fixed
/// package allowlist, and that no non-coin object leaves the owner's ownership. The guardian
/// exempts individual transactions by approving their digests.
///
/// A policy only takes effect `ACTIVATION_DELAY_EPOCHS` after it is enabled, and the owner can
/// cancel it before then with the key alone, so an attacker holding the key cannot lock the
/// owner out by enabling a policy with their own guardian.
module sui::account_policy;

use sui::dynamic_field as df;

const ACTIVATION_DELAY_EPOCHS: u64 = 1;
/// `activation_epoch` of a cancelled or disabled policy.
const DISABLED: u64 = 18446744073709551615;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> = b"Only the system can create the account policy registry.";
#[error(code = 1)]
const ENotGuardian: vector<u8> = b"Only the policy guardian can do this.";
#[error(code = 2)]
const EAlreadyActive: vector<u8> = b"An active policy can only be changed with guardian approval.";
#[error(code = 3)]
const ENotApproved: vector<u8> = b"This transaction has not been approved by the guardian.";

/// Singleton shared object holding every account policy as a dynamic field.
public struct AccountPolicyRegistry has key {
    id: UID,
}

/// Dynamic field key of an owner's policy.
public struct PolicyKey(address) has copy, drop, store;

/// The policy itself. Field layout is mirrored by `sui_types::account_policy::AccountPolicy`.
public struct AccountPolicy has store {
    owner: address,
    guardian: address,
    /// Maximum net SUI (in MIST) that may leave the owner's coins and stake in one transaction.
    sui_limit_per_tx: u64,
    gas_budget_cap: u64,
    /// First epoch in which the policy is enforced; `DISABLED` if cancelled or disabled.
    activation_epoch: u64,
    /// Transaction digests the guardian has exempted from the policy.
    approved_digests: vector<vector<u8>>,
}

#[allow(unused_function)]
/// Create and share the `AccountPolicyRegistry`. Called exactly once, by genesis.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    transfer::share_object(AccountPolicyRegistry { id: object::account_policy_registry() });
}

/// Opt the sender in. The policy is enforced from `ACTIVATION_DELAY_EPOCHS` epochs from now.
public fun enable(
    registry: &mut AccountPolicyRegistry,
    guardian: address,
    sui_limit_per_tx: u64,
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
            sui_limit_per_tx,
            gas_budget_cap,
            activation_epoch: ctx.epoch() + ACTIVATION_DELAY_EPOCHS,
            approved_digests: vector[],
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

/// Exempt the transaction with digest `digest` from `owner`'s policy. Guardian only.
public fun approve(
    registry: &mut AccountPolicyRegistry,
    owner: address,
    digest: vector<u8>,
    ctx: &TxContext,
) {
    let policy = registry.policy_mut(owner);
    assert!(ctx.sender() == policy.guardian, ENotGuardian);
    policy.approved_digests.push_back(digest);
}

/// Change the sender's policy. The calling transaction must itself be guardian-approved.
public fun update(
    registry: &mut AccountPolicyRegistry,
    guardian: address,
    sui_limit_per_tx: u64,
    gas_budget_cap: u64,
    ctx: &TxContext,
) {
    let policy = registry.policy_mut(ctx.sender());
    policy.assert_approved(ctx);
    policy.guardian = guardian;
    policy.sui_limit_per_tx = sui_limit_per_tx;
    policy.gas_budget_cap = gas_budget_cap;
}

/// Stop enforcing the sender's policy. The calling transaction must itself be guardian-approved.
public fun disable(registry: &mut AccountPolicyRegistry, ctx: &TxContext) {
    let policy = registry.policy_mut(ctx.sender());
    policy.assert_approved(ctx);
    policy.activation_epoch = DISABLED;
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

fun assert_approved(policy: &AccountPolicy, ctx: &TxContext) {
    assert!(policy.approved_digests.contains(ctx.digest()), ENotApproved);
}
