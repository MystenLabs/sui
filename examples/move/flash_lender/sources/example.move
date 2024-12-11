// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A flash loan that works for any Coin type
module flash_lender::example;

use sui::{balance::{Self, Balance}, coin::{Self, Coin}};

/// A shared object offering flash loans to any buyer willing to pay `fee`.
public struct FlashLender<phantom T> has key {
    id: UID,
    /// Amount available to be lent to prospective borrowers
    to_lend: Balance<T>,
    /// Value of `Coin<T>`'s that will be charged for the loan.  In
    /// practice, this would probably be a percentage, but we use a flat fee
    /// here for simplicity.
    fee: u64,
}

/// A "hot potato" struct recording the number of `Coin<T>`'s that were
/// borrowed. It cannot be discarded because it does not have `drop`, it
/// cannot be put in persistent storage because it does not have `key`, and
/// it cannot be transferred or wrapped because it does not have `store`.
///
/// Thus the only way to get rid of it is to call `repay` at some point in
/// the transaction that created it, forcing the debtor to pay back the
/// debt in a successful transaction.
public struct Receipt<phantom T> {
    /// ID of the flash lender object the debtor borrowed from.
    flash_lender_id: ID,
    /// Total funds to repay: amount borrowed + the fee.
    repay_amount: u64,
}

/// One `AdminCap` is created for every `FlashLender`.  Its owner can
/// control the funds held in that `FlashLender`.
public struct AdminCap has key, store {
    id: UID,
    flash_lender_id: ID,
}

// === Error codes ===

/// Attempted to borrow more than the `FlashLender` has.  Try borrowing a
/// smaller amount.
const ELoanTooLarge: u64 = 0;

/// Tried to repay an amount other than `repay_amount` (i.e., the amount
/// borrowed + the fee).  Try repaying the proper amount.
const EInvalidRepaymentAmount: u64 = 1;

/// Attempted to repay a `FlashLender` that was not the source of this
/// particular debt.  Try repaying the correct lender.
const ERepayToWrongLender: u64 = 2;

/// Attempted to perform an admin-only operation without valid permissions
/// Try using the correct `AdminCap`
const EAdminOnly: u64 = 3;

/// Attempted to withdraw more than the `FlashLender` has.  Try withdrawing
/// a smaller amount.
const EWithdrawTooLarge: u64 = 4;

// === Public Functions ===

/// Create a shared `FlashLender` object that makes `to_lend` available for
/// borrowing.  Any borrower will need to repay the borrowed amount and
/// `fee` by the end of the current transaction.
public fun new<T>(to_lend: Balance<T>, fee: u64, ctx: &mut TxContext): AdminCap {
    let id = object::new(ctx);
    let flash_lender_id = object::uid_to_inner(&id);
    let flash_lender = FlashLender { id, to_lend, fee };

    // make the `FlashLender` a shared object so anyone can request loans
    transfer::share_object(flash_lender);

    // give the creator admin permissions
    AdminCap { id: object::new(ctx), flash_lender_id }
}

/// Request a loan of `amount` from `lender`. The returned `Receipt<T>` "hot
/// potato" ensures that the borrower will call `repay(lender, ...)` later
/// on in this tx.  Aborts if `amount` is greater that the amount that
/// `lender` has available for lending.
public fun loan<T>(
    self: &mut FlashLender<T>,
    amount: u64,
    ctx: &mut TxContext,
): (Coin<T>, Receipt<T>) {
    assert!(balance::value(&self.to_lend) >= amount, ELoanTooLarge);

    let loan = coin::take(&mut self.to_lend, amount, ctx);
    let repay_amount = amount + self.fee;
    let flash_lender_id = object::id(self);
    let receipt = Receipt { flash_lender_id, repay_amount };

    (loan, receipt)
}

/// Repay the loan recorded by `receipt` to `lender` with `payment`.  Aborts
/// if the repayment amount is incorrect or `lender` is not the
/// `FlashLender` that issued the original loan.
public fun repay<T>(self: &mut FlashLender<T>, payment: Coin<T>, receipt: Receipt<T>) {
    let Receipt { flash_lender_id, repay_amount } = receipt;

    assert!(object::id(self) == flash_lender_id, ERepayToWrongLender);
    assert!(coin::value(&payment) == repay_amount, EInvalidRepaymentAmount);

    coin::put(&mut self.to_lend, payment)
}

