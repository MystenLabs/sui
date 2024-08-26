module 0x42::m;

#[allow(lint(unneeded_return))]
fun t0(): u64 { return 5 }

#[allow(lint(unneeded_return))]
fun t1(): u64 { return t0() }

public struct S {  }

#[allow(lint(unneeded_return))]
fun t2(): S { return S { } }

public enum E { V }

#[allow(lint(unneeded_return))]
fun t3(): E { return E::V }

#[allow(lint(unneeded_return))]
fun t4(): vector<u64> { return vector[1,2,3] }

#[allow(lint(unneeded_return))]
fun t5() { return () }

#[allow(lint(unneeded_return))]
fun t6(): u64 { let x = 0; return move x
}

#[allow(lint(unneeded_return))]
fun t7(): u64 {
    let x = 0;
    return copy x
}

const VALUE: u64 = 0;

#[allow(lint(unneeded_return))]
fun t8(): u64 { return VALUE }

#[allow(lint(unneeded_return))]
fun t9(): u64 { return 5 + 2 }

#[allow(lint(unneeded_return))]
fun t10(): bool { return !true }

#[allow(lint(unneeded_return))]
fun t11(x: &u64): u64 { return *x }

#[allow(lint(unneeded_return))]
fun t12(x: u64): u128 { return x as u128 }

#[allow(lint(unneeded_return))]
fun t13(): u64 { return (0: u64) }

#[allow(lint(unneeded_return))]
fun t14(): u64 { return loop { break 5 } }
