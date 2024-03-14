#[test_only]
module std::AModuleTests {
    use std::AModule;

    #[test]
    fun double_zero_zero() {
        assert!(AModule::double_except_three(0) == 0, 0)
    }

    #[test]
    #[expected_failure(abort_code = std::AModule::EIsThree)]
    fun double_three() {
        AModule::double_except_three(3);
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun double_three_should_fail() {
        AModule::double_except_three(3);
    }

    #[test]
    #[expected_failure(abort_code = std::AModule::ENotFound)]
    fun double_three_should_fail_named_const() {
        AModule::double_except_three(3);
    }

    #[test]
    #[expected_failure(abort_code = std::AModule::EIsThree, location = std::AModule)]
    fun double_three_location_based_valid() {
        AModule::double_except_three(3);
    }

    #[test]
    #[expected_failure(abort_code = std::AModule::EIsThree, location = Self)]
    fun double_three_location_based_invalid() {
        AModule::double_except_three(3);
    }

    #[test]
    #[expected_failure(abort_code = std::BModule::EIsThree)]
    fun double_three_const_based_different_module_fail() {
        // NB: there could be a case for confusion between error consts in
        // different modules with the same const name -- but this will
        // generally be quite rare since it would require const indexes to line
        // up perfectly. However, it is still possible, so using this in
        // conjunction with a location would be the best.
        AModule::double_except_three(3);
    }

    #[test]
    fun double_one_one() {
        assert!(AModule::double_except_three(1) == 1, 0)
    }
}
