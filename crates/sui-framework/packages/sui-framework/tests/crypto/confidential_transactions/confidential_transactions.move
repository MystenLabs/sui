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
    encrypted_amount_4_u16_to_encryption,
    encrypted_amount_2_u_32_zero_verify_non_negative
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
    let new_balance_range_proof = proofs.pop_back();
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
    assert!(
        encrypted_amount_2_u_32_zero_verify_non_negative(&new_balance, &new_balance_range_proof),
    );
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
    let new_balance_range_proof = proofs.pop_back();
    let balance_sum_proof = proofs.pop_back();

    // (1) current_balance = new_balance + taken_balance,
    assert!(
        verify_sum_proof(
            &account.balance,
            &new_balance,
            &encrypted_amount_4_u32_from_4_u16(taken_amount),
            pk,
            balance_sum_proof,
        ),
    );

    // (2) new_balance is u32 or full new_balance is u64 (not negative),
    assert!(
        encrypted_amount_2_u_32_zero_verify_non_negative(&new_balance, &new_balance_range_proof),
    );

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
            x"a00538b4ad0cbbda8d89776300f698ee7d54c6ceaff9739069a6386aeaca8ae3ec44068731bc8835977574b1cf3cfe497e335e4aaa53e53b8235a2951d6cc1b003500c70d34e53b7cf2ab9f8d21e7740529f17d477e6ebba8c610c940eefbb2dfb609a7aa96518ea78c30488f8a24284393d6346345d01f1ad93b05cc6c17da6e751fd8ba91c51032cf8704dd4afa4114ccdca5d3d9d70e9316e7650c076dca30a01bbefdd3c46f43d79292008967c4f232096813194875cdc185e08e902a3b6e80bb2b745ec08d81a0821c92d7c908547b7b8634157063324365a4d75a38dc7ca0a20b3a73503e3566f5b251e96cbd08047f355f50d26d915e37d8e479e30bd265b6c5a77daffe86c3d32b0197cb0d7013d58bacc313d49d11a524757bd22b3081dc21caada4cdd3fbfa83ee8bad11d9ec18af9fc06b0915293738ad9b88a79ae00423a48de0aa62df58ccc150db1f86579c112dcfa464fed7383c45215465b3161f2e1b896a6e1e9d2961c639145beb5ccfb6d1e4a280005ab8c666879df880e15f02dd97bad72148b266f80b06c350993908d5de1e52c38dda238848686df4e44e6ef4e1d518b0a21c42a67ce0fdd1df7d2a715844964a7ca77e6a6c90ba90a78b43857609bdccde5543e1461d412291a5cdb9d2f026b0e5e0dcd54d72fd6dc39a0766a866b315c52ca83f23bfb76ab2170548978a678cbd042d9ffed20eb4441104d4bd9144e78bcf3a64a157987e61e2ae24e79582b5dc7dcf1dafc4b3edc57a8557341878ee50f15c0b8fd52b3dba57d667f1497154f133a35e9bd16f5f63c7cc5163657a539a7a135440dbccebaeb1348cb1bded858d15740b91e91b28f7d6abefa05d6754c992843d575bb0cad8846b1a6d02f9d398c8f9d519b7a5deb06f4c1427a777175fb23f2eb16a577eb2ec31f28e2186bb0dbf79b3cb2abb41408",
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
            x"a00514fffca8e77d8f9bfca17e33d8454a3f7fa8834720b271f8841678f3b2552f490cc64dac5588a18589cd7ded16c7364bdc5903bbfcfd563682e6e290147de84dccbad55be7241ebd0865f2b5a52c38566a9c059ff1aafc8a1aba056ec7c02e04b84bce1537bfd1541ee720eb0280ee9161d5b15e6b79e194d2db5ab9ca62800760fd1aeaccb540fd2c93a1a3d0094079ad79874a289cb9d554eb06b4f78e16072cc647bbd7dce21fadd48bd704b54afbdaf7ee870bcd9e7f5551e8283cdddf08110fe806470b9bf8c3fa60d00b6ebb4953258799c2643a2b8d165abc5b2a170494a344be2e3342f4bef1257fec1a15a3117a177f8bc0572db0ac47455dca946842c3b5f383ff89a77cdfccd413d09a1035c30f6b6f63eeb24a9f7d5d761baf397c777540eec41f050c21b07cf0e6a2e6b289cb70baca2b2853143de1a08f7f7d4ac7d9de7966cc30b6534cb817afb1add42f6b27b9a034ee42f67b495fb92301823fa76f0b4306c57469d034b4985401039aa4e84e5927bf05c8248ea2465b1aeabb37ed7dc3beefd65389db5f513798b2ae9a132d4d545e31b0aab4596774421a9592086e1bbe13a7aae661d43049e98e28100ba5541b5c51fa5d982b7af364629addffc445ec67c489d8cb0a31e639680e2a05370453f0623e11a1a7446357e0ef935bd966052c648bc46df6cea7585a5ca0e2d49a70a0e58664f77d1af664c0b01e85ca5dceaee43c0e1017d2ef97a51348e152c1db371f87055ea3699c4d6c17df8f23361a936134ec7d80d44cb80234ea151563ba4b37536d3be31ee65e98e93746c43b81ecd052694532f4a0eb5ef429807417a6c95f03176585e7287e44083c9e134ef8ea97cd9d2ceb30ac81cdcf8dcea39ccce774335c1065712b095f11c07232b31d522d8b9a8681935d1dd96f78415dd47bc58fe9226f8c02ba03",
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
