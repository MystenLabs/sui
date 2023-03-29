module 0x2::A {
    use std::vector;

    struct Foo has drop {}
    struct Bar {}

    fun test_natives<T>(x1: T, x2: T): (T, T) {
        let v: vector<T> = vector::empty();
        assert!(vector::length(&v) == 0, 100);
        vector::push_back(&mut v, x1);
        assert!(vector::length(&v) == 1, 101);
        vector::push_back(&mut v, x2);
        assert!(vector::length(&v) == 2, 102);
        vector::swap(&mut v, 0, 1);
        x1 = vector::pop_back(&mut v);
        assert!(vector::length(&v) == 1, 103);
        x2 = vector::pop_back(&mut v);
        assert!(vector::length(&v) == 0, 104);
        vector::destroy_empty(v);
        (x1, x2)
    }

    #[test]
    public fun vector_ops() {
        test_natives<u8>(1u8, 2u8);
        test_natives<u64>(1u64, 2u64);
        test_natives<u128>(1u128, 2u128);
        test_natives<bool>(true, false);
        test_natives<address>(@0x1, @0x2);

        test_natives<vector<u8>>(vector::empty(), vector::empty());
        test_natives<Foo>(Foo {}, Foo {});
        (Bar {}, Bar {}) = test_natives<Bar>(Bar {}, Bar {});
    }
}
