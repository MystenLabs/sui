#[test_only]
module DiemFramework::ApprovalGroup {
    use DiemFramework::Signature;

    struct ApprovalGroup has store {
        // we do not have collection support in Move now, so illustrate
        // using 2 out-of 3

        // for simplicity just use the plain public key here. We could
        // also use the hash here as how auth key works
        pk1: vector<u8>,
        pk2: vector<u8>,
        pk3: vector<u8>,

        // the threshold policy
        threshold: u64,

        // Recipient address allowlist policy ...
    }

    // create a new approval group
    public fun create(
        pk1: vector<u8>,
        pk2: vector<u8>,
        pk3: vector<u8>,
    ): ApprovalGroup {
        ApprovalGroup {pk1, pk2, pk3, threshold: 2}
    }

    // helper function to evaluate the threshold policy
    fun verify_sig(
        group: &ApprovalGroup,
        pk: vector<u8>,
        sig: vector<u8>,
        hash: vector<u8>,
    ): bool {
        (
            copy pk == *&group.pk1 ||
            copy pk == *&group.pk2 ||
            copy pk == *&group.pk3
        ) && Signature::ed25519_verify(sig, pk, hash)
    }

    // evaluate whether the approval group can exercise its authority
    // right now only the threshold policy is checked
    public fun has_authority(
        group: &ApprovalGroup,
        pk1: vector<u8>,
        sig1: vector<u8>,
        pk2: vector<u8>,
        sig2: vector<u8>,
        hash: vector<u8>,
    ): bool {
        assert!(copy pk1 != copy pk2, 1000);
        let result1 = verify_sig(group, pk1, sig1, copy hash);
        let result2 = verify_sig(group, pk2, sig2, hash);
        result1 && result2
    }
}

#[test_only]
module DiemFramework::ColdWallet {
    use DiemFramework::XUS::XUS;
    use std::hash;
    use std::bcs;
    use DiemFramework::Diem;
    use std::vector;
    use std::signer;
    use DiemFramework::ApprovalGroup;

    struct ColdWallet has key {
        balance: Diem::Diem<XUS>,
        sequence_num: u64,
        genesis_group: ApprovalGroup::ApprovalGroup,
    }

    // This struct is unused, only intended to define the format of a transaction
    // the serialization of the transaction is the concatnation of all the fields
    struct Transaction {
        // The address that is going to be paid
        payee: address,
        // The amount of Diem<XUS> sent
        amount: u64
    }

    // create a new ColdWallet with a default genesis group
    public fun create(
        account: &signer,
        genesis_group: ApprovalGroup::ApprovalGroup,
    ) {
        let zero_balance = Diem::zero();
        let wallet = ColdWallet {
            balance: zero_balance,
            sequence_num: 0,
            genesis_group,
        };
        move_to<ColdWallet>(account, wallet);
    }

    public fun publish(account: &signer, self: ColdWallet) {
        move_to<ColdWallet>(account, self);
    }

    // deposit money into a payee's cold wallet
    public fun deposit(
        payee: address,
        to_deposit: Diem::Diem<XUS>,
    ) acquires ColdWallet {
        // Load the payee's account
        let payee_wallet_ref = borrow_global_mut<ColdWallet>(payee);
        // Deposit the `to_deposit` coin
        Diem::deposit(&mut payee_wallet_ref.balance, to_deposit);
    }

    // helper to get the expected serialization of a transaction
    // serialization format: 'prefix' || payee_address || amount || sequence_number
    fun get_transaction_bytes(
        payer: address,
        payee: address,
        amount: u64,
        wallet: &mut ColdWallet,
    ): vector<u8> {
        // TODO: consider moving into resource
        // TODO: Move doesn't support string now. As a workaround,
        // TODO: the prefix is the hex encoding of "coldwallet.transaction"
        let constant = x"636F6C6477616C6C65742E7472616E73616374696F6E";
        let payer_bytes = bcs::to_bytes(&payer);

        let transaction_bytes = copy payer_bytes;
        vector::append(&mut transaction_bytes, constant);

        let payee_bytes = bcs::to_bytes(&payee);
        let amount_bytes = bcs::to_bytes(&amount);
        let seq_bytes = bcs::to_bytes(&wallet.sequence_num);

        vector::append(&mut transaction_bytes, payee_bytes);
        vector::append(&mut transaction_bytes, amount_bytes);
        vector::append(&mut transaction_bytes, seq_bytes);

        transaction_bytes
    }

    // withdraw money from this wallet, and send to a payee account
    // Note that this implementation moves the fund into the payee's
    // Diem account, without assuming there's a cold wallet module
    // under that account
    public fun withdraw_from_payer(
        payer_: &signer,
        payee: address,
        amount: u64,
        pk1: vector<u8>,
        sig1: vector<u8>,
        pk2: vector<u8>,
        sig2: vector<u8>
    ) acquires ColdWallet {
        let payer = signer::address_of(payer_);
        let payer_ref = borrow_global_mut<ColdWallet>(payer);
        let account_balance = Diem::value(&payer_ref.balance);
        assert!(amount <= account_balance, 1001);

        // obtain the expected serialization of the transaction struct
        let transaction_bytes = get_transaction_bytes(
            payer,
            payee,
            amount,
            payer_ref,
        );


        let hash = hash::sha3_256(transaction_bytes);
        let has_authority = ApprovalGroup::has_authority(
            &payer_ref.genesis_group,
            pk1,
            sig1,
            pk2,
            sig2,
            hash,
        );
        // check to see if genesis group has authority to approve
        if (has_authority) {
            // bump the sequence number
            payer_ref.sequence_num = payer_ref.sequence_num + 1;

            let withdraw_amount = Diem::withdraw(&mut payer_ref.balance, amount);
            // DiemAccount no longer has this API
            //DiemAccount::deposit(payer_, payee, withdraw_amount);
            Diem::destroy_zero(withdraw_amount);
        };
    }
}

#[test_only]
module DiemFramework::WalletModuleTests {
    use DiemFramework::Genesis;
    use DiemFramework::DiemAccount;
    use DiemFramework::XUS::XUS;
    use std::signer;

    use DiemFramework::ApprovalGroup;
    use DiemFramework::ColdWallet;

    // private-key: 2f2bbeb071948c4ca586b0fa0f3663520fc3053056e79955059520d626117cdb
    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, account = @0xc2422587142d78bdf5a8068b8f9a2859)]
    fun wallet_module_test(dr: signer, tc: signer, account: signer) {
        Genesis::setup(&dr, &tc);
        DiemAccount::create_parent_vasp_account<XUS>(&tc, signer::address_of(&account), x"355b32f571f894cfd431ab40dc950037", b"", false);

        let genesis_group = ApprovalGroup::create(x"1234", x"5678", x"abc123");
        ColdWallet::create(&account, genesis_group);
    }
}
