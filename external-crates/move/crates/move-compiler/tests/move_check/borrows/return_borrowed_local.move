module 0x8675309::M {
    struct S has copy, drop { f: u64 }
    fun id<T>(r: &T): &T {
        r
    }
    fun id_mut<T>(r: &mut T): &mut T {
        r
    }
    fun t0() {
        let v1 = 0u64;
        let v2 = 0u64;
        let v3 = 0u64;
        let v4 = 0u64;
        let s1 = S { f: 0 };
        let s2 = S { f: 0 };
        let s3 = S { f: 0 };
        let s4 = S { f: 0 };

        &mut v1;
        &v2;
        id_mut(&mut v3);
        id(&v4);
        &mut s1.f;
        &s2.f;
        id_mut(&mut s3.f);
        id(&s4.f);
    }
}
