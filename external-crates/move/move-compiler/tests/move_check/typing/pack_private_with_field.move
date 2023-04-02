module 0x42::m {

struct S {
    f1: u64,
    f2: u64,
    f3: u64,
    f4: u64,
}

public fun consume(_: S) { abort 0 }

}

module 0x42::n {

use 0x42::m::S;

fun f() {
  let s = S {
    f1: 0,
    f4: 0,
    f2: 0,
    f3: 0,
  };
  0x42::m::consume(s);
}

}
