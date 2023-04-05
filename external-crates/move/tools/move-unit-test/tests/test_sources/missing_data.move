module 0x1::MissingData {
    struct Missing has key { }

    #[test]
    fun missing_data() acquires Missing {
        borrow_global<Missing>(@0x0);
    }

    #[test]
    fun missing_data_from_other_function() acquires Missing {
        // This call should create a stack trace entry
        missing_data()
    }

    #[test]
    #[expected_failure]
    fun missing_data_captured() acquires Missing {
        borrow_global<Missing>(@0x0);
    }

    #[test]
    #[expected_failure(major_status=4008, location=Self)]
    fun missing_data_exact() acquires Missing {
        borrow_global<Missing>(@0x0);
    }
}
