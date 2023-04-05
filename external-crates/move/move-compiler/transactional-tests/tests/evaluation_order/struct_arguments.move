//# publish
module 0x42::M {
    struct S has drop {
        a: u64,
        b: u64,
    }

    struct R has key, store {}
    struct Cup has key {
        a: u64,
        b: R,
    }

    public fun t0() {
        S { b: 1 / 0, a: fail(0) };
    }

    public fun t1() {
        S { b: 18446744073709551615 + 18446744073709551615, a: fail(0) };
    }

    public fun t2() {
        S { b: 0 - 1, a: fail(0) };
    }

    public fun t3() {
        S { b: 1 % 0, a: fail(0) };
    }

    public fun t4() {
        S { b: 18446744073709551615 * 18446744073709551615, a: fail(0) };
    }

    public fun t5(account: &signer) acquires R {
        move_to(account, Cup { b: move_from(@0x0), a: fail(0) });
    }

    public fun t6(account: &signer) {
        move_to(account, Cup { b: R{}, a: 0 });
        S { b: mts(account), a: fail(0) };
    }

    fun fail(code: u64): u64 {
        abort code
    }

    fun mts(account: &signer): u64 {
        move_to(account, Cup { b: R{}, a: 0 });
        0
    }
}

//# run
script {
use 0x42::M;
fun main() {
  // arithmetic error
  M::t0()
}
}

//# run
script {
use 0x42::M;
fun main() {
  // arithmetic error
  M::t1()
}
}

//# run
script {
use 0x42::M;
fun main() {
  // arithmetic error
  M::t2()
}
}

//# run
script {
use 0x42::M;
fun main() {
  // arithmetic error
  M::t3()
}
}

//# run
script {
use 0x42::M;
fun main() {
  // arithmetic error
  M::t4()
}
}

//# run --signers 0x1
script {
use 0x42::M;
fun main(account: signer) {
  // missing data
  M::t5(&account)
}
}

//# run --signers 0x1
script {
use 0x42::M;
fun main(account: signer) {
  // resource already exists
  M::t6(&account)
}
}
