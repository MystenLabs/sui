#[test_only]
module DiemFramework::AccountFreezingTests {
    use DiemFramework::AccountFreezing as AF;
    use DiemFramework::Genesis;
    use std::signer;

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun account_freezing_double_init(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        AF::initialize(&dr);
    }

    #[test]
    fun non_existent_account_not_frozen() {
        assert!(!AF::account_is_frozen(@0x2), 0);
        AF::assert_not_frozen(@0x2);
    }

    #[test(a = @0x2)]
    fun create_new(a: signer) {
        let a_addr = signer::address_of(&a);
        AF::create_for_test(&a);
        AF::assert_not_frozen(a_addr);
    }

    #[test(a = @0x2)]
    #[expected_failure(abort_code = 518, location = AF)]
    fun create_new_already_has_freezing_bit(a: signer) {
        AF::create_for_test(&a);
        AF::create_for_test(&a);
    }

    #[test(a = @0x2)]
    fun create_new_not_frozen(a: signer) {
        let a_addr = signer::address_of(&a);
        AF::create_for_test(&a);
        AF::assert_not_frozen(a_addr);
    }

    #[test(a = @0x2, tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 258, location = DiemFramework::CoreAddresses)]
    fun freeze_account_not_tc(a: signer, tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        AF::create_for_test(&a);
        AF::freeze_account(&a, @0x2);
    }

    #[test(tc = @TreasuryCompliance, a = @0x2)]
    #[expected_failure(abort_code = 257, location = DiemFramework::DiemTimestamp)]
    fun freeze_account_not_operating(tc: signer, a: signer) {
        AF::create_for_test(&a);
        AF::freeze_account(&tc, @0x2);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 775, location = AF)]
    fun cannot_freeze_diem_root(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        AF::freeze_account(&tc, @DiemRoot);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1031, location = AF)]
    fun cannot_freeze_treasury_compliance(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        AF::freeze_account(&tc, @TreasuryCompliance);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 517, location = AF)]
    fun freeze_no_freezing_bit(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        AF::freeze_account(&tc, @0x2);
    }

    #[test(a = @0x2, tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun account_frozen_after_freeze(a: signer, tc: signer, dr: signer) {
        let a_addr = signer::address_of(&a);
        Genesis::setup(&dr, &tc);
        AF::create_for_test(&a);
        AF::assert_not_frozen(a_addr);
        AF::freeze_account(&tc, @0x2);
        assert!(AF::account_is_frozen(a_addr), 0);
    }

    #[test(tc = @TreasuryCompliance, a = @0x2)]
    #[expected_failure(abort_code = 257, location = DiemFramework::DiemTimestamp)]
    fun unfreeze_account_not_operating(tc: signer, a: signer) {
        AF::create_for_test(&a);
        AF::unfreeze_account(&tc, @0x2);
    }

    #[test(a = @0x2, tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 258, location = DiemFramework::CoreAddresses)]
    fun unfreeze_account_not_tc(a: signer, tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        AF::create_for_test(&a);
        AF::unfreeze_account(&a, @0x2);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 517, location = AF)]
    fun unfreeze_no_freezing_bit(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        AF::unfreeze_account(&tc, @0x2);
    }

    #[test(a = @0x2, tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun account_unfrozen_after_unfreeze(a: signer, tc: signer, dr: signer) {
        let a_addr = signer::address_of(&a);
        Genesis::setup(&dr, &tc);

        AF::create_for_test(&a);
        AF::assert_not_frozen(a_addr);
        AF::freeze_account(&tc, @0x2);

        assert!(AF::account_is_frozen(a_addr), 0);
        AF::unfreeze_account(&tc, @0x2);
        AF::assert_not_frozen(a_addr);
    }
}
