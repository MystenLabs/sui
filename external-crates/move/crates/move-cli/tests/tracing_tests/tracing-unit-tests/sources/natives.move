module 0x1::natives {
    public struct X() has drop;

    #[test]
    fun get_type_name_test() {
        let x = std::type_name::get<X>();
        let _t = x.borrow_string();
        let _t = x.into_string();
    }

    #[test]
    fun get_orig_type_name_test() {
        let x = std::type_name::get_with_original_ids<X>();
        let _t = x.borrow_string();
        let _t = x.into_string();
    }
}
