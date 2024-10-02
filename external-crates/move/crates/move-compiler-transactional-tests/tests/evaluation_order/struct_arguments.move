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

    fun fail(code: u64): u64 {
        abort code
    }

}

//# run
module 6::m {
use 0x42::M;
fun main() {
  // arithmetic error
  M::t0()
}
}

//# run
module 7::m {
use 0x42::M;
fun main() {
  // arithmetic error
  M::t1()
}
}

//# run
module 8::m {
use 0x42::M;
fun main() {
  // arithmetic error
  M::t2()
}
}

//# run
module 9::m {
use 0x42::M;
fun main() {
  // arithmetic error
  M::t3()
}
}

//# run
module 0xa::m {
use 0x42::M;
fun main() {
  // arithmetic error
  M::t4()
}
}
