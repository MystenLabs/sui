#[allow(implicit_const_copy), test_only]
module sui::confidential_transactions;

use std::unit_test::assert_eq;
use sui::balance::Balance;
use sui::coin::Coin;
use sui::config;
use sui::group_ops::Element;
use sui::ristretto255::{Self, Point, Scalar};
use sui::twisted_elgamal::{
    add_assign,
    encrypted_amount_2_u_32_zero,
    encrypted_amount_4_u16_from_value,
    encrypted_amount_4_u32_zero,
    Encryption,
    EncryptedAmount2U32Unverified,
    EncryptedAmount4U16,
    EncryptedAmount4U32,
    encrypted_amount_4_u16,
    encrypted_amount_2_u32_unverified,
    add,
    encrypt_trivial,
    encrypt_zero,
    encrypted_amount_4_u32_from_4_u16
};
use sui::vec_map::VecMap;

const EAccountAlreadyRegistered: u64 = 0;
const EInvalidInput: u64 = 1;

const MAX_PENDING_BALANCES: u64 = 1000;

// Singleton, per conf token (TBD: init)
// TODO: consider using independent shared objects for Account, or something else.
// below I just use ConfidentialToken for simplicity.
public struct ConfidentialToken<phantom T> has key {
    id: UID,
    pool: Balance<T>,
    accounts: VecMap<address, Account<T>>,
    // auditor_key: Element<Point>, // assume there is a setter by the issuer/cap
}

public fun new_token<T>(pool: Balance<T>, ctx: &mut TxContext): ConfidentialToken<T> {
    ConfidentialToken {
        id: object::new(ctx),
        pool,
        accounts: sui::vec_map::empty(),
    }
}

// public -> private
public fun wrap<T>(
    ct: &mut ConfidentialToken<T>,
    coins: Coin<T>,
    pk: Element<Point>,
): BoundedEncryptedAmount<T> {
    let value = coins.value();
    sui::coin::put(&mut ct.pool, coins);
    let amount = encrypted_amount_4_u16_from_value(value, &pk);
    BoundedEncryptedAmount { pk, amount }
}

// private -> public
public fun unwrap<T>(
    ct: &mut ConfidentialToken<T>,
    eamount: BoundedEncryptedAmount<T>,
    amount: u64,
    _proof: &vector<u8>, // Sigma proof of the encrypted msg (DDH tuple for enc - H^{eamount})
    ctx: &mut TxContext,
): Coin<T> {
    let BoundedEncryptedAmount { pk: _, amount: _ } = eamount;

    // TODO: Verify proof

    sui::coin::take(&mut ct.pool, amount, ctx)
}

/// Transfer an encrypted amount to another account. Initially, the amount is stored at the pending deposits of the destination account.
public fun deposit<T>(
    ct: &mut ConfidentialToken<T>,
    amount: BoundedEncryptedAmount<T>,
    dest: address,
) {
    let BoundedEncryptedAmount { pk, amount } = amount;
    let account = &mut ct.accounts[&dest];
    assert!(&pk == &account.pk);
    assert!(account.active);
    account.pending_deposits.add_deposit(amount);
}

public fun merge_pending_deposit<T>(
    ct: &mut ConfidentialToken<T>,
    new_balance: vector<Encryption>, // expected to be EncryptedAmount2U32Unverified,
    _proof: &vector<u8>, // Proof that the new balance is old balance + pending deposit (sigma protocol)
    ctx: &mut TxContext,
) {
    let account = &mut ct.accounts[&ctx.sender()];
    let _deposit = account.pending_deposits.take_deposit();

    // TODO: check proof
    account.balance = encrypted_amount_2_u32_unverified(new_balance);
}

public fun add_to_balance<T>(
    ct: &mut ConfidentialToken<T>,
    amount: BoundedEncryptedAmount<T>,
    new_balance: vector<Encryption>, // expected to be EncryptedAmount2U32Unverified
    _proof: &vector<u8>, // Sigma protocol that the new balance is the same as the old one (though we don't use range proofs so it might be larger than 32bit)
    ctx: &mut TxContext,
) {
    assert!(new_balance.length() == 2, EInvalidInput);
    let account = &mut ct.accounts[&ctx.sender()];

    let BoundedEncryptedAmount {
        pk,
        amount: _amount,
    } = amount;

    assert!(account.pk == &pk, EInvalidInput);

    // TODO: compute the sum and check proof

    account.balance = encrypted_amount_2_u32_unverified(new_balance);
}

public fun take_from_balance<T>(
    ct: &mut ConfidentialToken<T>,
    new_balance: vector<Encryption>, // expected to be EncryptedAmount2U32Unverified,
    taken_balance: vector<Encryption>, // expected to be EncryptedAmount4U16,
    taken_balance_pk: Element<Point>,
    _proof: &vector<u8>, // Proof that (1) current_balance = new_balance + taken_balance (sigma protocol), (2) new_balance is u32 or full new_balance is u64 (not negative), (3) taken_balance is u16 (batch range proofs)
    ctx: &mut TxContext,
): BoundedEncryptedAmount<T> {
    assert!(new_balance.length() == 2, EInvalidInput);
    assert!(taken_balance.length() == 4, EInvalidInput);

    // TODO: check proofs

    // update stored balance
    ct.accounts[&ctx.sender()].balance = encrypted_amount_2_u32_unverified(new_balance);

    BoundedEncryptedAmount {
        pk: taken_balance_pk,
        amount: encrypted_amount_4_u16(taken_balance),
    }
}

