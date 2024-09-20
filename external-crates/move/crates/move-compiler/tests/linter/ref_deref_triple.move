module 0x42::m {
    struct S has copy, drop {}

    public fun test(s: S)  {
        let _x = &*&*&s;
    }
}
