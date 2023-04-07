//# publish
module 0x42::m {

struct Foo has key {
    f1: bool,
    f2: address,
    f3: u8,
    f4: u16,
    f5: u32,
    f6: u64,
    f7: u128,
    f8: u256,
    v1: vector<bool>,
    v2: vector<address>,
    v3: vector<u8>,
    v4: vector<u16>,
    v5: vector<u32>,
    v6: vector<u64>,
    v7: vector<u128>,
    v8: vector<u256>,
}

entry fun mt(s: signer) {
    move_to(&s, Foo {
        f1: false,
        f2: @0,
        f3: 10,
        f4: 100,
        f5: 1000,
        f6: 10000,
        f7: 100000,
        f8: 1000000,
        v1: vector[false],
        v2: vector[@0],
        v3: vector[10],
        v4: vector[100],
        v5: vector[1000],
        v6: vector[10000],
        v7: vector[100000],
        v8: vector[1000000],
    })
}

entry fun mf(a: address) acquires Foo {
    let Foo {
        f1,
        f2,
        f3,
        f4,
        f5,
        f6,
        f7,
        f8,
        v1,
        v2,
        v3,
        v4,
        v5,
        v6,
        v7,
        v8,
    } = move_from(a);
    assert!(f1 == f1, 0);
    assert!(f2 == f2, 0);
    assert!(f3 == f3, 0);
    assert!(f4 == f4, 0);
    assert!(f5 == f5, 0);
    assert!(f6 == f6, 0);
    assert!(f7 == f7, 0);
    assert!(f8 == f8, 0);
    assert!(v1 == v1, 0);
    assert!(v2 == v2, 0);
    assert!(v3 == v3, 0);
    assert!(v4 == v4, 0);
    assert!(v5 == v5, 0);
    assert!(v6 == v6, 0);
    assert!(v7 == v7, 0);
    assert!(v8 == v8, 0);
    assert!(f1 == *std::vector::borrow(&v1, 0), 0);
    assert!(f2 == *std::vector::borrow(&v2, 0), 0);
    assert!(f3 == *std::vector::borrow(&v3, 0), 0);
    assert!(f4 == *std::vector::borrow(&v4, 0), 0);
    assert!(f5 == *std::vector::borrow(&v5, 0), 0);
    assert!(f6 == *std::vector::borrow(&v6, 0), 0);
    assert!(f7 == *std::vector::borrow(&v7, 0), 0);
    assert!(f8 == *std::vector::borrow(&v8, 0), 0);
}

}

//# run 0x42::m::mt --signers 0x1

//# run 0x42::m::mf --args @0x1
