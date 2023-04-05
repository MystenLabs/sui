#[test_only]
module DiemFramework::RolesTests{
    use DiemFramework::Roles;
    use DiemFramework::Genesis;
    use std::unit_test;
    use std::vector;
    use std::signer;

    fun get_account(): signer {
        vector::pop_back(&mut unit_test::create_signers_for_testing(1))
    }

    #[test]
    #[expected_failure(abort_code = 2, location = DiemFramework::CoreAddresses)]
    fun grant_diem_root_wrong_addr_pre_genesis() {
        let account = get_account();
        Roles::grant_diem_root_role(&account);
    }

    #[test(tc = @TreasuryCompliance)]
    #[expected_failure(abort_code = 5, location = Roles)]
    fun tc_dne_pre_genesis(tc: signer) {
        Roles::assert_treasury_compliance(&tc);
    }

    #[test(dr = @DiemRoot)]
    #[expected_failure(abort_code = 5, location = Roles)]
    fun dr_dne_pre_genesis(dr: signer) {
        Roles::assert_diem_root(&dr);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun genesis_root_roles_exist(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        Roles::assert_diem_root(&dr);
        assert!(Roles::has_diem_root_role(&dr), 0);

        Roles::assert_treasury_compliance(&tc);
        assert!(Roles::has_treasury_compliance_role(&tc), 0);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 2, location = DiemFramework::CoreAddresses)]
    fun tc_is_not_dr(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        Roles::assert_diem_root(&tc);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 258, location = DiemFramework::CoreAddresses)]
    fun dr_is_not_tc(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        Roles::assert_treasury_compliance(&dr);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun grant_diem_root_wrong_addr_post_genesis(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::grant_diem_root_role(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun grant_diem_root_correct_addr_post_genesis(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        Roles::grant_diem_root_role(&dr);
    }

    #[test]
    #[expected_failure(abort_code = 258, location = DiemFramework::CoreAddresses)]
    fun grant_treasury_compliance_wrong_addr_pre_genesis() {
        let account = get_account();
        Roles::grant_treasury_compliance_role(&account, &account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun grant_treasury_compliance_wrong_addr_post_genesis(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::grant_treasury_compliance_role(&dr, &account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun grant_treasury_compliance_wrong_granting_addr_post_genesis(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        Roles::grant_treasury_compliance_role(&tc, &tc);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun grant_treasury_compliance_correct_addrs(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        Roles::grant_treasury_compliance_role(&dr, &tc);
    }

    #[test]
    #[expected_failure(abort_code = 5, location = Roles)]
    fun designated_dealer_role_dne() {
        let account = get_account();
        Roles::assert_designated_dealer(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1539, location = Roles)]
    fun designated_dealer_assert_wrong_role(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        Roles::assert_designated_dealer(&tc);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 258, location = DiemFramework::CoreAddresses)]
    fun grant_dd_role_non_tc_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::new_designated_dealer_role(&account, &account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun grant_dd_role_tc_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        assert!(!Roles::has_designated_dealer_role(&account), 0);
        Roles::new_designated_dealer_role(&tc, &account);
        assert!(Roles::has_designated_dealer_role(&account), 1);
        Roles::assert_designated_dealer(&account);
        Roles::assert_parent_vasp_or_designated_dealer(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 6, location = Roles)]
    fun double_grant_dd_role_tc_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::new_designated_dealer_role(&tc, &account);
        Roles::new_designated_dealer_role(&tc, &account);
    }

    #[test]
    #[expected_failure(abort_code = 5, location = Roles)]
    fun validator_role_dne() {
        let account = get_account();
        Roles::assert_validator(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1795, location = Roles)]
    fun validator_assert_wrong_role(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        Roles::assert_validator(&tc);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 2, location = DiemFramework::CoreAddresses)]
    fun grant_validator_role_non_dr_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::new_validator_role(&account, &account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun grant_validator_role_dr_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        assert!(!Roles::has_validator_role(&account), 0);
        Roles::new_validator_role(&dr, &account);
        assert!(Roles::has_validator_role(&account), 1);
        Roles::assert_validator(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 6, location = Roles)]
    fun double_grant_validator_role_dr_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::new_validator_role(&dr, &account);
        Roles::new_validator_role(&dr, &account);
    }

    #[test]
    #[expected_failure(abort_code = 5, location = Roles)]
    fun validator_operator_role_dne() {
        let account = get_account();
        Roles::assert_validator_operator(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 2051, location = Roles)]
    fun validator_operator_assert_wrong_role(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        Roles::assert_validator_operator(&tc);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 2, location = DiemFramework::CoreAddresses)]
    fun grant_validator_operator_role_non_dr_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::new_validator_operator_role(&account, &account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun grant_validator_operator_role_dr_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        assert!(!Roles::has_validator_operator_role(&account), 0);
        Roles::new_validator_operator_role(&dr, &account);
        assert!(Roles::has_validator_operator_role(&account), 1);
        Roles::assert_validator_operator(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 6, location = Roles)]
    fun double_grant_validator_operator_role_dr_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::new_validator_operator_role(&dr, &account);
        Roles::new_validator_operator_role(&dr, &account);
    }

    #[test]
    #[expected_failure(abort_code = 5, location = Roles)]
    fun parent_vasp_role_dne() {
        let account = get_account();
        Roles::assert_parent_vasp_role(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 258, location = DiemFramework::CoreAddresses)]
    fun grant_parent_vasp_role_non_tc_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::new_parent_vasp_role(&account, &account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun grant_parent_vasp_role_tc_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        assert!(!Roles::has_parent_VASP_role(&account), 0);
        Roles::new_parent_vasp_role(&tc, &account);
        assert!(Roles::has_parent_VASP_role(&account), 1);
        Roles::assert_parent_vasp_role(&account);
        Roles::assert_parent_vasp_or_designated_dealer(&account);
        Roles::assert_parent_vasp_or_child_vasp(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 6, location = Roles)]
    fun double_grant_parent_vasp_role_tc_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::new_parent_vasp_role(&tc, &account);
        Roles::new_parent_vasp_role(&tc, &account);
    }

    #[test]
    #[expected_failure(abort_code = 5, location = Roles)]
    fun child_vasp_role_dne() {
        let account = get_account();
        Roles::assert_child_vasp_role(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 2307, location = Roles)]
    fun child_vasp_assert_wrong_role(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::new_designated_dealer_role(&tc, &account);
        Roles::assert_child_vasp_role(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 771, location = Roles)]
    fun grant_child_vasp_role_non_parent_vasp_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let account = get_account();
        Roles::new_child_vasp_role(&tc, &account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun grant_child_vasp_role_parent_vasp_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let (account, pvasp) = {
            let accounts = unit_test::create_signers_for_testing(2);
            (vector::pop_back(&mut accounts), vector::pop_back(&mut accounts))
        };
        assert!(!Roles::has_child_VASP_role(&account), 0);
        Roles::new_parent_vasp_role(&tc, &pvasp);
        Roles::new_child_vasp_role(&pvasp, &account);
        assert!(Roles::has_child_VASP_role(&account), 1);
        Roles::assert_child_vasp_role(&account);
        Roles::assert_parent_vasp_or_child_vasp(&account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 6, location = Roles)]
    fun double_grant_child_vasp_role_tc_granter(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let (account, pvasp) = {
            let accounts = unit_test::create_signers_for_testing(2);
            (vector::pop_back(&mut accounts), vector::pop_back(&mut accounts))
        };
        Roles::new_parent_vasp_role(&tc, &pvasp);
        Roles::new_child_vasp_role(&pvasp, &account);
        Roles::new_child_vasp_role(&pvasp, &account);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun who_can_hold_balance(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let (dd_account, child_account, pvasp) = {
            let accounts = unit_test::create_signers_for_testing(3);
            (
                vector::pop_back(&mut accounts),
                vector::pop_back(&mut accounts),
                vector::pop_back(&mut accounts)
            )
        };

        Roles::new_parent_vasp_role(&tc, &pvasp);
        Roles::new_child_vasp_role(&pvasp, &child_account);
        Roles::new_designated_dealer_role(&tc, &dd_account);

        assert!(Roles::can_hold_balance(&dd_account), 0);
        assert!(Roles::can_hold_balance(&child_account), 1);
        assert!(Roles::can_hold_balance(&pvasp), 2);
    }


    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun role_ids(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let (validator_account, validator_operator_account, dd_account, child_account, pvasp) = {
            let accounts = unit_test::create_signers_for_testing(5);
            (
                vector::pop_back(&mut accounts),
                vector::pop_back(&mut accounts),
                vector::pop_back(&mut accounts),
                vector::pop_back(&mut accounts),
                vector::pop_back(&mut accounts),
            )
        };

        Roles::new_parent_vasp_role(&tc, &pvasp);
        Roles::new_child_vasp_role(&pvasp, &child_account);
        Roles::new_designated_dealer_role(&tc, &dd_account);
        Roles::new_validator_role(&dr, &validator_account);
        Roles::new_validator_operator_role(&dr, &validator_operator_account);

        assert!(Roles::get_role_id(signer::address_of(&dr)) == 0, 0);
        assert!(Roles::get_role_id(signer::address_of(&tc)) == 1, 1);
        assert!(Roles::get_role_id(signer::address_of(&dd_account)) == 2, 2);
        assert!(Roles::get_role_id(signer::address_of(&validator_account)) == 3, 3);
        assert!(Roles::get_role_id(signer::address_of(&validator_operator_account)) == 4, 4);
        assert!(Roles::get_role_id(signer::address_of(&pvasp)) == 5, 5);
        assert!(Roles::get_role_id(signer::address_of(&child_account)) == 6, 6);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 5, location = Roles)]
    fun get_role_id_no_role(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        Roles::get_role_id(@0x1);
    }

    #[test]
    #[expected_failure(abort_code = 5, location = Roles)]
    fun assert_parent_or_designated_dealer_role_dne() {
        let account = get_account();
        Roles::assert_parent_vasp_or_designated_dealer(&account);
    }

    #[test]
    #[expected_failure(abort_code = 5, location = Roles)]
    fun assert_parent_or_child_role_dne() {
        let account = get_account();
        Roles::assert_parent_vasp_or_child_vasp(&account);
    }
}
