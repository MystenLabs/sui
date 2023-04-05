#[test_only]
module DiemFramework::CRSNTests {
    use DiemFramework::CRSN;
    use DiemFramework::Genesis;
    use std::signer;
    use std::bit_vector;

    #[test_only]
    public fun setup(dr: &signer, tc: &signer, _: &signer) {
        Genesis::setup(dr, tc);
        CRSN::allow_crsns(dr)
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1281, location = CRSN)]
    public fun cant_publish_until_init(a: signer, tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        CRSN::test_publish(&a, 0, 10);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1537, location = CRSN)]
    public fun double_init(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        CRSN::allow_crsns(&dr);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun publish_exists_after_small_size(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        let addr = signer::address_of(&a);

        CRSN::test_publish(&a, 0, 10);
        assert!(CRSN::has_crsn(addr), 0);
        assert!(CRSN::min_nonce(addr)  == 0, 1);
        assert!(bit_vector::length(&CRSN::slots(addr)) == 10, 2);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun publish_exists_after_medium_size(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        let addr = signer::address_of(&a);

        CRSN::test_publish(&a, 20, 128);
        assert!(CRSN::has_crsn(addr), 0);
        assert!(CRSN::min_nonce(addr) == 20, 1);
        assert!(bit_vector::length(&CRSN::slots(addr)) == 128, 2);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun publish_exists_after_large_size(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        let addr = signer::address_of(&a);

        CRSN::test_publish(&a, 505, CRSN::max_crsn_size());
        assert!(CRSN::has_crsn(addr), 0);
        assert!(CRSN::min_nonce(addr) == 505, 1);
        assert!(bit_vector::length(&CRSN::slots(addr)) == CRSN::max_crsn_size(), 2);
    }


    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 257, location = CRSN)]
    public fun double_publish(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        CRSN::test_publish(&a, 0, 10);
        CRSN::test_publish(&a, 0, 10);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 519, location = CRSN)]
    public fun publish_zero_size(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        CRSN::test_publish(&a, 10, 0);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun publish_at_max_size(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        CRSN::test_publish(&a, 10, CRSN::max_crsn_size());
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 775, location = CRSN)]
    public fun publish_above_max_size(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        CRSN::test_publish(&a, 10, CRSN::max_crsn_size() + 1);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun test_has_crsn(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        let addr = signer::address_of(&a);
        assert!(!CRSN::has_crsn(addr), 0);
        CRSN::test_publish(&a, 0, 10);
        assert!(CRSN::has_crsn(addr), 1);
    }

    #[test(a=@0xCAFE)]
    #[expected_failure(abort_code = 1, location = CRSN)]
    public fun record_no_crsn(a: signer) {
        CRSN::test_record(&a, 0);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun record_too_high_low_accept(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        CRSN::test_publish(&a, 100, 10);
        assert!(!CRSN::test_record(&a, 110), 0);
        assert!(!CRSN::test_record(&a, 111), 1);
        // We allow recording in the past
        assert!(CRSN::test_record(&a, 99), 2);
        // But the check will fail since that happens in the prologue
        assert!(!CRSN::test_check(&a, 99), 2);
        assert!(CRSN::test_record(&a, 100), 3);
        assert!(CRSN::test_record(&a, 109), 4);
        assert!(!CRSN::test_record(&a, 109), 5);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun prevent_replay(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        CRSN::test_publish(&a, 100, 10);
        assert!(CRSN::test_record(&a, 101), 0);
        assert!(!CRSN::test_record(&a, 101), 1);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun prevent_replay_with_shift(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        CRSN::test_publish(&a, 100, 10);
        assert!(CRSN::test_record(&a, 100), 0);
        assert!(!CRSN::test_check(&a, 100), 1);
        assert!(CRSN::test_record(&a, 100), 1);
        assert!(CRSN::min_nonce(signer::address_of(&a)) == 101, 2);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun multiple_shifts_of_window(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        let addr = signer::address_of(&a);
        CRSN::test_publish(&a, 100, 10);
        assert!(CRSN::test_record(&a, 101), 0);
        assert!(CRSN::test_record(&a, 102), 0);
        assert!(CRSN::test_record(&a, 103), 0);

        assert!(CRSN::test_record(&a, 106), 0);
        assert!(CRSN::test_record(&a, 107), 0);
        assert!(CRSN::test_record(&a, 108), 0);

        // The window should not have shifted
        assert!(CRSN::min_nonce(addr) == 100, 1);
        assert!(CRSN::test_record(&a, 100), 0);
        // The window should now shift until it gets stuck on 104
        assert!(CRSN::min_nonce(addr) == 104, 1);
        assert!(CRSN::test_record(&a, 104), 0);
        assert!(CRSN::min_nonce(addr) == 105, 1);
        assert!(CRSN::test_record(&a, 105), 0);
        assert!(CRSN::min_nonce(addr) == 109, 1);

        // Now make sure that the window has shifted and opened-up higher slots
        assert!(CRSN::test_record(&a, 110), 0);
        assert!(CRSN::test_record(&a, 111), 0);
        assert!(CRSN::test_record(&a, 112), 0);
        assert!(CRSN::test_record(&a, 113), 0);
        assert!(CRSN::test_record(&a, 114), 0);
        assert!(CRSN::test_record(&a, 115), 0);
        assert!(CRSN::min_nonce(addr) == 109, 1);
        assert!(CRSN::test_record(&a, 109), 0);
        assert!(CRSN::min_nonce(addr) == 116, 1);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 1031, location = CRSN)]
    public fun force_expire_zero(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        CRSN::test_publish(&a, 100, 10);
        CRSN::test_force_expire(&a, 0);
    }


    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun force_expire_single(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        let addr = signer::address_of(&a);
        CRSN::test_publish(&a, 100, 10);
        CRSN::test_force_expire(&a, 1);
        assert!(CRSN::min_nonce(addr) == 101, 1);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun force_expire_shift_over_set_bits(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        let addr = signer::address_of(&a);
        CRSN::test_publish(&a, 0, 100);
        assert!(CRSN::test_record(&a, 1), 0);
        assert!(CRSN::test_record(&a, 2), 0);
        assert!(CRSN::test_record(&a, 3), 0);
        CRSN::test_force_expire(&a, 1);
        assert!(CRSN::min_nonce(addr) == 4, 1);
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun force_expire_past_set_bits(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        let addr = signer::address_of(&a);
        CRSN::test_publish(&a, 0, 100);
        assert!(CRSN::test_record(&a, 1), 0);
        assert!(CRSN::test_record(&a, 2), 0);
        assert!(CRSN::test_record(&a, 3), 0);
        CRSN::test_force_expire(&a, 15);
        assert!(CRSN::min_nonce(addr) == 15, 1);
        let i = 0;
        let len = 100;
        let slots = CRSN::slots(addr);

        while (i < len) {
            assert!(!bit_vector::is_index_set(&slots, i), 2);
            i = i + 1;
        }
    }

    #[test(a=@0xCAFE, tc = @TreasuryCompliance, dr = @DiemRoot)]
    public fun force_expire_past_window_size(a: signer, tc: signer, dr: signer) {
        setup(&dr, &tc, &a);
        let addr = signer::address_of(&a);
        CRSN::test_publish(&a, 0, 100);
        assert!(CRSN::test_record(&a, 1), 0);
        assert!(CRSN::test_record(&a, 2), 0);
        assert!(CRSN::test_record(&a, 3), 0);
        CRSN::test_force_expire(&a, 10000);
        assert!(CRSN::min_nonce(addr) == 10000, 1);
        let i = 0;
        let len = 100;
        let slots = CRSN::slots(addr);

        while (i < len) {
            assert!(!bit_vector::is_index_set(&slots, i), 2);
            i = i + 1;
        }
    }
}
