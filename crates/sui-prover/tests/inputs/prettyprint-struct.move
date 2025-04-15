module 0x42::foo;

#[spec_only]
use prover::prover::ensures;

public struct MyStruct<T> has copy, drop {
    a: T,
    b: u64,
}

public fun foo(x: u64): MyStruct<u64> {
    MyStruct {
        a: x,
        b: 0,
    }
}

#[spec(prove)]
public fun foo_spec(x: u64): MyStruct<u64> {
    let res = foo(x);
    ensures(res.b != x);
    res
}
