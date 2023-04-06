module 0x1::CopyStruct {
    struct S has copy, drop { a: address }

    struct G has key { f: u64 }

    public fun ret_struct(a: address): S {
        S { a }
    }

    // returning a copy of S should behave the same as returning S
    public fun ret_struct_copy(a: address): S {
        let s = S { a };
        *&s
    }

    public fun g() acquires G {
        let s1 = ret_struct_copy(@0x7);
        borrow_global_mut<G>(s1.a).f = 1;

        let s2 = ret_struct(@0x7);
        borrow_global_mut<G>(s2.a).f = 2;
    }
}
