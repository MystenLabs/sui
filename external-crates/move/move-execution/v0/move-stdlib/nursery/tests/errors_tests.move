#[test_only]
module std::errors_tests {
    use std::errors;

    #[test]
    fun errors_state() {
        assert!(errors::invalid_state(0) == 1, 0);
        assert!(errors::requires_address(0) == 2, 1);
        assert!(errors::requires_role(0) == 3, 2);
        assert!(errors::not_published(0) == 5, 4);
        assert!(errors::already_published(0) == 6, 5);
        assert!(errors::invalid_argument(0) == 7, 6);
        assert!(errors::limit_exceeded(0) == 8, 7);
        assert!(errors::internal(0) == 10, 8);
        assert!(errors::custom(0) == 255, 9);
    }

}
