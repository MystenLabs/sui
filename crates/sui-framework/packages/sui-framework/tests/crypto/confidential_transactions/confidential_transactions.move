#[allow(implicit_const_copy), test_only]
module sui::confidential_transactions;

use sui::balance::Balance;
use sui::coin::{Self, Coin};
use sui::group_ops::Element;
use sui::ristretto255::{Self, Point};
use sui::twisted_elgamal::{
    Self,
    g,
    add_assign,
    encrypted_amount_2_u_32_zero,
    encrypted_amount_4_u16_from_value,
    Encryption,
    EncryptedAmount2U32Unverified,
    EncryptedAmount4U16,
    EncryptedAmount4U32,
    encrypted_amount_4_u16,
    encrypted_amount_2_u32_unverified,
    encrypt_trivial,
    encrypt_zero,
    encrypted_amount_4_u32_from_4_u16,
    verify_value_proof,
    verify_sum_proof,
    verify_handle_eq,
    encrypted_amount_2_u32_unverified_to_encryption,
    verify_sum_proof_with_encryption,
    encrypted_amount_4_u16_to_encryption
};
use sui::vec_map::VecMap;

const EAccountAlreadyRegistered: u64 = 0;
const EInvalidInput: u64 = 1;

const U16_MAX: u16 = 65535;
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

public fun freeze_deposits<T>(ct: &mut ConfidentialToken<T>, ctx: &mut TxContext) {
    ct.accounts[&ctx.sender()].active = false;
}

public fun unfreeze_deposits<T>(ct: &mut ConfidentialToken<T>, ctx: &mut TxContext) {
    ct.accounts[&ctx.sender()].active = true;
}

// public -> private
public fun wrap<T>(
    ct: &mut ConfidentialToken<T>,
    coins: Coin<T>,
    pk: Element<Point>,
): BoundedEncryptedAmount<T> {
    let value = coins.value();
    coin::put(&mut ct.pool, coins);
    // TODO: Do we need an actual encryption (with randomness) here?
    let amount = encrypted_amount_4_u16_from_value(value, &pk);
    BoundedEncryptedAmount { pk, amount }
}

// private -> public
public fun unwrap<T>(
    ct: &mut ConfidentialToken<T>,
    ea: BoundedEncryptedAmount<T>,
    amount: u64,
    proof: vector<u8>, // Sigma proof of the encrypted msg (DDH tuple for enc - H^{eamount})
    ctx: &mut TxContext,
): Coin<T> {
    let account = &mut ct.accounts[&ctx.sender()];
    let BoundedEncryptedAmount { pk, amount: ea } = ea;
    assert!(account.pk == &pk, EInvalidInput);
    assert!(verify_value_proof(amount, &ea, &pk, proof));
    sui::coin::take(&mut ct.pool, amount, ctx)
}

