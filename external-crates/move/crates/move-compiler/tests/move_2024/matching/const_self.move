module 0x42::m {

    const A: u64 = 10;
    const B: u64 = 20;

    fun test0(): u64 {
        match (A) {
            A => 0,
            B => 1,
            _ => 2,
        }
    }

    #[allow(implicit_const_copy)]
    fun test1(): u64 {
        match (&A) {
            A => 0,
            B => 1,
            _ => 2,
        }
    }

    #[allow(implicit_const_copy)]
    fun test2(): u64 {
        match (&mut A) {
            A => 0,
            B => 1,
            _ => 2,
        }
    }

}
