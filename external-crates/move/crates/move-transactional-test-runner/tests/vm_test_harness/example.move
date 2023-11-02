//# init --addresses A=0x42

//# run

script {
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

script {
    fun main(v: u64) {
        A::N::take(A::N::make(v));
    }
}

//# run --args 42 --syntax=mvir

import 0x42.N;
main(v: u64) {
label b0:
    _ = N.take(N.make(move(v)));
    return;
}

//# run 0x42::N::make --args 42

//# run 0x42::N::take --args struct(42)
