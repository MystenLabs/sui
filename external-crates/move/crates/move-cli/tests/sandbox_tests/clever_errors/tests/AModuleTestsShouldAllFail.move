// All tests in this module should fail to pass (marked
// as test failures).
#[test_only]
module std::AModuleTestsShouldAllFail {
    use std::AModule;

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
    #[expected_failure(abort_code = std::AModule::EIsThree, location = Self)]
    fun double_three_location_based_invalid() {
        AModule::double_except_three(3);
    }

    #[test]
    #[expected_failure(abort_code = std::BModule::EIsThree)]
    fun double_three_const_based_different_module_fail() {
        AModule::double_except_three(3);
    }

    #[test]
    fun abort_in_macro() {
        AModule::abort_!();
    }

    #[test]
    fun clever_error_line_abort_in_non_macro() {
        assert!(false);
    }
}
