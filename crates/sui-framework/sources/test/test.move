#[test_only]
module sui::test {

    public fun assert_eq<T: drop>(t1: T, t2: T) {
        assert_ref_eq(&t1, &t2)
    }

    public fun assert_ref_eq<T>(t1: &T, t2: &T) {
        let res = t1 == t2;
        if (!res) {
            print(b"Assertion failed:");
            std::debug::print(t1);
            print(b"!=");
            std::debug::print(t2);
            abort(0)
        }
    }

    public fun print(str: vector<u8>) {
        std::debug::print(&std::ascii::string(str))
    }
}