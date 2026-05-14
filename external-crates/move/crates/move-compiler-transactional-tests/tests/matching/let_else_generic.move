// Runtime check: `let ... else` inside a generic function body. The pattern
// names a generic constructor; binders are typed by the instantiation.
//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Option<T> has drop {
        Some(T),
        None,
    }

    public fun some<T>(t: T): Option<T> { Option::Some(t) }
    public fun none<T>(): Option<T> { Option::None }

    public fun unwrap_or_default<T: drop>(opt: Option<T>, default: T): T {
        let Option::Some(val) = opt else { return default };
        val
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{some, none, unwrap_or_default};

        // u64 instantiation
        assert!(unwrap_or_default(some(42u64), 0) == 42, 1);
        assert!(unwrap_or_default(none<u64>(), 7) == 7, 2);

        // bool instantiation
        assert!(unwrap_or_default(some(true), false) == true, 3);
        assert!(unwrap_or_default(none<bool>(), true) == true, 4);
    }
}
