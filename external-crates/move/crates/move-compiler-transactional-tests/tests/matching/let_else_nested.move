//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Outer has drop {
        Some(Inner),
        None,
    }

    public enum Inner has drop {
        Val(u64),
        Empty,
    }

    public fun some_val(x: u64): Outer { Outer::Some(Inner::Val(x)) }
    public fun some_empty(): Outer { Outer::Some(Inner::Empty) }
    public fun none(): Outer { Outer::None }

    public fun extract_nested(o: Outer): u64 {
        let Outer::Some(Inner::Val(x)) = o else { return 0 };
        x
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{extract_nested, some_val, some_empty, none};

        assert!(extract_nested(some_val(42)) == 42, 1);
        assert!(extract_nested(some_empty()) == 0, 2);
        assert!(extract_nested(none()) == 0, 3);
    }
}
