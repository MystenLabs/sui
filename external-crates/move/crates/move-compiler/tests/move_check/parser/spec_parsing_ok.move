// These are some basic parsing tests for specifications which are expected to succeed.
// Full testing of spec parsing correctness is done outside of this crate.
//
// Note that even though we only test parsing, we still need to ensure that the move code (not the specification)
// is type checking correctly, because with no parsing error, the test harness
// will run subsequent phases of the move-compiler compiler.
//
// For parse failures, see the `spec_*_fail.move` test cases.

module 0x8675309::M {
    spec_block

    struct T has key { x: u64 }
    struct R has key { x: u64 }

    struct SomeCoin {
        x: u64,
        y: u64,
    }

    spec_block

    spec_block
    fun with_aborts_if(x: u64): u64 {
        x
    }

    spec_block
    fun with_ensures(x: u64): u64 {
        x + 1
    }

    spec_block
    fun using_block(x: u64): u64 {
        x + 1
    }

    spec_block
    fun using_lambda(x: u64): u64 {
        x
    }

    spec_block
    fun using_index_and_range(x: u64): u64 {
        x
    }

    spec_block
    fun using_implies(x: u64): u64 {
        x
    }

    spec_block
    fun with_emits<T: drop>(_guid: vector<u8>, _msg: T, x: u64): u64 {
        x
    }

    spec_block

    fun some_generic<T>() {}
    spec_block

    spec_block

    spec_block

    spec_block

    spec_block

    spec_block
}