// === Accessor Functions ===

/// Return the current fee for `self`
public fun fee<T>(self: &FlashLender<T>): u64 {
    self.fee
}

/// Return the maximum amount available for borrowing
public fun max_loan<T>(self: &FlashLender<T>): u64 {
    balance::value(&self.to_lend)
}

/// Return the amount that the holder of `self` must repay
public fun repay_amount<T>(self: &Receipt<T>): u64 {
    self.repay_amount
}

/// Return the id of the FlashLender object
public fun flash_lender_id<T>(self: &Receipt<T>): ID {
    self.flash_lender_id
}

// === Admin-only functions ===

/// Allow admin for `self` to withdraw funds.
public fun withdraw<T>(
    self: &mut FlashLender<T>,
    admin: &AdminCap,
    amount: u64,
    ctx: &mut TxContext,
): Coin<T> {
    // only the holder of the `AdminCap` for `self` can withdraw funds
    assert!(object::borrow_id(self) == &admin.flash_lender_id, EAdminOnly);
    assert!(balance::value(&self.to_lend) >= amount, EWithdrawTooLarge);

    coin::take(&mut self.to_lend, amount, ctx)
}

/// Allow admin to add more funds to `self`
public fun deposit<T>(self: &mut FlashLender<T>, admin: &AdminCap, coin: Coin<T>) {
    // only the holder of the `AdminCap` for `self` can deposit funds
    assert!(object::borrow_id(self) == &admin.flash_lender_id, EAdminOnly);
    coin::put(&mut self.to_lend, coin);
}

/// Allow admin to update the fee for `self`
public fun update_fee<T>(self: &mut FlashLender<T>, admin: &AdminCap, new_fee: u64) {
    // only the holder of the `AdminCap` for `self` can update the fee
    assert!(object::borrow_id(self) == &admin.flash_lender_id, EAdminOnly);
    self.fee = new_fee
}

// === Tests ===
#[test_only]
use sui::sui::SUI;
#[test_only]
use sui::test_scenario as ts;

#[test_only]
const ADMIN: address = @0xAD;
#[test_only]
const ALICE: address = @0xA;

#[test]
fun test_flash_loan() {
    let mut ts = ts::begin(@0x0);

    // Admin creates a flash lender with 100 coins and a fee of 1 coin.
    {
        ts::next_tx(&mut ts, ADMIN);
        let coin = coin::mint_for_testing<SUI>(100, ts::ctx(&mut ts));
        let bal = coin::into_balance(coin);
        let cap = new(bal, 1, ts::ctx(&mut ts));
        transfer::public_transfer(cap, ADMIN);
    };

    // Alice requests and repays a loan of 10 coins and the fee
    {
        ts::next_tx(&mut ts, ALICE);

        let mut lender = ts::take_shared(&ts);
        let (loan, receipt) = loan(&mut lender, 10, ts::ctx(&mut ts));

        // Simulate Alice making enough profit to repay.
        let mut profit = coin::mint_for_testing<SUI>(1, ts::ctx(&mut ts));
        coin::join(&mut profit, loan);

        repay(&mut lender, profit, receipt);
        ts::return_shared(lender);
    };

    // Admin withdraws 1 coin profit
    {
        ts::next_tx(&mut ts, ADMIN);
        let cap = ts::take_from_sender(&ts);
        let mut lender: FlashLender<SUI> = ts::take_shared(&ts);

        // Max loan increased because of the fee payment
        assert!(max_loan(&lender) == 101, 0);

        // Withdraw a coin from the pool for lending
        let coin = withdraw(&mut lender, &cap, 1, ts::ctx(&mut ts));
        transfer::public_transfer(coin, ADMIN);

        ts::return_shared(lender);
        ts::return_to_sender(&ts, cap);
    };

    ts::end(ts);
}
