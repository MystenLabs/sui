#[test_only]
module DiemFramework::ValidatorOperatorConfigTests {
    use DiemFramework::ValidatorOperatorConfig as VOC;
    use DiemFramework::Roles;
    use DiemFramework::Genesis;
    use std::unit_test;
    use std::vector;
    use std::signer;

    fun get_signer(): signer {
        vector::pop_back(&mut unit_test::create_signers_for_testing(1))
    }

    #[test]
    #[expected_failure(abort_code = 257, location = DiemFramework::DiemTimestamp)]
    fun publish_pre_genesis() {
        let s = get_signer();
        VOC::publish(&s, &s, x"");
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 2, location = DiemFramework::CoreAddresses)]
    fun publish_post_genesis_non_dr(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let s = get_signer();
        VOC::publish(&s, &tc, x"");
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 5, location = DiemFramework::Roles)]
    fun publish_post_genesis_non_validator_operator(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let s = get_signer();
        VOC::publish(&s, &dr, x"");
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun publish_post_genesis(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let s = get_signer();
        Roles::new_validator_operator_role(&dr, &s);
        VOC::publish(&s, &dr, x"");
        assert!(VOC::has_validator_operator_config(signer::address_of(&s)), 0);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    #[expected_failure(abort_code = 6, location = VOC)]
    fun publish_post_genesis_double_publish(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let s = get_signer();
        Roles::new_validator_operator_role(&dr, &s);
        VOC::publish(&s, &dr, x"");
        VOC::publish(&s, &dr, x"");
    }

    #[test]
    #[expected_failure(abort_code = 5, location = VOC)]
    fun get_human_name_not_validator_operator() {
        VOC::get_human_name(@0x1);
    }

    #[test(tc = @TreasuryCompliance, dr = @DiemRoot)]
    fun get_human_name(tc: signer, dr: signer) {
        Genesis::setup(&dr, &tc);
        let s = get_signer();
        Roles::new_validator_operator_role(&dr, &s);
        VOC::publish(&s, &dr, b"test");
        assert!(VOC::get_human_name(signer::address_of(&s)) == b"test", 0);
    }
}
