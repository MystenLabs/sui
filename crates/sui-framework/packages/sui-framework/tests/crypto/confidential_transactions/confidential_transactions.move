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
    encrypt_zero
};
use sui::vec_map::VecMap;

const EAccountAlreadyRegistered: u64 = 0;
const EInvalidInput: u64 = 1;

// Singleton, per conf token (TBD: init)
// TODO: consider using independent shared objects for Account, or something else.
// below I just use ConfidentialToken for simplicity.
public struct ConfidentialToken<phantom T> has key {
    id: UID,
    pool: Balance<T>,
    accounts: VecMap<address, Account<T>>,
    // auditor_key: Element<Point>, // assume there is a setter by the issuer/cap
}

public fun take_deposit<T>(
    ct: &mut ConfidentialToken<T>,
    ctx: &mut TxContext,
): EncryptedAmount4U32<T> {
    let account = ct.accounts.get_mut(&ctx.sender());

    let deposit = account.pending_deposits.pending_balance;
    account.pending_deposits.num_of_deposits = 0;

    // Reset pending balance
    account.pending_deposits.pending_balance = encrypted_amount_4_u32_zero(&account.pk);

    deposit
}

public fun new_token<T>(pool: Balance<T>, ctx: &mut TxContext): ConfidentialToken<T> {
    ConfidentialToken {
        id: object::new(ctx),
        pool,
        accounts: sui::vec_map::empty(),
    }
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
            pending_balance: encrypted_amount_4_u32_zero(&pk),
        },
        pk,
    };
    ct.accounts.insert(ctx.sender(), account);
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
    // TODO: Verify proof
    let BoundedEncryptedAmount { pk: _, amount: _ } = eamount;
    sui::coin::take(&mut ct.pool, amount, ctx)
}

public fun add_deposit<T>(
    ct: &mut ConfidentialToken<T>,
    amount: BoundedEncryptedAmount<T>,
    dest: address,
) {
    let BoundedEncryptedAmount { pk, amount } = amount;
    assert!(&pk == ct.accounts[&dest].pk);
    ct.accounts[&dest].pending_deposits.add_to_deposit(amount);
}

public fun add_to_balance<T>(
    ct: &mut ConfidentialToken<T>,
    amount: BoundedEncryptedAmount<T>,
    new_balance: vector<Encryption>, // expected to be EncryptedAmount2U32Unverified
    _proof: &vector<u8>, // Sigma protocol that the new balance is the same as the old one (though we don't use range proofs so it might be larger than 32bit)
    ctx: &mut TxContext,
) {
    assert!(new_balance.length() == 2, EInvalidInput);
    let BoundedEncryptedAmount {
        pk,
        amount: _,
    } = amount;

    assert!(&ct.accounts[&ctx.sender()].pk == &pk, EInvalidInput);

    // TODO: compute the sum and check proof

    ct.accounts[&ctx.sender()].balance = encrypted_amount_2_u32_unverified(new_balance);
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
    pending_balance: EncryptedAmount4U32<T>, // TODO: Support for up to 1000 pending balances.
}

/// Add an encrypted amount to the pending deposits. The amount is expected to be well-formed (i.e., each limb is a u16 encryption).
fun add_to_deposit<T>(self: &mut PendingDeposits<T>, ea: EncryptedAmount4U16<T>) {
    assert!(self.num_of_deposits < 65535, EInvalidInput);
    self.num_of_deposits = self.num_of_deposits + 1;
    add_assign(&mut self.pending_balance, &ea);
}

public struct Account<phantom T> has store {
    pk: Element<Point>,
    active: bool,
    balance: EncryptedAmount2U32Unverified<T>,
    pending_deposits: PendingDeposits<T>,
}

#[test_only]
fun destroy_account<T>(account: Account<T>) {
    let Account {
        pk: _,
        active: _,
        balance: _,
        pending_deposits: PendingDeposits {
            num_of_deposits: _,
            pending_balance: _,
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

    // Clean up -- TODO: Make sure these are actually empty...
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
