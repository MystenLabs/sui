// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A shared SUI escrow that is available only to the gas payment mechanism.
module sui::gas_allowance;

use sui::balance::Balance;
use sui::coin::{Self, Coin};
use sui::sui::SUI;

#[error(code = 0)]
const ENotEnabled: vector<u8> = b"Gas allowances are not enabled";
#[error(code = 1)]
const ENotFunder: vector<u8> = b"Only the gas allowance funder may perform this operation";
#[error(code = 2)]
const ERevoked: vector<u8> = b"Gas allowance has been revoked";

/// Shared escrow for gas payments. The gas payment mechanism is the only path that can spend its
/// balance on behalf of `spender`.
public struct GasAllowance has key {
    id: UID,
    funder: address,
    spender: address,
    balance: Balance<SUI>,
    revoked: bool,
}

/// Native storage attribution. Object-owned markers protect `amount` of their parent's deposit.
/// Address-owned claims preserve deposits released by a different gas payer, including system
/// transactions, until their funder claims them. Neither kind can be constructed or publicly transferred.
#[allow(unused_field)] // The attributed amount is read and written by native settlement.
public struct StorageRebate has key {
    id: UID,
    recipient: address,
    amount: u64,
}

/// Creates and shares an allowance funded by `payment`.
entry fun new(payment: Coin<SUI>, spender: address, ctx: &mut TxContext) {
    assert!(sui::protocol_config::is_feature_enabled(b"enable_gas_allowances"), ENotEnabled);
    transfer::share_object(GasAllowance {
        id: object::new(ctx),
        funder: ctx.sender(),
        spender,
        balance: coin::into_balance(payment),
        revoked: false,
    });
}

/// Adds SUI to an active allowance. Only its funder can add funds.
entry fun top_up(self: &mut GasAllowance, payment: Coin<SUI>, ctx: &TxContext) {
    assert_funder(self, ctx);
    assert!(!self.revoked, ERevoked);
    self.balance.join(coin::into_balance(payment));
}

/// Withdraws `amount` from an active allowance to its original funder.
entry fun withdraw(self: &mut GasAllowance, amount: u64, ctx: &mut TxContext) {
    assert_funder(self, ctx);
    assert!(!self.revoked, ERevoked);
    transfer::public_transfer(self.balance.split(amount).into_coin(ctx), self.funder);
}

/// Permanently disables an allowance and returns its remaining balance to its original funder.
entry fun revoke(self: &mut GasAllowance, ctx: &mut TxContext) {
    assert_funder(self, ctx);
    assert!(!self.revoked, ERevoked);
    self.revoked = true;
    transfer::public_transfer(self.balance.withdraw_all().into_coin(ctx), self.funder);
}

/// Claims a protected storage deposit released by another payer or by system execution.
/// The claim must be owned by its recipient; object-owned markers cannot be transaction inputs.
/// Deleting the claim refunds its deposit through the ordinary gas-payment mechanism.
entry fun claim_storage_rebate(claim: StorageRebate, ctx: &TxContext) {
    let StorageRebate { id, recipient, amount: _ } = claim;
    assert!(ctx.sender() == recipient, ENotFunder);
    id.delete();
}

/// Returns the address that funded this allowance.
public fun funder(self: &GasAllowance): address {
    self.funder
}

/// Returns the sole address authorized to use this allowance for gas.
public fun spender(self: &GasAllowance): address {
    self.spender
}

/// Returns the escrowed SUI available to the gas payment mechanism.
public fun remaining(self: &GasAllowance): u64 {
    self.balance.value()
}

/// Returns whether this allowance has been permanently revoked.
public fun revoked(self: &GasAllowance): bool {
    self.revoked
}

fun assert_funder(self: &GasAllowance, ctx: &TxContext) {
    assert!(ctx.sender() == self.funder, ENotFunder);
}