/// This represents a bounded encrypted amount: Each of the limbs is encrypted under the given PK and is a u16 amount (checked either in wrap or take_from_balance).
/// Note that this represents an amount of the coin type T.
public struct BoundedEncryptedAmount<phantom T> {
    pk: Element<Point>,
    amount: EncryptedAmount4U16<T>,
}

/// Pending deposits for an [Account]. Deposits from other accounts shuold be added here first, and then merged into the main balance later.
public struct PendingDeposits<phantom T> has store {
    // number of deposits added to the last pending_balance.
    // Once num_of_deposits = 2^16 we create a new pending balance.
    // If pending_balance.len() > 1000, we reject the deposit.
    num_of_deposits: u16,
    pending_balances: vector<EncryptedAmount4U32<T>>, // TODO: Support for up to 1000 pending balances.
}

/// Add an encrypted amount to the pending deposits. The amount is expected to be well-formed (i.e., each limb is a u16 encryption).
fun add_deposit<T>(self: &mut PendingDeposits<T>, ea: EncryptedAmount4U16<T>) {
    if (self.pending_balances.is_empty() || self.num_of_deposits == 65535) {
        // Now more pending balances allowed
        assert!(self.pending_balances.length() < MAX_PENDING_BALANCES);
        // This is O(n), but we don't expect n to be very large
        self.pending_balances.insert(encrypted_amount_4_u32_from_4_u16(ea), 0);
        self.num_of_deposits = 1;
        return;
    };
    add_assign(&mut self.pending_balances[0], &ea);
    self.num_of_deposits = self.num_of_deposits + 1;
}

// TODO: For segregated pending balances, we need to choose which pending balance to merge.
fun take_deposit<T>(self: &mut PendingDeposits<T>): EncryptedAmount4U32<T> {
    let deposit = self.pending_balances.pop_back();
    if (self.pending_balances.is_empty()) {
        // We took the last pending balance
        self.num_of_deposits = 0;
    };
    deposit
}

public struct Account<phantom T> has store {
    pk: Element<Point>,
    active: bool,
    balance: EncryptedAmount2U32Unverified<T>,
    pending_deposits: PendingDeposits<T>,
}

public fun register_account<T>(
    ct: &mut ConfidentialToken<T>,
    pk: Element<Point>,
    ctx: &mut TxContext,
) {
    assert!(!ct.accounts.contains(&ctx.sender()), EAccountAlreadyRegistered);
    let account = Account {
        active: true,
        balance: encrypted_amount_2_u_32_zero(&pk),
        pending_deposits: PendingDeposits {
            num_of_deposits: 0,
            pending_balances: vector::empty(),
        },
        pk,
    };
    ct.accounts.insert(ctx.sender(), account);
}

#[test_only]
fun destroy_account<T>(account: Account<T>) {
    let Account {
        pk: _,
        active: _,
        balance: _,
        pending_deposits: PendingDeposits {
            num_of_deposits: _,
            pending_balances: _,
        },
    } = account;
}

#[test_only]
public struct CONFIDENTIAL_TRANSACTIONS has drop {}

#[test]
fun test_flow() {
    use sui::coin;
    use sui::twisted_elgamal::{Self, encrypt_trivial, encrypt_zero, g};

    // Begins a multi-transaction scenario with addr1 as the sender
    let addr1 = @0xA;
    let mut scenario = sui::test_scenario::begin(addr1);

    let (mut treasury, metadata) = coin::create_currency(
        CONFIDENTIAL_TRANSACTIONS {},
        9,
        b"TEST",
        b"Test Coin",
        b"A test coin",
        option::none(),
        scenario.ctx(),
    );

    let balance = coin::mint_balance(&mut treasury, 0);
    let mut confidential_token = new_token(balance, scenario.ctx());
    assert!(confidential_token.pool.value() == 0);
    assert!(confidential_token.accounts.length() == 0);

    let sk_1 = ristretto255::scalar_from_u64(12345);
    let pk_1 = ristretto255::point_mul(&sk_1, &g());
    confidential_token.register_account(pk_1, scenario.ctx());
    let coins = coin::mint(&mut treasury, 100, scenario.ctx());

    assert!(confidential_token.accounts.length() == 1);

    let wrapped = wrap(
        &mut confidential_token,
        coins,
        pk_1,
    );

    assert!(confidential_token.pool.value() == 100);

    confidential_token.add_to_balance(
        wrapped,
        vector[encrypt_trivial(100, &pk_1), encrypt_zero(&pk_1)],
        &vector::empty(), // TODO
        scenario.ctx(),
    );

    let taken = confidential_token.take_from_balance(
        vector[encrypt_zero(&pk_1), encrypt_zero(&pk_1)],
        vector[
            encrypt_trivial(100, &pk_1),
            encrypt_trivial(0, &pk_1),
            encrypt_trivial(0, &pk_1),
            encrypt_trivial(0, &pk_1),
        ],
        pk_1,
        &vector::empty(), // TODO
        scenario.ctx(),
    );

    let unwrapped = unwrap(
        &mut confidential_token,
        taken,
        100,
        &vector::empty(),
        scenario.ctx(),
    );

    assert!(confidential_token.pool.value() == 0);
    assert!(unwrapped.value() == 100);

    let ConfidentialToken { mut accounts, pool, id } = confidential_token;

    treasury.burn(unwrapped);
    id.delete();

    while (!accounts.is_empty()) {
        let (_addr, account) = accounts.pop();
        destroy_account(account);
    };

    accounts.destroy_empty();
    pool.destroy_for_testing();
    metadata.destroy_metadata();
    treasury.treasury_into_supply().destroy_supply();
    scenario.end();
}
