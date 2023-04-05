#[test_only]
module DiemFramework::SharedEd25519PublicKeyTests {
    use DiemFramework::DiemAccount;
    use DiemFramework::Genesis;
    use DiemFramework::SharedEd25519PublicKey;
    use DiemFramework::XUS::XUS;

    use std::signer;

    fun setup(dr: &signer, tc: &signer, account: &signer) {
        let addr = signer::address_of(account);

        Genesis::setup(dr, tc);
        DiemAccount::create_parent_vasp_account<XUS>(tc, addr, x"0d3e1bd412376e933a0e794d65b41f97", b"", false);
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, account = @0x100)]
    fun publish_and_rotate_shared_key(dr: signer, tc: signer, account: signer) {
        let dr = &dr;
        let tc = &tc;
        let account = &account;

        let addr = signer::address_of(account);

        setup(dr, tc, account);

        let old_auth_key = DiemAccount::authentication_key(addr);
        // from RFC 8032
        let pubkey1 = x"3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c";
        SharedEd25519PublicKey::publish(account, copy pubkey1);
        let new_auth_key = DiemAccount::authentication_key(addr);

        // check that publishing worked
        assert!(SharedEd25519PublicKey::exists_at(addr), 3000);
        assert!(SharedEd25519PublicKey::key(addr) == pubkey1, 3001);

        // publishing should extract the sender's key rotation capability
        assert!(DiemAccount::delegated_key_rotation_capability(addr), 3002);
        // make sure the sender's auth key has changed
        assert!(copy new_auth_key != old_auth_key, 3003);

        // now rotate to another pubkey and redo the key-related checks
        // from RFC 8032
        let pubkey2 = x"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
        SharedEd25519PublicKey::rotate_key(account, copy pubkey2);
        assert!(SharedEd25519PublicKey::key(addr) == pubkey2, 3004);
        // make sure the auth key changed again
        assert!(new_auth_key != DiemAccount::authentication_key(addr), 3005);
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, account = @0x100)]
    #[expected_failure(abort_code = 261, location = SharedEd25519PublicKey)]
    fun get_key_for_non_shared_account_should_fail(dr: signer, tc: signer, account: signer) {
        setup(&dr, &tc, &account);

        SharedEd25519PublicKey::key(signer::address_of(&account));
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, account = @0x100)]
    #[expected_failure(abort_code = 7, location = SharedEd25519PublicKey)]
    fun publish_key_with_bad_length_1(dr: signer, tc: signer, account: signer) {
        setup(&dr, &tc, &account);

        let invalid_pubkey = x"0000";
        SharedEd25519PublicKey::publish(&account, invalid_pubkey);
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, account = @0x100)]
    #[expected_failure(abort_code = 7, location = SharedEd25519PublicKey)]
    fun publish_key_with_bad_length_2(dr: signer, tc: signer, account: signer) {
        setup(&dr, &tc, &account);

        let invalid_pubkey = x"10003d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c";
        SharedEd25519PublicKey::publish(&account, invalid_pubkey);
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, account = @0x100)]
    #[expected_failure(abort_code = 7, location = SharedEd25519PublicKey)]
    fun rotate_to_key_with_bad_length(dr: signer, tc: signer, account: signer) {
        setup(&dr, &tc, &account);

        // from RFC 8032
        let valid_pubkey =  x"3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c";
        SharedEd25519PublicKey::publish(&account, valid_pubkey);
        // now rotate to an invalid key
        let invalid_pubkey = x"010000";
        SharedEd25519PublicKey::rotate_key(&account, invalid_pubkey)
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, account = @0x100)]
    #[expected_failure(abort_code = 7, location = SharedEd25519PublicKey)]
    fun rotate_to_key_with_good_length_but_bad_contents(dr: signer, tc: signer, account: signer) {
        setup(&dr, &tc, &account);

        let valid_pubkey =  x"3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c";
        SharedEd25519PublicKey::publish(&account, valid_pubkey);
        // now rotate to an invalid key
        let invalid_pubkey = x"0000000000000000000000000000000000000000000000000000000000000000";
        SharedEd25519PublicKey::rotate_key(&account, invalid_pubkey)
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, account = @0x100)]
    #[expected_failure(abort_code = 261, location = SharedEd25519PublicKey)]
    fun rotate_key_on_non_shared_account(dr: signer, tc: signer, account: signer) {
        setup(&dr, &tc, &account);

        let invalid_pubkey = x"";
        SharedEd25519PublicKey::rotate_key(&account, invalid_pubkey);
    }
}
