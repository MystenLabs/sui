//# init --addresses A=0x42

//# run

module 0x5::m {
    fun main() {}
}

//# publish
module A::N {
    struct R<V: store> has key {
        v: V
    }

    public fun make(v: u64): R<u64> {
        R { v }
    }

    public fun take(r: R<u64>): u64 {
        let R { v } = r;
        v
    }

    public entry fun ex(_s: signer, _u: u64) {
        abort 0
    }
}

//# run --signers 0x1 --args 0 -- 0x42::N::ex

//# run --args 0

module 0x6::m {
    entry fun main(v: u64) {
        helper(v)
    }
    fun helper(v: u64) {
        A::N::take(A::N::make(v));
    }
}

//# run --args 42 --syntax=mvir

module 0x7.m {
import 0x42.N;
entry foo(v: u64) {
label b0:
    _ = N.take(N.make(move(v)));
    return;
}
}

//# run 0x42::N::make --args 42

//# run 0x42::N::take --args struct(42)
