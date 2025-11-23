module 0x8675309::M {
    struct S { f: u64, g: u64 }
    fun id<T>(r: &T): &T {
        r
    }
    fun id_mut<T>(r: &mut T): &mut T {
        r
    }

    fun t0() {
        let x = &mut 0u64;
        let y = copy x;
        *y;
        *x;

        let x = &mut 0u64;
        let y = id_mut(x);
        *y;
        *x;

        let x = &0u64;
        let y = copy x;
        *y;
        *x;


        let x = &0u64;
        let y = id(x);
        *y;
        *x;
    }

}
