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
    #[expected_failure(abort_code = std::AModule::EIsThree, location = std::AModule)]
    fun double_three_location_based_valid() {
        AModule::double_except_three(3);
    }

    #[test]
    fun double_one_one() {
        assert!(AModule::double_except_three(1) == 1, 0)
    }
}
