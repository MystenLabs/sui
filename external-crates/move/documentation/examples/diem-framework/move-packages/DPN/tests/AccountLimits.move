#[test_only]
module DiemFramework::AccountLimitsTests {
    use std::signer;
    use DiemFramework::Genesis;
    use DiemFramework::AccountLimits;
    use DiemFramework::XUS::XUS;
    use DiemFramework::Roles;

    struct Hold<T> has key { x: T }
    public fun hold<T: store>(account: &signer, x: T) {
        move_to(account, Hold<T>{ x })
    }

    fun setup(dr: &signer, tc: &signer, vasp: &signer) {
        Genesis::setup(dr, tc);

        Roles::new_parent_vasp_role(tc, vasp);

        AccountLimits::publish_unrestricted_limits_for_testing<XUS>(vasp);
        AccountLimits::publish_window<XUS>(dr, vasp, signer::address_of(vasp));
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    #[expected_failure(abort_code = 1, location = DiemFramework::DiemTimestamp)]
    fun grant_mutation_capability_after_genesis(dr: signer, tc: signer, vasp: signer) {
        Genesis::setup(&dr, &tc);

        hold(&vasp, AccountLimits::grant_mutation_capability(&vasp));
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    fun publish_window(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    #[expected_failure(abort_code = 262, location = AccountLimits)]
    fun publish_window_twice(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);

        AccountLimits::publish_window<XUS>(&dr, &vasp, signer::address_of(&vasp));
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    #[expected_failure(abort_code = 2, location = DiemFramework::CoreAddresses)]
    fun publish_window_non_diem_root(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
        AccountLimits::publish_window<XUS>(&vasp, &vasp, signer::address_of(&vasp));
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    #[expected_failure(abort_code = 5, location = AccountLimits)]
    fun publish_window_non_existent_limit_address(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
        AccountLimits::publish_window<XUS>(&dr, &vasp, @0x42 /* non-exsistent */);
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    #[expected_failure(abort_code = 6, location = AccountLimits)]
    fun publish_unrestricted_limits_for_testing_twice(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
        AccountLimits::publish_unrestricted_limits_for_testing<XUS>(&vasp);
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    fun update_limits_definition_1(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
        AccountLimits::update_limits_definition<XUS>(
            &tc,
            signer::address_of(&vasp),
            100, /* new_max_inflow */
            200, /* new_max_outflow */
            150, /* new_max_holding_balance */
            10000, /* new_time_period */
        )
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    fun update_limits_definition_2(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
        AccountLimits::update_limits_definition<XUS>(
            &tc,
            signer::address_of(&vasp),
            0, /* new_max_inflow */
            0, /* new_max_outflow */
            150, /* new_max_holding_balance */
            10000, /* new_time_period */
        )
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    fun update_limits_definition_twice(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
        AccountLimits::update_limits_definition<XUS>(
            &tc,
            signer::address_of(&vasp),
            100, /* new_max_inflow */
            200, /* new_max_outflow */
            150, /* new_max_holding_balance */
            10000, /* new_time_period */
        );
        AccountLimits::update_limits_definition<XUS>(
            &tc,
            signer::address_of(&vasp),
            0, /* new_max_inflow */
            0, /* new_max_outflow */
            150, /* new_max_holding_balance */
            10000, /* new_time_period */
        )
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    #[expected_failure(abort_code = 258, location = DiemFramework::CoreAddresses)]
    fun update_limits_definition_non_tc(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
        AccountLimits::update_limits_definition<XUS>(
            &dr,
            signer::address_of(&vasp),
            100, /* new_max_inflow */
            200, /* new_max_outflow */
            150, /* new_max_holding_balance */
            10000, /* new_time_period */
        )
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    #[expected_failure(abort_code = 5, location = AccountLimits)]
    fun update_limits_definition_non_exsistent(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
        AccountLimits::update_limits_definition<XUS>(
            &tc,
            @0x42, // non-exsistent
            100, /* new_max_inflow */
            200, /* new_max_outflow */
            150, /* new_max_holding_balance */
            10000, /* new_time_period */
        )
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    fun update_window_info(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
        let vasp_addr = signer::address_of(&vasp);
        AccountLimits::update_window_info<XUS>(
            &tc,
            vasp_addr,
            120,
            vasp_addr,
        );
        AccountLimits::update_window_info<XUS>(
            &tc,
            vasp_addr,
            0,
            vasp_addr,
        );
        AccountLimits::update_window_info<XUS>(
            &tc,
            vasp_addr,
            120,
            vasp_addr,
        );
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    #[expected_failure(abort_code = 258, location = DiemFramework::CoreAddresses)]
    fun update_window_info_non_tc(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
        let vasp_addr = signer::address_of(&vasp);
        AccountLimits::update_window_info<XUS>(
            &dr,
            vasp_addr,
            120,
            vasp_addr,
        );
    }

    #[test(dr = @DiemRoot, tc = @TreasuryCompliance, vasp = @0x2)]
    fun has_limits_published(dr: signer, tc: signer, vasp: signer) {
        setup(&dr, &tc, &vasp);
        assert!(AccountLimits::has_limits_published<XUS>(signer::address_of(&vasp)), 1);
        assert!(!AccountLimits::has_limits_published<XUS>(@0x42 /* non-exsistent */), 3);
    }
}