/// Transfer an encrypted amount to another account. Initially, the amount is stored at the pending deposits of the destination account.
public fun transfer<T>(
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

/// Merge a pending deposit into the main balance. This is FIFO, so the first pending deposit is merged.
public fun merge_pending_deposit<T>(
    ct: &mut ConfidentialToken<T>,
    new_balance: vector<Encryption>,
    proof: vector<u8>,
    ctx: &mut TxContext,
) {
    let new_balance = encrypted_amount_2_u32_unverified(new_balance);
    let account = &mut ct.accounts[&ctx.sender()];
    let deposit = account.pending_deposits.take_deposit();

    // Verify that new_balance = old_balance + deposit
    assert!(verify_sum_proof(&new_balance, &account.balance, &deposit, &account.pk, proof));

    account.balance = new_balance;
}

/// Add an encrypted amount to the balance.
public fun add_to_balance<T>(
    ct: &mut ConfidentialToken<T>,
    amount: BoundedEncryptedAmount<T>,
    new_balance: vector<Encryption>, // expected to be EncryptedAmount2U32Unverified
    proof: vector<u8>,
    ctx: &mut TxContext,
) {
    let new_balance = encrypted_amount_2_u32_unverified(new_balance);
    let account = &mut ct.accounts[&ctx.sender()];

    let BoundedEncryptedAmount {
        pk,
        amount: deposit,
    } = amount;

    assert!(account.pk == &pk, EInvalidInput);
    assert!(
        verify_sum_proof(
            &new_balance,
            &account.balance,
            &encrypted_amount_4_u32_from_4_u16(deposit),
            &account.pk,
            proof,
        ),
    );

    account.balance = new_balance;
}

/// Take an amount from the balance.
/// The taken amount is expected to be well-formed (i.e., each limb is an u16 encryption), and should be encrypted under taken_amount_pk.
public fun take_from_balance_to_other<T>(
    ct: &mut ConfidentialToken<T>,
    new_balance: vector<Encryption>, // expected to be EncryptedAmount2U32Unverified
    taken_amount: vector<Encryption>, // Under other_pk - expected to be EncryptedAmount4U16
    decryption_handle: Element<Point>, // Under account.pk - sum of the taken_amount decryption handles
    other_pk: Element<Point>,
    proofs: vector<vector<u8>>,
    ctx: &mut TxContext,
): BoundedEncryptedAmount<T> {
    let new_balance = encrypted_amount_2_u32_unverified(new_balance);
    let taken_amount = encrypted_amount_4_u16(taken_amount);

    let account = &mut ct.accounts[&ctx.sender()];
    let pk = &account.pk;

    let mut proofs = proofs;
    let _taken_balance_range_proof = proofs.pop_back();
    let _new_balance_range_proof = proofs.pop_back();
    let balance_sum_proof = proofs.pop_back();
    let handle_eq_proof = proofs.pop_back();

    std::debug::print(&handle_eq_proof);

    // (1) check that the blinding used in decryption_handle is the same as for the taken_amount
    let total_taken_amount = twisted_elgamal::encrypted_amount_4_u16_to_encryption(&taken_amount);
    // total_taken_amount under account.pk
    if (pk != other_pk) {
        assert!(
            verify_handle_eq(
                pk,
                &other_pk,
                &decryption_handle,
                total_taken_amount.decryption_handle(),
                handle_eq_proof,
            ),
        );
    };

    std::debug::print(&25);

    // (2) current_balance = new_balance + taken_balance,
    let taken_amount_my_pk = twisted_elgamal::new(
        *total_taken_amount.ciphertext(),
        decryption_handle,
    );
    assert!(
        verify_sum_proof_with_encryption(
            &account.balance,
            &new_balance,
            &taken_amount_my_pk,
            pk,
            balance_sum_proof,
        ),
    );

    // TODO: check proofs that
    // (2) new_balance is u32 or full new_balance is u64 (not negative),
    // (3) taken_balance is u16 (batch range proofs)

    account.balance = new_balance;

    BoundedEncryptedAmount {
        pk: other_pk,
        amount: taken_amount,
    }
}

/// Take an amount from the balance.
/// The taken amount is expected to be well-formed (i.e., each limb is an u16 encryption), and should be encrypted under taken_amount_pk.
public fun take_from_balance_to_self<T>(
    ct: &mut ConfidentialToken<T>,
    new_balance: vector<Encryption>, // expected to be EncryptedAmount2U32Unverified
    taken_amount: vector<Encryption>, // Under other_pk - expected to be EncryptedAmount4U16
    proofs: vector<vector<u8>>,
    ctx: &mut TxContext,
): BoundedEncryptedAmount<T> {
    let new_balance = encrypted_amount_2_u32_unverified(new_balance);
    let taken_amount = encrypted_amount_4_u16(taken_amount);

    let account = &mut ct.accounts[&ctx.sender()];
    let pk = &account.pk;

    let mut proofs = proofs;
    let _taken_balance_range_proof = proofs.pop_back();
    let _new_balance_range_proof = proofs.pop_back();
    let balance_sum_proof = proofs.pop_back();

    assert!(
        verify_sum_proof(
            &account.balance,
            &new_balance,
            &encrypted_amount_4_u32_from_4_u16(taken_amount),
            pk,
            balance_sum_proof,
        ),
    );

    // (1) current_balance = new_balance + taken_balance,
    //assert!(verify_sum_proof_simple(&account.balance, &new_balance, &taken_amount_my_pk, pk, balance_sum_proof));

    // TODO: check proofs that
    // (2) new_balance is u32 or full new_balance is u64 (not negative),
    // (3) taken_balance is u16 (batch range proofs)

    account.balance = new_balance;

    BoundedEncryptedAmount {
        pk: *pk,
        amount: taken_amount,
    }
}

/// This represents a bounded encrypted amount: Each of the limbs is encrypted under the given PK and is a u16 amount (checked either in wrap or take_from_balance).
/// Note that this represents an amount of the coin type T and should be handled carefully.
public struct BoundedEncryptedAmount<phantom T> {
    pk: Element<Point>,
    amount: EncryptedAmount4U16,
}

/// Pending deposits for an [Account]. Deposits from other accounts shuold be added here first, and then merged into the main balance later.
public struct PendingDeposits<phantom T> has store {
    // number of deposits added to the last pending_balance.
    // Once num_of_deposits = 2^16 we create a new pending balance.
    // If pending_balance.len() > 1000, we reject the deposit.
    num_of_deposits: u16,
    pending_balances: vector<EncryptedAmount4U32>,
}

/// Add an encrypted amount to the pending deposits. The amount is expected to be well-formed (i.e., each limb is an u16 encryption).
fun add_deposit<T>(self: &mut PendingDeposits<T>, amount: EncryptedAmount4U16) {
    if (self.pending_balances.is_empty() || self.num_of_deposits == U16_MAX) {
        // If we have enough pending balances, abort
        assert!(self.pending_balances.length() < MAX_PENDING_BALANCES);

        // This is O(n), but we don't expect n to be very large
        self.pending_balances.insert(encrypted_amount_4_u32_from_4_u16(amount), 0);
        self.num_of_deposits = 1;
        return
    };
    add_assign(&mut self.pending_balances[0], &amount);
    self.num_of_deposits = self.num_of_deposits + 1;
}

// TODO: For segregated pending balances, we need to choose which pending balance to merge.
// Aborts if there are no pending balances.
fun take_deposit<T>(self: &mut PendingDeposits<T>): EncryptedAmount4U32 {
    let deposit = self.pending_balances.pop_back();
    if (self.pending_balances.is_empty()) {
        // We took the last pending balance, the one currently being used for deposits, so we need to reset
        self.num_of_deposits = 0;
    };
    deposit
}

public struct Account<phantom T> has store {
    pk: Element<Point>,
    active: bool,
    balance: EncryptedAmount2U32Unverified,
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

    // Setup addresses
    let addr1 = @0xA;
    let sk_1 = ristretto255::scalar_from_u64(12345);
    let pk_1 = ristretto255::point_mul(&sk_1, &g());

    let addr2 = @0xB;
    let sk_2 = ristretto255::scalar_from_u64(67890);
    let pk_2 = ristretto255::point_mul(&sk_2, &g());

    // Account 1 sets up currency
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

    // Define keys and register first account

    confidential_token.register_account(pk_1, scenario.ctx());
    assert!(confidential_token.accounts.length() == 1);

    // Mint and wrap some coins
    let coins = coin::mint(&mut treasury, 100, scenario.ctx());
    let wrapped = wrap(
        &mut confidential_token,
        coins,
        pk_1,
    );
    assert!(confidential_token.pool.value() == 100);
    std::debug::print(&1);

    // Add the newly minted coins to the balance of account 1
    let new_balance = vector[encrypt_trivial(100, &pk_1), encrypt_zero(&pk_1)];
    confidential_token.add_to_balance(
        wrapped,
        new_balance,
        x"002b72d4814f160298c2df462b71ad3ecda58b3819663be75c70b44c2ef8f33cfeed1c05a4091ad1c24bf5683d0d60fbc7a2aa86258aee3c65d35da00ed18b7f165610ee8e101f561e89ce0225900a7f9ea89a2e57c2c419d0105a6e6d3afa0b",
        scenario.ctx(),
    );
    std::debug::print(&2);

    // Take some from the balance and deposit to another account. Make sure to take it as encrypted to account 2
    let taken = confidential_token.take_from_balance_to_other(
        vector[encrypt_trivial(50, &pk_1), encrypt_trivial(0, &pk_1)],
        vector[
            encrypt_trivial(50, &pk_2),
            encrypt_trivial(0, &pk_2),
            encrypt_trivial(0, &pk_2),
            encrypt_trivial(0, &pk_2),
        ],
        ristretto255::point_mul(&ristretto255::scalar_from_u64(281479271743489u64), &pk_1), // decryption handle under owners pk. Note that 281479271743489 = 2^48 + 2^32 + 2^16 + 1 is the sum of the blinding factors for the four limbs when encrypting 50
        pk_2,
        vector[
            x"4ec74ffb7b9991039a09f3b076090ca410135eb33dba879068cee4356d858023fcb8b20b11c1bdb8a7858910dedebf6bfc341b408848c5899d8a917657921c6f3ee48728e43c4f994ec514b2188ca5b801e498490ae3bb1c66aabdaf8ab5c40e",
            x"305cb80cd22385166ca38a21268d8444ef51565238b82c22a5ed2c958e2cf85ce86f3ceacd6e58fea84b553f92289a24d345a73750b0242b6b72b99089b65859f6ff9546e477e3567079995f0d9e70dc58c8f45bf0137a8f8e74e24b1ae3410d",
            x"",
            x"",
        ], // TODO
        scenario.ctx(),
    );
    std::debug::print(&3);

    // Register second account and deposit
    scenario.next_tx(addr2);
    confidential_token.register_account(pk_2, scenario.ctx());

    // Account 1 deposits 50 coins to account 2
    scenario.next_tx(addr1);
    confidential_token.transfer(
        taken,
        addr2,
    );
    std::debug::print(&4);
    // Account 2 merges the pending deposit into its balance, merges and unwraps
    scenario.next_tx(addr2);
    confidential_token.merge_pending_deposit(
        vector[encrypt_trivial(50, &pk_2), encrypt_zero(&pk_2)],
        // Proof generated in fastcrypto
        x"94c23f676ffd26d996be23ca8a34d15b4ae45660c8a4f9f16dc3975023a444415ea6e591ee90950b31bfb39f5601eec1e294ebde6713c9d351dfc39e148715353e6b237b5cc782de8c21fc7f35402765247635412eff733e45d0d1bb027ad301",
        scenario.ctx(),
    );
    std::debug::print(&5);
    let taken = confidential_token.take_from_balance_to_self(
        vector[encrypt_zero(&pk_2), encrypt_zero(&pk_2)],
        vector[
            encrypt_trivial(50, &pk_2),
            encrypt_trivial(0, &pk_2),
            encrypt_trivial(0, &pk_2),
            encrypt_trivial(0, &pk_2),
        ],
        vector[
            x"d4e6e331bcabcd2507a2fb1e5137078e0cf88801fc0d78fa68486207efd2023090d3988ba69737b2e8ad52d54ccc7a6b35f8c7eb517780c4e19765167d051b63ba160348dd2d19bb50e0a109bab8ef35b2f03611df00c7aea4f80535d2ca3d01",
            x"",
            x"",
        ], // TODO
        scenario.ctx(),
    );
    std::debug::print(&6);

    let unwrapped = unwrap(
        &mut confidential_token,
        taken,
        50,
        // Proof generated in fastcrypto
        x"9ed4828b102660f6f788a5fcb390229fad5e9642f33f947c649f3e99437eee48faf9a28df164fdcc460d827d48d8a5d98d9327c6f2eda486d975a02685efb01889c6352af6c11b33913c6ae8a5cbc1ddf8e33a1bbde17fa67a6bd72abb4e1d04",
        scenario.ctx(),
    );

    std::debug::print(&7);

    assert!(confidential_token.pool.value() == 50);
    assert!(unwrapped.value() == 50);

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
